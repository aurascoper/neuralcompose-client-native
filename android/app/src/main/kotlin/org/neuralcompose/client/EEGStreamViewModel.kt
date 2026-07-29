// The shell side of the contract (port of ios/.../EEGStreamModel.swift):
// owns the socket and timers, feeds raw frames + MONOTONIC timestamps into
// the Rust core, and renders whatever the core says. It never derives stream
// health from socket state — M5-A semantics: a reopened socket is
// OpenNoData until its own first accepted frame.
//
// It ALSO owns the golden-capture recorder (Muse capture gate). Every frame
// goes to the live monitor first, unchanged, and then — only while a capture
// is active — to `CaptureRecorder.onMessage`, whose returned line is appended
// verbatim. EEG is never parsed here; the envelope is Rust's.

package org.neuralcompose.client

import android.app.Application
import android.content.Context
import android.os.Build
import android.os.SystemClock
import android.security.NetworkSecurityPolicy
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import java.net.URI
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import uniffi.neuralcompose_mobile_core.BridgeLocality
import uniffi.neuralcompose_mobile_core.CaptureBuildIdentity
import uniffi.neuralcompose_mobile_core.ChannelSnapshot
import uniffi.neuralcompose_mobile_core.CaptureRecorder
import uniffi.neuralcompose_mobile_core.MonitorConfig
import uniffi.neuralcompose_mobile_core.Presentation
import uniffi.neuralcompose_mobile_core.ReconnectDecision
import uniffi.neuralcompose_mobile_core.SocketEvent
import uniffi.neuralcompose_mobile_core.StreamMonitor

data class EEGUiState(
    val presentation: Presentation,
    val snapshot: ChannelSnapshot,
)

/** Counters read straight off the live recorder — never recomputed here. */
data class ActiveCaptureUi(
    val recordingId: String,
    val messagesReceived: ULong,
    val acceptedSampleCount: ULong,
    val elapsedMs: Long,
)

data class CaptureUiState(
    /** The endpoint the socket is actually using. */
    val endpoint: String,
    val locality: BridgeLocality,
    /** True when the platform will refuse cleartext to this host. */
    val cleartextBlocked: Boolean,
    val active: ActiveCaptureUi?,
    val recordings: List<StoredCapture>,
    /** Manifests found on disk that could not be read. Visible, never hidden. */
    val problems: List<String>,
    /** Last action result (publish / verify / delete). */
    val notice: String?,
    /** Replay verdicts keyed by recording id, from the core's verifier. */
    val verdicts: Map<String, String>,
)

class EEGStreamViewModel(app: Application) : AndroidViewModel(app) {

    private val prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val store = CaptureStore(app.filesDir)

    /** Operator-configurable; a phone's own loopback is not the bridge. */
    private var url: String = prefs.getString(KEY_ENDPOINT, DEFAULT_ENDPOINT) ?: DEFAULT_ENDPOINT

    private val monitor = StreamMonitor(
        MonitorConfig(
            keepSamples = 1280u,
            staleAfterMs = 2000uL,
            maxReconnectAttempts = 3u,
            backoffBaseMs = 500uL,
            backoffCapMs = 30000uL,
        ),
    )

    // pingInterval is load-bearing: without it a socket that dies while the
    // process is frozen (screen off) stays a believed-open zombie forever —
    // the core then reports STALE honestly but recovery never starts. Pings
    // surface the dead socket as onFailure → CLOSED → the core's retry ladder.
    private val client = OkHttpClient.Builder()
        .pingInterval(java.time.Duration.ofSeconds(5))
        .build()
    private var socket: WebSocket? = null
    private var pollJob: Job? = null

    // Callbacks from a socket the operator has already replaced must not feed
    // the monitor or the capture: a superseded endpoint's frames would be
    // recorded under a manifest that names the new one.
    @Volatile
    private var generation: Long = 0

    private val _state = MutableStateFlow(
        EEGUiState(monitor.presentation(nowMs()), monitor.snapshot()),
    )
    val state: StateFlow<EEGUiState> = _state

    private val _capture = MutableStateFlow(
        CaptureUiState(
            endpoint = url,
            locality = localityOf(url),
            cleartextBlocked = cleartextBlocked(url),
            active = null,
            recordings = emptyList(),
            problems = emptyList(),
            notice = null,
            verdicts = emptyMap(),
        ),
    )
    val capture: StateFlow<CaptureUiState> = _capture

    /** Monotonic ms — never wall clock. */
    private fun nowMs(): ULong = SystemClock.elapsedRealtime().toULong()

    // ---- capture state -------------------------------------------------

    private class ActiveCapture(
        val recordingId: String,
        val recorder: CaptureRecorder,
        val writer: CapturePayloadWriter,
        val startedAtMs: ULong,
    )

    /**
     * Guards the (decode → append) pair. Frames arrive on OkHttp's reader
     * thread while Stop runs on the main thread; without this a frame could
     * be counted by the recorder after the writer closed, and the manifest
     * would claim a message the file does not contain.
     */
    private val captureLock = Any()

    @Volatile
    private var active: ActiveCapture? = null

    fun start() {
        if (pollJob != null) return
        refreshRecordings(null)
        connect()
        pollJob = viewModelScope.launch {
            while (true) {
                _state.value = EEGUiState(monitor.presentation(nowMs()), monitor.snapshot())
                _capture.value = _capture.value.copy(active = activeUi())
                delay(500)
            }
        }
    }

    private fun activeUi(): ActiveCaptureUi? {
        val a = active ?: return null
        return ActiveCaptureUi(
            recordingId = a.recordingId,
            messagesReceived = a.recorder.messagesReceived(),
            acceptedSampleCount = a.recorder.acceptedSampleCount(),
            elapsedMs = (nowMs() - a.startedAtMs).toLong(),
        )
    }

    // ---- endpoint ------------------------------------------------------

    /** Persists a new endpoint and reconnects to it. */
    fun applyEndpoint(raw: String) {
        val trimmed = raw.trim()
        if (trimmed.isEmpty()) {
            note("Endpoint cannot be empty.")
            return
        }
        if (!trimmed.startsWith("ws://") && !trimmed.startsWith("wss://")) {
            note("Endpoint must start with ws:// or wss://")
            return
        }
        prefs.edit().putString(KEY_ENDPOINT, trimmed).apply()
        url = trimmed
        reconnect()
    }

    /** Drops the current socket and dials the configured endpoint afresh. */
    fun reconnect() {
        generation += 1 // orphan the outgoing socket's callbacks
        socket?.cancel()
        socket = null
        monitor.reset()
        _capture.value = _capture.value.copy(
            endpoint = url,
            locality = localityOf(url),
            cleartextBlocked = cleartextBlocked(url),
        )
        connect()
    }

    private fun connect() {
        val gen = generation
        monitor.onSocketEvent(SocketEvent.CONNECTING, nowMs())
        socket = client.newWebSocket(
            Request.Builder().url(url).build(),
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    if (gen != generation) {
                        webSocket.cancel()
                        return
                    }
                    monitor.onSocketEvent(SocketEvent.OPENED, nowMs())
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    if (gen != generation) return
                    val now = nowMs()
                    monitor.onFrame(text, now)
                    onCaptureMessage(text, now)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    if (gen != generation) return
                    handleDisconnect()
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (gen != generation) return
                    handleDisconnect()
                }
            },
        )
    }

    private fun handleDisconnect() {
        monitor.onSocketEvent(SocketEvent.CLOSED, nowMs())
        when (val decision = monitor.reconnectDecision()) {
            is ReconnectDecision.RetryAfterMs -> viewModelScope.launch {
                delay(decision.delayMs.toLong())
                connect()
            }
            is ReconnectDecision.GiveUp -> Unit // Error latched; core reports it
        }
    }

    // ---- capture: record ------------------------------------------------

    /**
     * Every received message is offered to the recorder, malformed ones
     * included — the core counts those as rejected and still preserves them
     * verbatim. Dropping them here would misrepresent the stream.
     */
    private fun onCaptureMessage(text: String, now: ULong) {
        val failure: Exception? = synchronized(captureLock) {
            val a = active ?: return
            try {
                a.writer.appendLine(a.recorder.onMessage(text, now))
                null
            } catch (e: Exception) {
                // The recorder has counted a message the file does not hold,
                // so this capture can never reconcile. Drop the partial rather
                // than leave bytes that could be mistaken for a recording.
                active = null
                a.writer.abandon()
                runCatching { a.recorder.close() }
                e
            }
        }
        if (failure != null) {
            // The file is now incomplete relative to the recorder's counters,
            // so the capture cannot be published as evidence. Say so.
            note("Recording aborted — write failed: ${failure.message ?: failure.javaClass.simpleName}")
            refreshRecordings(_capture.value.notice)
        }
    }

    fun startRecording() {
        if (active != null) {
            note("A recording is already running.")
            return
        }
        // Wall clock is used ONLY to name the recording; every timestamp the
        // envelope carries comes from the monotonic clock.
        val recordingId = "rec-${System.currentTimeMillis()}"
        val startedAt = nowMs()
        try {
            val writer = store.beginPayload(recordingId)
            val recorder = CaptureRecorder(recordingId, buildIdentity(), startedAt)
            synchronized(captureLock) {
                active = ActiveCapture(recordingId, recorder, writer, startedAt)
            }
            note("Recording $recordingId")
            _capture.value = _capture.value.copy(active = activeUi())
        } catch (e: Exception) {
            note("Could not start recording: ${e.message ?: e.javaClass.simpleName}")
        }
    }

    /**
     * Publication transaction:
     *   swap the recorder out under the lock (no further frames can be
     *   counted) → flush + fsync the payload → size + streamed SHA-256 →
     *   `recorder.finish` → manifest .partial + fsync → rename PAYLOAD →
     *   rename MANIFEST.
     * A failure anywhere leaves only `.partial` files, which discovery
     * ignores, so a crash can never publish a manifest without its payload.
     */
    fun stopRecording() {
        val endedAt = nowMs()
        val a = synchronized(captureLock) {
            val a = active
            active = null
            a
        }
        if (a == null) {
            note("No recording is running.")
            return
        }
        _capture.value = _capture.value.copy(active = null)
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                try {
                    a.writer.closeDurably()
                    val partial: File = a.writer.partialFile
                    val size = partial.length().toULong()
                    val sha = CaptureStore.sha256OfFile(partial)
                    val manifest = a.recorder.finish(endedAt, size, sha)
                    store.publish(manifest)
                    "Published ${manifest.recordingId} — " +
                        "${manifest.acceptedSampleCount} samples, " +
                        "${manifest.messagesReceived} messages, " +
                        "${manifest.rejectedMessageCount} rejected, " +
                        "${manifest.durationMs} ms, ${manifest.payloadByteSize} bytes"
                } catch (e: Exception) {
                    a.writer.abandon()
                    store.manifestPartial(a.recordingId).delete()
                    "Publish FAILED for ${a.recordingId}: " +
                        (e.message ?: e.javaClass.simpleName) + " — nothing published"
                } finally {
                    runCatching { a.recorder.close() } // release the Rust handle
                }
            }
            refreshRecordings(result)
        }
    }

    // ---- capture: replay / delete / discovery ---------------------------

    fun verify(recordingId: String) {
        viewModelScope.launch {
            val text = withContext(Dispatchers.IO) {
                try {
                    val verdict = store.verify(recordingId)
                        ?: return@withContext "Cannot verify $recordingId — files missing"
                    CaptureStore.describe(verdict)
                } catch (e: Exception) {
                    "Cannot verify $recordingId — ${e.message ?: e.javaClass.simpleName}"
                }
            }
            _capture.value = _capture.value.copy(
                notice = "$recordingId: $text",
                verdicts = _capture.value.verdicts + (recordingId to text),
            )
        }
    }

    fun delete(recordingId: String) {
        viewModelScope.launch {
            val ok = withContext(Dispatchers.IO) { store.delete(recordingId) }
            _capture.value = _capture.value.copy(
                verdicts = _capture.value.verdicts - recordingId,
            )
            refreshRecordings(
                if (ok) {
                    "Deleted $recordingId (payload + manifest)"
                } else {
                    "Delete INCOMPLETE for $recordingId — files remain on disk"
                },
            )
        }
    }

    /** Rescans the capture dir; `.partial` files are never recordings. */
    fun refreshRecordings(notice: String?) {
        val (found, problems) = store.listPublished()
        _capture.value = _capture.value.copy(
            recordings = found,
            problems = problems,
            notice = notice ?: _capture.value.notice,
            active = activeUi(),
        )
    }

    private fun note(message: String) {
        _capture.value = _capture.value.copy(notice = message)
    }

    // ---- build identity --------------------------------------------------

    private fun buildIdentity() = CaptureBuildIdentity(
        platform = "android",
        osVersion = Build.VERSION.RELEASE ?: "unknown",
        appVersion = appVersion(),
        // No gradle git plumbing in this build; claiming a commit we cannot
        // read would be worse than admitting we do not know it.
        gitCommit = "unknown",
        bridgeLocality = localityOf(url),
    )

    private fun appVersion(): String = try {
        val ctx = getApplication<Application>()
        ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName ?: FALLBACK_VERSION
    } catch (e: Exception) {
        FALLBACK_VERSION
    }

    override fun onCleared() {
        pollJob?.cancel()
        generation += 1
        // An in-flight capture is NOT published: its payload would be missing
        // whatever the socket had not yet delivered, and a partial recording
        // presented as complete is exactly the failure this gate rules out.
        synchronized(captureLock) {
            active?.writer?.abandon()
            active?.let { runCatching { it.recorder.close() } }
            active = null
        }
        socket?.close(1000, "view model cleared")
        client.dispatcher.executorService.shutdown()
    }

    companion object {
        const val DEFAULT_ENDPOINT = "ws://127.0.0.1:8787/api/eeg/stream"
        private const val PREFS = "neuralcompose.eeg"
        private const val KEY_ENDPOINT = "streamEndpoint"
        private const val FALLBACK_VERSION = "0.1.0"

        fun hostOf(url: String): String? = try {
            URI(url).host
        } catch (e: Exception) {
            null
        }

        /**
         * LAN-vs-remote is recorded in the manifest so a capture taken over a
         * remote bridge can never be mistaken for an on-network one.
         */
        fun localityOf(url: String): BridgeLocality {
            val host = hostOf(url)?.lowercase() ?: return BridgeLocality.REMOTE_ENDPOINT
            val private = host == "localhost" ||
                host.endsWith(".local") ||
                host.startsWith("127.") ||
                host.startsWith("10.") ||
                host.startsWith("192.168.") ||
                isCarrierGradePrivate172(host)
            return if (private) BridgeLocality.LOCAL_NETWORK else BridgeLocality.REMOTE_ENDPOINT
        }

        private fun isCarrierGradePrivate172(host: String): Boolean {
            if (!host.startsWith("172.")) return false
            val second = host.split('.').getOrNull(1)?.toIntOrNull() ?: return false
            return second in 16..31
        }

        /**
         * The platform blocks cleartext for hosts not exempted in
         * res/xml/network_security_config.xml. Surfacing it here turns an
         * otherwise cryptic socket failure into an actionable message.
         */
        fun cleartextBlocked(url: String): Boolean {
            if (!url.startsWith("ws://")) return false
            val host = hostOf(url) ?: return false
            return !NetworkSecurityPolicy.getInstance().isCleartextTrafficPermitted(host)
        }
    }
}

// M6 Android shell: MediaRecorder/MediaPlayer + files stay HERE; every state
// decision comes from the Rust AudioLifecycle. Manifests persist to a local
// JSON file (metadata only — audio bytes live as .m4a files in filesDir).

package org.neuralcompose.client

import android.app.Application
import android.media.MediaPlayer
import android.media.MediaRecorder
import android.os.SystemClock
import androidx.lifecycle.AndroidViewModel
import java.io.File
import java.util.UUID
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONArray
import org.json.JSONObject
import uniffi.neuralcompose_mobile_core.AudioLifecycle
import uniffi.neuralcompose_mobile_core.AudioSnapshot
import uniffi.neuralcompose_mobile_core.RecordingManifest
import uniffi.neuralcompose_mobile_core.sha256Hex

class JournalViewModel(app: Application) : AndroidViewModel(app) {

    private val recordingsDir = File(app.filesDir, "recordings").apply { mkdirs() }
    private val manifestFile = File(app.filesDir, "recording-manifests.json")

    private val lifecycle: AudioLifecycle = AudioLifecycle.withManifests(loadManifests())

    private var recorder: MediaRecorder? = null
    private var player: MediaPlayer? = null
    private var activeFile: File? = null
    private var recordStartedAt: Long = 0
    private var recordStartedWallMs: Long = 0

    private val _state = MutableStateFlow(lifecycle.snapshot())
    val state: StateFlow<AudioSnapshot> = _state

    private fun now(): ULong = SystemClock.elapsedRealtime().toULong()

    private fun refresh() {
        _state.value = lifecycle.snapshot()
    }

    fun onPermissionResult(granted: Boolean) {
        lifecycle.onPermission(granted, now())
        refresh()
    }

    fun startRecording() {
        if (!lifecycle.onRecordStart(now())) return
        val file = File(recordingsDir, "${UUID.randomUUID()}.m4a")
        activeFile = file
        recordStartedAt = SystemClock.elapsedRealtime()
        recordStartedWallMs = System.currentTimeMillis()
        try {
            @Suppress("DEPRECATION")
            recorder = MediaRecorder().apply {
                setAudioSource(MediaRecorder.AudioSource.MIC)
                setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
                setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
                setOutputFile(file.absolutePath)
                prepare()
                start()
            }
        } catch (e: Exception) {
            // Recorder never started: report interruption + recovery so the
            // core lands back on Ready; no file, no entry.
            lifecycle.onInterruption(now())
            lifecycle.onInterruptionEnded(now())
            file.delete()
            activeFile = null
        }
        refresh()
    }

    fun stopRecording() {
        if (!lifecycle.onRecordStop(now())) return
        refresh()
        val durationMs = SystemClock.elapsedRealtime() - recordStartedAt
        val file = activeFile
        try {
            recorder?.apply {
                stop()
                release()
            }
            recorder = null
            if (file == null || !file.exists()) {
                lifecycle.onPersistFailed("recording file missing", now())
            } else {
                val bytes = file.readBytes()
                lifecycle.onPersisted(
                    id = file.nameWithoutExtension,
                    createdAtMs = recordStartedWallMs.toULong(),
                    durationMs = durationMs.toULong(),
                    format = "m4a",
                    byteSize = bytes.size.toULong(),
                    sha256Hex = sha256Hex(bytes),
                    nowMs = now(),
                )
                saveManifests()
            }
        } catch (e: Exception) {
            lifecycle.onPersistFailed(e.message ?: "recorder stop failed", now())
            file?.delete()
        }
        activeFile = null
        refresh()
    }

    fun playLatest() {
        val manifest = lifecycle.snapshot().manifests.lastOrNull() ?: return
        if (!lifecycle.onPlayStart(now())) return
        val file = File(recordingsDir, "${manifest.id}.m4a")
        try {
            player = MediaPlayer().apply {
                setDataSource(file.absolutePath)
                setOnCompletionListener {
                    lifecycle.onPlayStop(now())
                    refresh()
                }
                prepare()
                start()
            }
        } catch (e: Exception) {
            lifecycle.onPlayStop(now())
        }
        refresh()
    }

    fun stopPlayback() {
        if (!lifecycle.onPlayStop(now())) return
        player?.release()
        player = null
        refresh()
    }

    fun acknowledgeFailure() {
        lifecycle.onFailureAcknowledged(now())
        refresh()
    }

    /** App backgrounded / focus lost while active. */
    fun onInterruption() {
        if (lifecycle.onInterruption(now())) {
            recorder?.release()
            recorder = null
            activeFile?.delete()
            activeFile = null
            player?.release()
            player = null
            refresh()
        }
    }

    fun onInterruptionEnded() {
        if (lifecycle.onInterruptionEnded(now())) refresh()
    }

    private fun loadManifests(): List<RecordingManifest> {
        if (!manifestFile.exists()) return emptyList()
        return try {
            val arr = JSONArray(manifestFile.readText())
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                RecordingManifest(
                    id = o.getString("id"),
                    createdAtMs = o.getLong("createdAtMs").toULong(),
                    durationMs = o.getLong("durationMs").toULong(),
                    format = o.getString("format"),
                    byteSize = o.getLong("byteSize").toULong(),
                    sha256Hex = o.getString("sha256Hex"),
                )
            }
        } catch (e: Exception) {
            emptyList()
        }
    }

    private fun saveManifests() {
        val arr = JSONArray()
        lifecycle.snapshot().manifests.forEach { m ->
            arr.put(
                JSONObject()
                    .put("id", m.id)
                    .put("createdAtMs", m.createdAtMs.toLong())
                    .put("durationMs", m.durationMs.toLong())
                    .put("format", m.format)
                    .put("byteSize", m.byteSize.toLong())
                    .put("sha256Hex", m.sha256Hex),
            )
        }
        manifestFile.writeText(arr.toString())
    }

    override fun onCleared() {
        recorder?.release()
        player?.release()
    }
}

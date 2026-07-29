// M6 Android shell: MediaRecorder/MediaPlayer + files stay HERE; every state
// decision comes from the Rust AudioLifecycle. Manifest JSON is published
// ATOMICALLY (same-dir .partial + fsync + rename) BEFORE the core is told
// persistence succeeded — the core's manifest list never gets ahead of the
// durable file. Corruption and integrity failures are VISIBLE, never an
// empty journal.

package org.neuralcompose.client

import android.app.Application
import android.media.MediaPlayer
import android.media.MediaRecorder
import android.os.SystemClock
import androidx.lifecycle.AndroidViewModel
import java.io.File
import java.io.FileOutputStream
import java.util.UUID
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONArray
import org.json.JSONObject
import uniffi.neuralcompose_mobile_core.AudioLifecycle
import uniffi.neuralcompose_mobile_core.AudioSnapshot
import uniffi.neuralcompose_mobile_core.RecordingManifest
import uniffi.neuralcompose_mobile_core.sha256Hex

data class JournalUiState(
    val snapshot: AudioSnapshot,
    /** Non-null when the canonical manifest file was malformed (quarantined). */
    val manifestError: String?,
    /** Manifest IDs whose audio bytes are missing or fail size/hash checks. */
    val invalidIds: Set<String>,
)

class JournalViewModel(app: Application) : AndroidViewModel(app) {

    private val recordingsDir = File(app.filesDir, "recordings").apply { mkdirs() }
    private val manifestFile = File(app.filesDir, "recording-manifests.json")

    private var manifestError: String? = null
    private var invalidIds: Set<String> = emptySet()

    private val lifecycle: AudioLifecycle

    private var recorder: MediaRecorder? = null
    private var player: MediaPlayer? = null
    private var activeFile: File? = null
    private var recordStartedAt: Long = 0
    private var recordStartedWallMs: Long = 0

    private val _state: MutableStateFlow<JournalUiState>
    val state: StateFlow<JournalUiState>

    init {
        val loaded = loadManifestsVisible()
        lifecycle = AudioLifecycle.withManifests(loaded)
        invalidIds = verifyIntegrity(loaded)
        _state = MutableStateFlow(JournalUiState(lifecycle.snapshot(), manifestError, invalidIds))
        state = _state
    }

    private fun now(): ULong = SystemClock.elapsedRealtime().toULong()

    private fun refresh() {
        _state.value = JournalUiState(lifecycle.snapshot(), manifestError, invalidIds)
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
            lifecycle.onInterruption(now())
            lifecycle.onInterruptionEnded(now())
            file.delete()
            activeFile = null
        }
        refresh()
    }

    /**
     * The atomic publish transaction (review finding 1):
     *   stop+close recorder → read bytes → size+sha → CANDIDATE manifest list
     *   → write .partial (fsync) → atomic rename over canonical
     *   → ONLY THEN tell Rust persistence succeeded.
     * Any failure before the rename leaves the previous canonical file
     * intact, reports persist_failed (no manifest), and removes the orphan
     * audio bytes so no committed entry can reference missing data.
     */
    fun stopRecording() {
        if (!lifecycle.onRecordStop(now())) return
        refresh()
        val durationMs = SystemClock.elapsedRealtime() - recordStartedAt
        val file = activeFile
        activeFile = null
        try {
            recorder?.apply {
                stop()
                release()
            }
            recorder = null
            if (file == null || !file.exists()) {
                lifecycle.onPersistFailed("recording file missing", now())
                refresh()
                return
            }
            val bytes = file.readBytes()
            val candidate = RecordingManifest(
                id = file.nameWithoutExtension,
                createdAtMs = recordStartedWallMs.toULong(),
                durationMs = durationMs.toULong(),
                format = "m4a",
                byteSize = bytes.size.toULong(),
                sha256Hex = sha256Hex(bytes),
            )
            val candidateList = lifecycle.snapshot().manifests + candidate
            writeManifestsAtomically(candidateList) // throws on any failure
            check(
                lifecycle.onPersisted(
                    candidate.id, candidate.createdAtMs, candidate.durationMs,
                    candidate.format, candidate.byteSize, candidate.sha256Hex, now(),
                ),
            ) { "core rejected persisted event" }
        } catch (e: Exception) {
            lifecycle.onPersistFailed(e.message ?: "persist failed", now())
            file?.delete() // orphan bytes must not outlive a failed commit
        }
        refresh()
    }

    fun playLatest() {
        val manifest = lifecycle.snapshot().manifests.lastOrNull() ?: return
        if (manifest.id in invalidIds) return // UI shows the integrity error
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

    // ---- manifest persistence + integrity (review findings 1 and 3) ----

    /** Malformed canonical JSON is QUARANTINED and reported, never silently
     *  presented as an empty journal. */
    private fun loadManifestsVisible(): List<RecordingManifest> {
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
            val quarantine = File(
                manifestFile.parentFile,
                "recording-manifests.corrupt-${System.currentTimeMillis()}.json",
            )
            manifestFile.renameTo(quarantine)
            manifestError =
                "Manifest file was corrupted; preserved as ${quarantine.name}. " +
                "Recordings on disk were NOT deleted."
            emptyList()
        }
    }

    /** Entries whose bytes are missing or mismatched are marked not-playable. */
    private fun verifyIntegrity(manifests: List<RecordingManifest>): Set<String> {
        val bad = mutableSetOf<String>()
        for (m in manifests) {
            val f = File(recordingsDir, "${m.id}.m4a")
            if (!f.exists()) {
                bad += m.id
                continue
            }
            if (f.length().toULong() != m.byteSize) {
                bad += m.id
                continue
            }
            if (sha256Hex(f.readBytes()) != m.sha256Hex) bad += m.id
        }
        return bad
    }

    private fun writeManifestsAtomically(manifests: List<RecordingManifest>) {
        val arr = JSONArray()
        manifests.forEach { m ->
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
        val partial = File(manifestFile.parentFile, "${manifestFile.name}.partial")
        FileOutputStream(partial).use { out ->
            out.write(arr.toString().toByteArray())
            out.fd.sync()
        }
        if (!partial.renameTo(manifestFile)) {
            partial.delete()
            throw java.io.IOException("atomic manifest rename failed")
        }
    }

    override fun onCleared() {
        recorder?.release()
        player?.release()
    }
}

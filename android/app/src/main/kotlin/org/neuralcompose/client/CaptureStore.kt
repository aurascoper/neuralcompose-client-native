// Golden-capture filesystem side. The ENVELOPE lives in Rust
// (crates/neuralcompose-mobile-core/src/capture.rs) — this file only does the
// two things Rust deliberately refuses to do: touch files and read a clock.
//
// Nothing here parses EEG. Lines come back from `CaptureRecorder.onMessage`
// verbatim and are appended byte-for-byte; the manifest is transcribed to and
// from JSON using the EXACT camelCase field names serde emits, so a recording
// written by this shell is the same document an iOS shell would have written.
//
// Publication is atomic in the only sense that matters to a replay: a
// `.manifest.json` is never visible before the `.eeg.jsonl` it describes.

package org.neuralcompose.client

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.security.MessageDigest
import org.json.JSONArray
import org.json.JSONObject
import uniffi.neuralcompose_mobile_core.BridgeLocality
import uniffi.neuralcompose_mobile_core.CaptureManifest
import uniffi.neuralcompose_mobile_core.ReplayFailure
import uniffi.neuralcompose_mobile_core.ReplayVerdict
import uniffi.neuralcompose_mobile_core.captureManifestFilename
import uniffi.neuralcompose_mobile_core.capturePayloadFilename
import uniffi.neuralcompose_mobile_core.partialSuffix
import uniffi.neuralcompose_mobile_core.verifyCapture

/** A published recording as discovered on disk. */
data class StoredCapture(
    val manifest: CaptureManifest,
    val payloadExists: Boolean,
    val payloadFileSize: Long,
) {
    val id: String get() = manifest.recordingId
}

/**
 * Appends recorder-produced JSONL lines to `<id>.eeg.jsonl.partial`.
 *
 * Frames arrive on OkHttp's reader thread and Stop runs on the main thread,
 * so every method is synchronized on the writer itself — a half-written line
 * would fail replay as a MalformedLine, which is exactly the outcome the gate
 * exists to make impossible.
 */
class CapturePayloadWriter(val partialFile: File) {
    private val stream = FileOutputStream(partialFile, /* append = */ false)
    private var closed = false

    /** Appends one JSONL line plus the record separator. */
    @Synchronized
    fun appendLine(line: String) {
        if (closed) return
        stream.write(line.toByteArray(Charsets.UTF_8))
        stream.write(NEWLINE)
    }

    /** Flushes and fsyncs, then closes. Safe to call twice. */
    @Synchronized
    fun closeDurably() {
        if (closed) return
        closed = true
        stream.flush()
        stream.fd.sync()
        stream.close()
    }

    @Synchronized
    fun abandon() {
        if (!closed) {
            closed = true
            runCatching { stream.close() }
        }
        partialFile.delete()
    }

    private companion object {
        val NEWLINE = "\n".toByteArray(Charsets.UTF_8)
    }
}

/**
 * Owns the capture directory: discovery, publication, replay and deletion.
 * Recordings live in the app's private files dir and never leave it.
 */
class CaptureStore(filesDir: File) {

    val dir: File = File(filesDir, "captures").apply { mkdirs() }

    fun payloadFile(recordingId: String) = File(dir, capturePayloadFilename(recordingId))

    fun manifestFile(recordingId: String) = File(dir, captureManifestFilename(recordingId))

    fun payloadPartial(recordingId: String) =
        File(dir, capturePayloadFilename(recordingId) + partialSuffix())

    fun manifestPartial(recordingId: String) =
        File(dir, captureManifestFilename(recordingId) + partialSuffix())

    fun beginPayload(recordingId: String): CapturePayloadWriter =
        CapturePayloadWriter(payloadPartial(recordingId))

    /**
     * Publishes a finished recording.
     *
     * The payload `.partial` must already be flushed and fsynced by the
     * caller (only the caller knows when the last frame landed). This writes
     * the manifest `.partial`, fsyncs it, then renames the PAYLOAD first and
     * the MANIFEST second: discovery keys on `*.manifest.json`, so that order
     * makes a manifest that references a missing payload unreachable rather
     * than merely unlikely.
     *
     * Any failure leaves both `.partial` files unpublished and undiscoverable.
     */
    fun publish(manifest: CaptureManifest) {
        val id = manifest.recordingId
        val payloadPartial = payloadPartial(id)
        val manifestPartial = manifestPartial(id)
        if (!payloadPartial.exists()) throw IOException("payload partial missing for $id")

        FileOutputStream(manifestPartial, false).use { out ->
            out.write(encodeManifest(manifest).toByteArray(Charsets.UTF_8))
            out.flush()
            out.fd.sync()
        }
        if (!payloadPartial.renameTo(payloadFile(id))) {
            manifestPartial.delete()
            throw IOException("atomic payload rename failed for $id")
        }
        if (!manifestPartial.renameTo(manifestFile(id))) {
            // The payload is published but undiscoverable without a manifest;
            // roll it back to .partial so nothing half-published survives.
            payloadFile(id).renameTo(payloadPartial)
            manifestPartial.delete()
            throw IOException("atomic manifest rename failed for $id")
        }
    }

    /**
     * Discovers published recordings by scanning for `*.manifest.json`.
     * `.partial` files are in-progress or failed and are never recordings.
     * A manifest that will not parse is REPORTED, never silently skipped.
     */
    fun listPublished(): Pair<List<StoredCapture>, List<String>> {
        val suffix = captureManifestFilename("")
        val problems = mutableListOf<String>()
        val found = mutableListOf<StoredCapture>()
        val files = dir.listFiles() ?: return Pair(emptyList(), emptyList())
        for (f in files.sortedBy { it.name }) {
            val name = f.name
            if (name.endsWith(partialSuffix()) || !name.endsWith(suffix)) continue
            val manifest = try {
                decodeManifest(f.readText(Charsets.UTF_8))
            } catch (e: Exception) {
                problems += "${f.name}: unreadable manifest (${e.message ?: e.javaClass.simpleName})"
                continue
            }
            val payload = payloadFile(manifest.recordingId)
            found += StoredCapture(
                manifest = manifest,
                payloadExists = payload.exists(),
                payloadFileSize = if (payload.exists()) payload.length() else 0L,
            )
        }
        return Pair(found.sortedByDescending { it.id }, problems)
    }

    /** Replays a published recording through the core's verifier. */
    fun verify(id: String): ReplayVerdict? {
        val payload = payloadFile(id)
        val manifestFile = manifestFile(id)
        if (!payload.exists() || !manifestFile.exists()) return null
        val manifest = decodeManifest(manifestFile.readText(Charsets.UTF_8))
        return verifyCapture(payload.readText(Charsets.UTF_8), manifest)
    }

    /** Removes both files. Returns true only if nothing is left behind. */
    fun delete(id: String): Boolean {
        val p = payloadFile(id)
        val m = manifestFile(id)
        // Manifest first: a payload without a manifest is undiscoverable,
        // whereas the reverse would leave a dangling reference visible.
        val mGone = !m.exists() || m.delete()
        val pGone = !p.exists() || p.delete()
        return mGone && pGone
    }

    // ---- manifest JSON codec -------------------------------------------
    // Field names and the bridgeLocality spellings mirror serde's camelCase
    // rename on CaptureManifest / BridgeLocality exactly. Changing either
    // side without the other silently breaks cross-platform replay.

    fun encodeManifest(m: CaptureManifest): String {
        val channels = JSONArray()
        m.channelOrder.forEach { channels.put(it) }
        return JSONObject()
            .put("schemaId", m.schemaId)
            .put("lineSchemaId", m.lineSchemaId)
            .put("recordingId", m.recordingId)
            .put("platform", m.platform)
            .put("osVersion", m.osVersion)
            .put("appVersion", m.appVersion)
            .put("gitCommit", m.gitCommit)
            .put("bridgeLocality", localityToJson(m.bridgeLocality))
            .put("startedAtMonotonicMs", m.startedAtMonotonicMs.toLong())
            .put("endedAtMonotonicMs", m.endedAtMonotonicMs.toLong())
            .put("durationMs", m.durationMs.toLong())
            .put("messagesReceived", m.messagesReceived.toLong())
            .put("acceptedSampleCount", m.acceptedSampleCount.toLong())
            .put("rejectedMessageCount", m.rejectedMessageCount.toLong())
            .put("firstSourceTimestamp", m.firstSourceTimestamp ?: JSONObject.NULL)
            .put("lastSourceTimestamp", m.lastSourceTimestamp ?: JSONObject.NULL)
            .put("channelOrder", channels)
            .put("payloadByteSize", m.payloadByteSize.toLong())
            .put("payloadSha256Hex", m.payloadSha256Hex)
            .toString()
    }

    fun decodeManifest(json: String): CaptureManifest {
        val o = JSONObject(json)
        val channels = o.getJSONArray("channelOrder")
        return CaptureManifest(
            schemaId = o.getString("schemaId"),
            lineSchemaId = o.getString("lineSchemaId"),
            recordingId = o.getString("recordingId"),
            platform = o.getString("platform"),
            osVersion = o.getString("osVersion"),
            appVersion = o.getString("appVersion"),
            gitCommit = o.getString("gitCommit"),
            bridgeLocality = localityFromJson(o.getString("bridgeLocality")),
            startedAtMonotonicMs = o.getLong("startedAtMonotonicMs").toULong(),
            endedAtMonotonicMs = o.getLong("endedAtMonotonicMs").toULong(),
            durationMs = o.getLong("durationMs").toULong(),
            messagesReceived = o.getLong("messagesReceived").toULong(),
            acceptedSampleCount = o.getLong("acceptedSampleCount").toULong(),
            rejectedMessageCount = o.getLong("rejectedMessageCount").toULong(),
            firstSourceTimestamp = if (o.isNull("firstSourceTimestamp")) {
                null
            } else {
                o.getDouble("firstSourceTimestamp")
            },
            lastSourceTimestamp = if (o.isNull("lastSourceTimestamp")) {
                null
            } else {
                o.getDouble("lastSourceTimestamp")
            },
            channelOrder = (0 until channels.length()).map { channels.getString(it) },
            payloadByteSize = o.getLong("payloadByteSize").toULong(),
            payloadSha256Hex = o.getString("payloadSha256Hex"),
        )
    }

    companion object {
        fun localityToJson(l: BridgeLocality): String = when (l) {
            BridgeLocality.LOCAL_NETWORK -> "localNetwork"
            BridgeLocality.REMOTE_ENDPOINT -> "remoteEndpoint"
        }

        fun localityFromJson(s: String): BridgeLocality = when (s) {
            "localNetwork" -> BridgeLocality.LOCAL_NETWORK
            "remoteEndpoint" -> BridgeLocality.REMOTE_ENDPOINT
            else -> throw IllegalArgumentException("unknown bridgeLocality: $s")
        }

        /** Streamed so a long capture is never held in memory to be hashed. */
        fun sha256OfFile(file: File): String {
            val digest = MessageDigest.getInstance("SHA-256")
            file.inputStream().use { input ->
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n <= 0) break
                    digest.update(buf, 0, n)
                }
            }
            return digest.digest().joinToString("") { "%02x".format(it) }
        }

        /** Operator-readable rendering of the core's failure variants. */
        fun describe(verdict: ReplayVerdict): String = when (verdict) {
            is ReplayVerdict.Verified ->
                "VERIFIED — ${verdict.acceptedSampleCount} samples replayed"
            is ReplayVerdict.Failed -> "FAILED — ${describe(verdict.failure)}"
        }

        fun describe(f: ReplayFailure): String = when (f) {
            is ReplayFailure.ManifestSchemaMismatch -> "manifest schema mismatch"
            is ReplayFailure.PayloadDigestMismatch -> "payload digest mismatch"
            is ReplayFailure.PayloadSizeMismatch -> "payload size mismatch"
            is ReplayFailure.MalformedLine -> "malformed line ${f.lineNumber}"
            is ReplayFailure.LineSchemaMismatch -> "line schema mismatch at line ${f.lineNumber}"
            is ReplayFailure.SequenceOutOfOrder -> "sequence out of order at line ${f.lineNumber}"
            is ReplayFailure.ReceiveTimeWentBackwards ->
                "receive time went backwards at line ${f.lineNumber}"
            is ReplayFailure.AcceptedCountMismatch ->
                "accepted count mismatch at line ${f.lineNumber}"
            is ReplayFailure.MessageCountMismatch -> "message count mismatch"
            is ReplayFailure.AcceptedSampleCountMismatch -> "accepted sample count mismatch"
            is ReplayFailure.RejectedMessageCountMismatch -> "rejected message count mismatch"
            is ReplayFailure.SourceTimestampNotMonotonic ->
                "source timestamp not monotonic at line ${f.lineNumber}"
            is ReplayFailure.NonFiniteChannel -> "non-finite channel at line ${f.lineNumber}"
            is ReplayFailure.WrongChannelCount -> "wrong channel count at line ${f.lineNumber}"
            is ReplayFailure.FirstSourceTimestampMismatch -> "first source timestamp mismatch"
            is ReplayFailure.LastSourceTimestampMismatch -> "last source timestamp mismatch"
            is ReplayFailure.ChannelOrderMismatch -> "channel order mismatch"
        }
    }
}

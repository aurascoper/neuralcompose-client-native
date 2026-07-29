// The in-flight recording. Reached from the WebSocket receive callback (a
// non-main thread) AND from the main actor, so everything sits behind one
// lock — and that lock deliberately spans `recorder.onMessage` plus the file
// append: the core assigns the sequence number, so if two threads interleaved
// between numbering and writing, the file would contain lines out of sequence
// and replay would (correctly) reject a recording that was actually fine.

import Foundation
import NeuralComposeCore

/// What the UI needs to show about a recording in progress.
struct CaptureStats: Equatable, Sendable {
    let recordingId: String
    let startedAtMs: UInt64
    let messagesReceived: UInt64
    let acceptedSampleCount: UInt64
    /// Latched write error. Once set, the recording is doomed and `finish`
    /// will discard rather than publish a manifest the file cannot honour.
    let writeFailure: String?
}

final class CaptureController: @unchecked Sendable {

    private struct Active {
        let recordingId: String
        let recorder: CaptureRecorder
        let handle: FileHandle
        let payloadPartial: URL
        let payloadPublished: URL
        let manifestPartial: URL
        let manifestPublished: URL
        let directory: URL
        let startedAtMs: UInt64
    }

    private let lock = NSLock()
    private var active: Active?
    private var writeFailure: String?

    var isRecording: Bool {
        lock.lock()
        defer { lock.unlock() }
        return active != nil
    }

    func stats() -> CaptureStats? {
        lock.lock()
        defer { lock.unlock() }
        guard let active else { return nil }
        return CaptureStats(
            recordingId: active.recordingId,
            startedAtMs: active.startedAtMs,
            messagesReceived: active.recorder.messagesReceived(),
            acceptedSampleCount: active.recorder.acceptedSampleCount(),
            writeFailure: writeFailure)
    }

    // MARK: - lifecycle

    func start(
        recordingId: String, build: CaptureBuildIdentity, startedAtMs: UInt64, directory: URL
    ) throws {
        lock.lock()
        defer { lock.unlock() }
        guard active == nil else { throw CaptureError.alreadyRecording }

        let published = CaptureStore.payloadURL(for: recordingId, in: directory)
        let partial = CaptureStore.partialURL(published)
        guard FileManager.default.createFile(atPath: partial.path, contents: nil) else {
            throw CaptureError.couldNotCreateFile(partial)
        }
        let handle: FileHandle
        do {
            handle = try FileHandle(forWritingTo: partial)
        } catch {
            try? FileManager.default.removeItem(at: partial)
            throw error
        }
        let manifestPublished = CaptureStore.manifestURL(for: recordingId, in: directory)
        active = Active(
            recordingId: recordingId,
            recorder: CaptureRecorder(
                recordingId: recordingId, build: build, startedAtMs: startedAtMs),
            handle: handle,
            payloadPartial: partial,
            payloadPublished: published,
            manifestPartial: CaptureStore.partialURL(manifestPublished),
            manifestPublished: manifestPublished,
            directory: directory,
            startedAtMs: startedAtMs)
        writeFailure = nil
    }

    /// Called for EVERY received text frame, including malformed ones — the
    /// core counts them as rejected and preserves them verbatim. A capture
    /// that quietly dropped bad frames would misrepresent the stream.
    func append(payload: String, nowMs: UInt64) {
        lock.lock()
        defer { lock.unlock() }
        guard let active, writeFailure == nil else { return }
        let line = active.recorder.onMessage(payload: payload, nowMs: nowMs)
        guard let data = (line + "\n").data(using: .utf8) else {
            writeFailure = "line was not encodable as UTF-8"
            return
        }
        do {
            try active.handle.write(contentsOf: data)
        } catch {
            // The core has already counted this message and cannot un-count
            // it, so the manifest would over-claim. Latch and discard later.
            writeFailure = error.localizedDescription
        }
    }

    /// Seal and publish, or discard. Publication is: fsync payload → digest →
    /// seal manifest → fsync manifest → rename PAYLOAD → rename MANIFEST.
    /// Payload first so a discoverable manifest never points at a file that
    /// is not there yet.
    func finish(nowMs: UInt64) throws -> CaptureManifest {
        lock.lock()
        defer {
            active = nil
            writeFailure = nil
            lock.unlock()
        }
        guard let active else { throw CaptureError.notRecording }

        try? active.handle.synchronize()
        try? active.handle.close()

        if let failure = writeFailure {
            discardPartials(active)
            throw CaptureError.writeFailed(failure)
        }

        do {
            let (byteSize, sha256) = try CaptureStore.digest(of: active.payloadPartial)
            let manifest = active.recorder.finish(
                endedAtMs: nowMs, payloadByteSize: byteSize, payloadSha256Hex: sha256)

            let manifestData = try CaptureStore.encodeManifest(manifest)
            try manifestData.write(to: active.manifestPartial, options: .atomic)
            let manifestHandle = try FileHandle(forUpdating: active.manifestPartial)
            try? manifestHandle.synchronize()
            try? manifestHandle.close()

            // Both renames are same-directory, so each is atomic. Stale
            // published files would make moveItem throw, so clear them first
            // (a re-used recording id is the operator overwriting on purpose).
            try? FileManager.default.removeItem(at: active.payloadPublished)
            try FileManager.default.moveItem(
                at: active.payloadPartial, to: active.payloadPublished)
            try? FileManager.default.removeItem(at: active.manifestPublished)
            try FileManager.default.moveItem(
                at: active.manifestPartial, to: active.manifestPublished)
            CaptureStore.fsyncDirectory(active.directory)
            return manifest
        } catch {
            discardPartials(active)
            throw error
        }
    }

    private func discardPartials(_ active: Active) {
        try? FileManager.default.removeItem(at: active.payloadPartial)
        try? FileManager.default.removeItem(at: active.manifestPartial)
    }
}

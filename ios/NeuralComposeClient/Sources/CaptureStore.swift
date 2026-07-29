// Golden-capture persistence for the Muse gate. The FILESYSTEM lives here —
// names, bytes, digests, atomic publication, discovery. The ENVELOPE lives in
// the Rust core: what a line says, what a manifest claims, and whether a file
// can reproduce its own manifest. Nothing in this file parses EEG, and the
// manifest field names below are a mirror of the Rust `CaptureManifest`
// camelCase, not an independent schema.

import CryptoKit
import Darwin
import Foundation
import NeuralComposeCore

/// A published recording: manifest readable, both files named canonically.
struct CapturedRecording: Identifiable, Sendable {
    let manifest: CaptureManifest
    let manifestURL: URL
    let payloadURL: URL
    /// Payload size as it sits on disk right now. `nil` means the payload is
    /// gone — a manifest making a claim about a file that isn't there.
    let payloadByteSizeOnDisk: UInt64?

    var id: String { manifest.recordingId }

    /// Cheap pre-replay smell test. Full proof is `verifyCapture`; this just
    /// makes an obviously broken pair visible without reading megabytes.
    var integrityNote: String? {
        guard let onDisk = payloadByteSizeOnDisk else { return "payload file missing" }
        if onDisk != manifest.payloadByteSize {
            return "payload is \(onDisk) B, manifest claims \(manifest.payloadByteSize) B"
        }
        return nil
    }
}

enum CaptureStore {

    // MARK: - locations

    static var documentsURL: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }

    /// Derived from the core rather than hardcoded, so a rename in Rust can
    /// never leave iOS writing files Android cannot discover.
    static var manifestSuffix: String { captureManifestFilename(recordingId: "") }
    static var payloadSuffix: String { capturePayloadFilename(recordingId: "") }

    static func payloadURL(for recordingId: String, in dir: URL = documentsURL) -> URL {
        dir.appendingPathComponent(capturePayloadFilename(recordingId: recordingId))
    }

    static func manifestURL(for recordingId: String, in dir: URL = documentsURL) -> URL {
        dir.appendingPathComponent(captureManifestFilename(recordingId: recordingId))
    }

    static func partialURL(_ published: URL) -> URL {
        published.deletingLastPathComponent()
            .appendingPathComponent(published.lastPathComponent + partialSuffix())
    }

    // MARK: - manifest JSON

    /// Wire mirror of the Rust record. Key names are load-bearing: they must
    /// match `#[serde(rename_all = "camelCase")]` on `CaptureManifest`.
    private struct StoredManifest: Codable {
        let schemaId: String
        let lineSchemaId: String
        let recordingId: String
        let platform: String
        let osVersion: String
        let appVersion: String
        let gitCommit: String
        let bridgeLocality: String
        let startedAtMonotonicMs: UInt64
        let endedAtMonotonicMs: UInt64
        let durationMs: UInt64
        let messagesReceived: UInt64
        let acceptedSampleCount: UInt64
        let rejectedMessageCount: UInt64
        let firstSourceTimestamp: Double?
        let lastSourceTimestamp: Double?
        let channelOrder: [String]
        let payloadByteSize: UInt64
        let payloadSha256Hex: String

        init(_ m: CaptureManifest) {
            schemaId = m.schemaId
            lineSchemaId = m.lineSchemaId
            recordingId = m.recordingId
            platform = m.platform
            osVersion = m.osVersion
            appVersion = m.appVersion
            gitCommit = m.gitCommit
            bridgeLocality = CaptureStore.string(for: m.bridgeLocality)
            startedAtMonotonicMs = m.startedAtMonotonicMs
            endedAtMonotonicMs = m.endedAtMonotonicMs
            durationMs = m.durationMs
            messagesReceived = m.messagesReceived
            acceptedSampleCount = m.acceptedSampleCount
            rejectedMessageCount = m.rejectedMessageCount
            firstSourceTimestamp = m.firstSourceTimestamp
            lastSourceTimestamp = m.lastSourceTimestamp
            channelOrder = m.channelOrder
            payloadByteSize = m.payloadByteSize
            payloadSha256Hex = m.payloadSha256Hex
        }

        /// Explicit so an absent source timestamp is written as `null` rather
        /// than dropped — a reader must not have to guess whether the key was
        /// missing because there were no accepted samples or because the
        /// writer omitted it.
        func encode(to encoder: any Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(schemaId, forKey: .schemaId)
            try c.encode(lineSchemaId, forKey: .lineSchemaId)
            try c.encode(recordingId, forKey: .recordingId)
            try c.encode(platform, forKey: .platform)
            try c.encode(osVersion, forKey: .osVersion)
            try c.encode(appVersion, forKey: .appVersion)
            try c.encode(gitCommit, forKey: .gitCommit)
            try c.encode(bridgeLocality, forKey: .bridgeLocality)
            try c.encode(startedAtMonotonicMs, forKey: .startedAtMonotonicMs)
            try c.encode(endedAtMonotonicMs, forKey: .endedAtMonotonicMs)
            try c.encode(durationMs, forKey: .durationMs)
            try c.encode(messagesReceived, forKey: .messagesReceived)
            try c.encode(acceptedSampleCount, forKey: .acceptedSampleCount)
            try c.encode(rejectedMessageCount, forKey: .rejectedMessageCount)
            try c.encode(firstSourceTimestamp, forKey: .firstSourceTimestamp)
            try c.encode(lastSourceTimestamp, forKey: .lastSourceTimestamp)
            try c.encode(channelOrder, forKey: .channelOrder)
            try c.encode(payloadByteSize, forKey: .payloadByteSize)
            try c.encode(payloadSha256Hex, forKey: .payloadSha256Hex)
        }

        func core() throws -> CaptureManifest {
            guard let locality = CaptureStore.locality(from: bridgeLocality) else {
                throw CaptureError.unknownBridgeLocality(bridgeLocality)
            }
            return CaptureManifest(
                schemaId: schemaId, lineSchemaId: lineSchemaId, recordingId: recordingId,
                platform: platform, osVersion: osVersion, appVersion: appVersion,
                gitCommit: gitCommit, bridgeLocality: locality,
                startedAtMonotonicMs: startedAtMonotonicMs,
                endedAtMonotonicMs: endedAtMonotonicMs, durationMs: durationMs,
                messagesReceived: messagesReceived, acceptedSampleCount: acceptedSampleCount,
                rejectedMessageCount: rejectedMessageCount,
                firstSourceTimestamp: firstSourceTimestamp,
                lastSourceTimestamp: lastSourceTimestamp, channelOrder: channelOrder,
                payloadByteSize: payloadByteSize, payloadSha256Hex: payloadSha256Hex)
        }
    }

    static func encodeManifest(_ manifest: CaptureManifest) throws -> Data {
        let encoder = JSONEncoder()
        // Deterministic bytes: two devices recording the same stream should
        // differ only where the facts differ, never in key order.
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return try encoder.encode(StoredManifest(manifest))
    }

    static func decodeManifest(_ data: Data) throws -> CaptureManifest {
        try JSONDecoder().decode(StoredManifest.self, from: data).core()
    }

    static func string(for locality: BridgeLocality) -> String {
        switch locality {
        case .localNetwork: return "localNetwork"
        case .remoteEndpoint: return "remoteEndpoint"
        }
    }

    static func locality(from raw: String) -> BridgeLocality? {
        switch raw {
        case "localNetwork": return .localNetwork
        case "remoteEndpoint": return .remoteEndpoint
        default: return nil
        }
    }

    /// Recorded so a capture taken through a remote relay can never be
    /// mistaken for one taken off the LAN bridge.
    static func bridgeLocality(for url: URL) -> BridgeLocality {
        guard let host = url.host?.lowercased(), !host.isEmpty else { return .remoteEndpoint }
        if host == "localhost" || host == "::1" || host.hasSuffix(".local") {
            return .localNetwork
        }
        let octets = host.split(separator: ".", omittingEmptySubsequences: false)
            .compactMap { Int($0) }
        guard octets.count == 4, octets.allSatisfy({ (0...255).contains($0) }) else {
            return .remoteEndpoint
        }
        switch (octets[0], octets[1]) {
        case (127, _), (10, _), (192, 168), (169, 254): return .localNetwork
        case (172, 16...31): return .localNetwork
        default: return .remoteEndpoint
        }
    }

    // MARK: - bytes

    /// Streamed so a long capture is never held in memory just to be hashed.
    /// Lowercase hex to match the core's `sha256_hex`.
    static func digest(of url: URL) throws -> (byteSize: UInt64, sha256Hex: String) {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        var total: UInt64 = 0
        while let chunk = try handle.read(upToCount: 1 << 20), !chunk.isEmpty {
            hasher.update(data: chunk)
            total += UInt64(chunk.count)
        }
        return (total, hasher.finalize().map { String(format: "%02x", $0) }.joined())
    }

    /// A rename is only durable once the DIRECTORY entry is on disk. Failure
    /// is not fatal (the rename itself already happened) so this is advisory.
    static func fsyncDirectory(_ url: URL) {
        let fd = Darwin.open(url.path, O_RDONLY)
        guard fd >= 0 else { return }
        _ = Darwin.fsync(fd)
        _ = Darwin.close(fd)
    }

    // MARK: - discovery

    struct Listing: Sendable {
        var recordings: [CapturedRecording] = []
        /// Manifests that exist but could not be read. Surfaced, never
        /// silently skipped — an unreadable manifest is a finding.
        var unreadable: [String] = []
        /// `.partial` leftovers from an interrupted run. Not recordings.
        var partials: [String] = []
    }

    static func list(in dir: URL = documentsURL) -> Listing {
        var listing = Listing()
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
        for name in names.sorted() {
            if name.hasSuffix(partialSuffix()) {
                // Only OUR partials — Documents is shared with the journal,
                // and calling someone else's temp file an incomplete capture
                // would be a false claim.
                let stem = String(name.dropLast(partialSuffix().count))
                if stem.hasSuffix(manifestSuffix) || stem.hasSuffix(payloadSuffix) {
                    listing.partials.append(name)
                }
                continue
            }
            guard name.hasSuffix(manifestSuffix) else { continue }
            let manifestURL = dir.appendingPathComponent(name)
            guard let data = try? Data(contentsOf: manifestURL),
                let manifest = try? decodeManifest(data)
            else {
                listing.unreadable.append(name)
                continue
            }
            let payload = payloadURL(for: manifest.recordingId, in: dir)
            let attrs = try? FileManager.default.attributesOfItem(atPath: payload.path)
            let size = (attrs?[.size] as? NSNumber)?.uint64Value
            listing.recordings.append(
                CapturedRecording(
                    manifest: manifest, manifestURL: manifestURL, payloadURL: payload,
                    payloadByteSizeOnDisk: size))
        }
        // Ids embed epoch millis, so lexical descending is newest-first.
        listing.recordings.sort { $0.id > $1.id }
        return listing
    }

    // MARK: - replay

    static func verify(payloadURL: URL, manifest: CaptureManifest) -> ReplayVerdict? {
        guard let jsonl = try? String(contentsOf: payloadURL, encoding: .utf8) else { return nil }
        return verifyCapture(jsonl: jsonl, manifest: manifest)
    }

    static func describe(_ verdict: ReplayVerdict) -> String {
        switch verdict {
        case .verified(let count):
            return "Verified — \(count) samples replayed through the core decoder"
        case .failed(let failure):
            return "FAILED — \(describe(failure))"
        }
    }

    static func describe(_ failure: ReplayFailure) -> String {
        switch failure {
        case .manifestSchemaMismatch: return "manifest schema mismatch"
        case .payloadDigestMismatch: return "payload digest mismatch"
        case .payloadSizeMismatch: return "payload size mismatch"
        case .malformedLine(let n): return "malformed line \(n)"
        case .lineSchemaMismatch(let n): return "line schema mismatch at line \(n)"
        case .sequenceOutOfOrder(let n): return "sequence out of order at line \(n)"
        case .receiveTimeWentBackwards(let n): return "receive time went backwards at line \(n)"
        case .acceptedCountMismatch(let n): return "accepted count mismatch at line \(n)"
        case .messageCountMismatch: return "message count mismatch"
        case .acceptedSampleCountMismatch: return "accepted sample count mismatch"
        case .rejectedMessageCountMismatch: return "rejected message count mismatch"
        case .sourceTimestampNotMonotonic(let n):
            return "source timestamp not monotonic at line \(n)"
        case .nonFiniteChannel(let n): return "non-finite channel at line \(n)"
        case .wrongChannelCount(let n): return "wrong channel count at line \(n)"
        case .firstSourceTimestampMismatch: return "first source timestamp mismatch"
        case .lastSourceTimestampMismatch: return "last source timestamp mismatch"
        case .channelOrderMismatch: return "channel order mismatch"
        }
    }

    // MARK: - delete

    /// Manifest first: an orphan payload is inert, but an orphan manifest is a
    /// claim about a file that no longer exists.
    static func delete(_ recording: CapturedRecording) -> String {
        var removed: [String] = []
        var failed: [String] = []
        for url in [recording.manifestURL, recording.payloadURL] {
            guard FileManager.default.fileExists(atPath: url.path) else { continue }
            do {
                try FileManager.default.removeItem(at: url)
                removed.append(url.lastPathComponent)
            } catch {
                failed.append("\(url.lastPathComponent) (\(error.localizedDescription))")
            }
        }
        fsyncDirectory(recording.manifestURL.deletingLastPathComponent())
        if !failed.isEmpty {
            return "Delete INCOMPLETE for \(recording.id) — could not remove \(failed.joined(separator: ", "))"
        }
        if removed.isEmpty {
            return "Nothing to delete for \(recording.id) — both files were already gone"
        }
        return "Deleted \(recording.id) — removed \(removed.joined(separator: " + "))"
    }
}

enum CaptureError: LocalizedError {
    case alreadyRecording
    case notRecording
    case couldNotCreateFile(URL)
    case writeFailed(String)
    case unknownBridgeLocality(String)

    var errorDescription: String? {
        switch self {
        case .alreadyRecording: return "a recording is already in progress"
        case .notRecording: return "no recording is in progress"
        case .couldNotCreateFile(let url):
            return "could not create \(url.lastPathComponent)"
        case .writeFailed(let why): return "write failed — \(why)"
        case .unknownBridgeLocality(let raw): return "unknown bridgeLocality \"\(raw)\""
        }
    }
}

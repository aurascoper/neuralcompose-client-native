// M6 iOS shell: AVAudioSession/AVAudioRecorder/AVAudioPlayer + files stay
// HERE; every state decision comes from the Rust AudioLifecycle. Manifests
// persist as local JSON (metadata only — audio bytes are .m4a files in
// Documents/recordings).

import AVFoundation
import Foundation
import NeuralComposeCore

@MainActor
final class JournalModel: NSObject, ObservableObject {
    @Published var snapshot: AudioSnapshot

    private let lifecycle: AudioLifecycle
    private var recorder: AVAudioRecorder?
    private var player: AVAudioPlayer?
    private var activeURL: URL?
    private var recordStartedAt: UInt64 = 0
    private var recordStartedWall: UInt64 = 0

    private static var recordingsDir: URL {
        let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("recordings", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private static var manifestURL: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("recording-manifests.json")
    }

    private static func nowMs() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_UPTIME_RAW) / 1_000_000
    }

    override init() {
        lifecycle = AudioLifecycle.withManifests(manifests: Self.loadManifests())
        snapshot = lifecycle.snapshot()
        super.init()
        NotificationCenter.default.addObserver(
            self, selector: #selector(handleInterruption),
            name: AVAudioSession.interruptionNotification, object: nil)
    }

    func start() {
        AVAudioSession.sharedInstance().requestRecordPermission { [weak self] granted in
            Task { @MainActor in
                guard let self else { return }
                _ = self.lifecycle.onPermission(granted: granted, nowMs: Self.nowMs())
                self.refresh()
            }
        }
    }

    private func refresh() {
        snapshot = lifecycle.snapshot()
    }

    func toggleRecord() {
        if case .recording = snapshot.phase {
            stopRecording()
        } else {
            startRecording()
        }
    }

    private func startRecording() {
        guard lifecycle.onRecordStart(nowMs: Self.nowMs()) else { return }
        let url = Self.recordingsDir.appendingPathComponent("\(UUID().uuidString).m4a")
        activeURL = url
        recordStartedAt = Self.nowMs()
        recordStartedWall = UInt64(Date().timeIntervalSince1970 * 1000)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: .defaultToSpeaker)
            try session.setActive(true)
            recorder = try AVAudioRecorder(
                url: url,
                settings: [
                    AVFormatIDKey: kAudioFormatMPEG4AAC,
                    AVSampleRateKey: 44_100,
                    AVNumberOfChannelsKey: 1,
                ])
            recorder?.record()
        } catch {
            // Recorder never started: interruption + recovery → Ready; no entry.
            _ = lifecycle.onInterruption(nowMs: Self.nowMs())
            _ = lifecycle.onInterruptionEnded(nowMs: Self.nowMs())
            try? FileManager.default.removeItem(at: url)
            activeURL = nil
        }
        refresh()
    }

    private func stopRecording() {
        guard lifecycle.onRecordStop(nowMs: Self.nowMs()) else { return }
        refresh()
        let durationMs = Self.nowMs() - recordStartedAt
        recorder?.stop()
        recorder = nil
        if let url = activeURL, let bytes = try? Data(contentsOf: url) {
            _ = lifecycle.onPersisted(
                id: url.deletingPathExtension().lastPathComponent,
                createdAtMs: recordStartedWall,
                durationMs: durationMs,
                format: "m4a",
                byteSize: UInt64(bytes.count),
                sha256Hex: sha256Hex(bytes: bytes),
                nowMs: Self.nowMs())
            Self.saveManifests(lifecycle.snapshot().manifests)
        } else {
            _ = lifecycle.onPersistFailed(reason: "recording file missing", nowMs: Self.nowMs())
        }
        activeURL = nil
        refresh()
    }

    func togglePlay() {
        if case .playing = snapshot.phase {
            _ = lifecycle.onPlayStop(nowMs: Self.nowMs())
            player?.stop()
            player = nil
        } else if let manifest = snapshot.manifests.last,
            lifecycle.onPlayStart(nowMs: Self.nowMs())
        {
            let url = Self.recordingsDir.appendingPathComponent("\(manifest.id).m4a")
            player = try? AVAudioPlayer(contentsOf: url)
            player?.delegate = self
            if player?.play() != true {
                _ = lifecycle.onPlayStop(nowMs: Self.nowMs())
            }
        }
        refresh()
    }

    func acknowledgeFailure() {
        _ = lifecycle.onFailureAcknowledged(nowMs: Self.nowMs())
        refresh()
    }

    @objc private nonisolated func handleInterruption(_ note: Notification) {
        Task { @MainActor in
            guard let info = note.userInfo,
                let typeRaw = info[AVAudioSessionInterruptionTypeKey] as? UInt,
                let type = AVAudioSession.InterruptionType(rawValue: typeRaw)
            else { return }
            switch type {
            case .began:
                if self.lifecycle.onInterruption(nowMs: Self.nowMs()) {
                    self.recorder?.stop()
                    self.recorder = nil
                    if let url = self.activeURL {
                        try? FileManager.default.removeItem(at: url)
                        self.activeURL = nil
                    }
                    self.player?.stop()
                    self.player = nil
                }
            case .ended:
                _ = self.lifecycle.onInterruptionEnded(nowMs: Self.nowMs())
            @unknown default:
                break
            }
            self.refresh()
        }
    }

    // MARK: manifest persistence (metadata only)

    private struct StoredManifest: Codable {
        var id: String
        var createdAtMs: UInt64
        var durationMs: UInt64
        var format: String
        var byteSize: UInt64
        var sha256Hex: String
    }

    private static func loadManifests() -> [RecordingManifest] {
        guard let data = try? Data(contentsOf: manifestURL),
            let stored = try? JSONDecoder().decode([StoredManifest].self, from: data)
        else { return [] }
        return stored.map {
            RecordingManifest(
                id: $0.id, createdAtMs: $0.createdAtMs, durationMs: $0.durationMs,
                format: $0.format, byteSize: $0.byteSize, sha256Hex: $0.sha256Hex)
        }
    }

    private static func saveManifests(_ manifests: [RecordingManifest]) {
        let stored = manifests.map {
            StoredManifest(
                id: $0.id, createdAtMs: $0.createdAtMs, durationMs: $0.durationMs,
                format: $0.format, byteSize: $0.byteSize, sha256Hex: $0.sha256Hex)
        }
        if let data = try? JSONEncoder().encode(stored) {
            try? data.write(to: manifestURL)
        }
    }
}

extension JournalModel: AVAudioPlayerDelegate {
    nonisolated func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool)
    {
        Task { @MainActor in
            _ = self.lifecycle.onPlayStop(nowMs: Self.nowMs())
            self.refresh()
        }
    }
}

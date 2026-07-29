// The shell side of the contract: this class owns the socket and timers,
// feeds raw frames + MONOTONIC timestamps into the Rust core, and renders
// whatever the core says. It never derives stream health from socket state.
//
// It also owns the golden-capture gate's recording controls. The same frame
// text goes to the StreamMonitor and to the CaptureRecorder, with the SAME
// monotonic timestamp, so what was recorded is what the live view saw.

import Foundation
import NeuralComposeCore
import UIKit

@MainActor
final class EEGStreamModel: NSObject, ObservableObject {
    @Published var presentation: Presentation
    @Published var snapshot: ChannelSnapshot

    /// Operator-editable endpoint. `127.0.0.1` means the PHONE on a physical
    /// device, so the gate needs the bridge's LAN address here.
    @Published var endpointText: String
    @Published var endpointError: String?
    @Published private(set) var connectedEndpoint: String

    @Published private(set) var captureStats: CaptureStats?
    @Published private(set) var elapsedMs: UInt64 = 0
    @Published private(set) var listing = CaptureStore.Listing()
    /// Result of the last publish / delete / start attempt.
    @Published var captureMessage: String?
    /// Replay verdicts keyed by recording id.
    @Published var verdicts: [String: String] = [:]

    private let monitor: StreamMonitor
    private let capture = CaptureController()
    private var session: URLSession!
    private var task: URLSessionWebSocketTask?
    private var refreshTimer: Timer?
    private(set) var url: URL

    private static let endpointDefaultsKey = "eeg.endpointURL"
    static let fallbackEndpoint = "ws://127.0.0.1:8787/api/eeg/stream"

    /// Monotonic milliseconds — never wall clock (suspend/resume would
    /// otherwise fabricate huge stale ages).
    nonisolated static func nowMs() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_UPTIME_RAW) / 1_000_000
    }

    override init() {
        let stored =
            UserDefaults.standard.string(forKey: Self.endpointDefaultsKey)
            ?? Self.fallbackEndpoint
        let resolved = URL(string: stored) ?? URL(string: Self.fallbackEndpoint)!
        self.endpointText = stored
        self.connectedEndpoint = resolved.absoluteString
        self.url = resolved
        self.monitor = StreamMonitor(
            config: MonitorConfig(
                keepSamples: 1280,
                staleAfterMs: 2000,
                maxReconnectAttempts: 3,
                backoffBaseMs: 500,
                backoffCapMs: 30000
            ))
        self.presentation = monitor.presentation(nowMs: Self.nowMs())
        self.snapshot = monitor.snapshot()
        super.init()
        self.session = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
    }

    func start() {
        reloadRecordings()
        // Re-appearing must not stack a second socket + timer on top of a
        // recording that is still running.
        guard refreshTimer == nil else {
            refresh()
            return
        }
        connect()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    /// A capture in progress outranks the view lifecycle: leaving the tab
    /// must not silently stall a recording the operator believes is running.
    func stop() {
        guard !capture.isRecording else { return }
        refreshTimer?.invalidate()
        refreshTimer = nil
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

    // MARK: - endpoint

    func applyEndpoint() {
        guard !capture.isRecording else {
            endpointError = "Stop the recording before changing the endpoint."
            return
        }
        let trimmed = endpointText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let candidate = URL(string: trimmed),
            let scheme = candidate.scheme?.lowercased(), scheme == "ws" || scheme == "wss",
            let host = candidate.host, !host.isEmpty
        else {
            endpointError = "Expected a ws:// or wss:// URL with a host."
            return
        }
        endpointError = nil
        endpointText = trimmed
        UserDefaults.standard.set(trimmed, forKey: Self.endpointDefaultsKey)
        url = candidate
        connectedEndpoint = candidate.absoluteString

        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        // A new subscription: clear cached data, generations, retry budget —
        // otherwise a latched give-up from the old host would be reported
        // against the new one.
        monitor.reset()
        connect()
        if refreshTimer == nil {
            refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) {
                [weak self] _ in
                Task { @MainActor in self?.refresh() }
            }
        }
        refresh()
    }

    // MARK: - capture

    var isRecording: Bool { capture.isRecording }

    func startRecording() {
        guard !capture.isRecording else { return }
        // Wall clock names the recording; it is never used as a time AXIS.
        let id = "rec-\(UInt64(Date().timeIntervalSince1970 * 1000))"
        let build = CaptureBuildIdentity(
            platform: "ios",
            osVersion: UIDevice.current.systemVersion,
            appVersion: Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString")
                as? String ?? "0.1.0",
            // No build phase stamps a commit into this app, so the honest
            // value is "unknown" rather than a fabricated one.
            gitCommit: Bundle.main.object(forInfoDictionaryKey: "GitCommit") as? String
                ?? "unknown",
            bridgeLocality: CaptureStore.bridgeLocality(for: url))
        do {
            try capture.start(
                recordingId: id, build: build, startedAtMs: Self.nowMs(),
                directory: CaptureStore.documentsURL)
            captureMessage = "Recording \(id)"
        } catch {
            captureMessage = "Could not start: \(error.localizedDescription)"
        }
        refresh()
    }

    func stopRecording() {
        guard capture.isRecording else { return }
        do {
            let manifest = try capture.finish(nowMs: Self.nowMs())
            captureMessage =
                "Published \(manifest.recordingId) — \(manifest.acceptedSampleCount) samples, "
                + "\(manifest.messagesReceived) messages (\(manifest.rejectedMessageCount) rejected), "
                + "\(manifest.durationMs) ms, \(manifest.payloadByteSize) B"
        } catch {
            captureMessage = "Recording DISCARDED, nothing published: \(error.localizedDescription)"
        }
        reloadRecordings()
        refresh()
    }

    func reloadRecordings() {
        listing = CaptureStore.list()
    }

    /// Replay reads the whole payload, so it happens off the main actor.
    func verify(_ recording: CapturedRecording) {
        let id = recording.id
        let payloadURL = recording.payloadURL
        let manifest = recording.manifest
        verdicts[id] = "Verifying…"
        Task.detached(priority: .userInitiated) {
            let text = try? String(contentsOf: payloadURL, encoding: .utf8)
            let description: String
            if let text {
                description = CaptureStore.describe(verifyCapture(jsonl: text, manifest: manifest))
            } else {
                description = "FAILED — payload file could not be read as UTF-8"
            }
            await MainActor.run { self.verdicts[id] = description }
        }
    }

    func delete(_ recording: CapturedRecording) {
        captureMessage = CaptureStore.delete(recording)
        verdicts[recording.id] = nil
        reloadRecordings()
    }

    // MARK: - socket

    private func connect() {
        monitor.onSocketEvent(event: .connecting, nowMs: Self.nowMs())
        refresh()
        let t = session.webSocketTask(with: url)
        task = t
        receiveLoop(t)
        t.resume()
    }

    private nonisolated func receiveLoop(_ t: URLSessionWebSocketTask) {
        t.receive { [weak self] result in
            guard let self else { return }
            switch result {
            case .success(.string(let text)):
                // One clock read for both sinks: the recorded receive time is
                // exactly the time the live monitor was told about.
                let now = Self.nowMs()
                _ = self.monitor.onFrame(text: text, nowMs: now)
                self.capture.append(payload: text, nowMs: now)
                self.receiveLoop(t)
            case .success:
                self.receiveLoop(t)
            case .failure:
                // didCloseWith / didCompleteWithError report the socket event;
                // the receive loop just stops.
                break
            }
        }
    }

    private func handleDisconnect() {
        monitor.onSocketEvent(event: .closed, nowMs: Self.nowMs())
        refresh()
        switch monitor.reconnectDecision() {
        case .retryAfterMs(let delayMs):
            DispatchQueue.main.asyncAfter(deadline: .now() + Double(delayMs) / 1000.0) {
                [weak self] in
                self?.connect()
            }
        case .giveUp:
            refresh()  // phase() now reports Error (latched)
        }
    }

    private func refresh() {
        let now = Self.nowMs()
        presentation = monitor.presentation(nowMs: now)
        snapshot = monitor.snapshot()
        let stats = capture.stats()
        captureStats = stats
        elapsedMs = stats.map { now >= $0.startedAtMs ? now - $0.startedAtMs : 0 } ?? 0
    }
}

extension EEGStreamModel: URLSessionWebSocketDelegate {
    nonisolated func urlSession(
        _ session: URLSession, webSocketTask: URLSessionWebSocketTask,
        didOpenWithProtocol protocol: String?
    ) {
        monitor.onSocketEvent(event: .opened, nowMs: Self.nowMs())
    }

    nonisolated func urlSession(
        _ session: URLSession, webSocketTask: URLSessionWebSocketTask,
        didCloseWith closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?
    ) {
        Task { @MainActor in self.handleDisconnect() }
    }

    nonisolated func urlSession(
        _ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?
    ) {
        if error != nil {
            Task { @MainActor in self.handleDisconnect() }
        }
    }
}

// The shell side of the contract: this class owns the socket and timers,
// feeds raw frames + MONOTONIC timestamps into the Rust core, and renders
// whatever the core says. It never derives stream health from socket state.

import Foundation
import NeuralComposeCore

@MainActor
final class EEGStreamModel: NSObject, ObservableObject {
    @Published var presentation: Presentation
    @Published var snapshot: ChannelSnapshot

    private let monitor: StreamMonitor
    private var session: URLSession!
    private var task: URLSessionWebSocketTask?
    private var refreshTimer: Timer?
    private let url: URL

    /// Monotonic milliseconds — never wall clock (suspend/resume would
    /// otherwise fabricate huge stale ages).
    nonisolated static func nowMs() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_UPTIME_RAW) / 1_000_000
    }

    init(url: URL) {
        self.url = url
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
        connect()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    func stop() {
        refreshTimer?.invalidate()
        refreshTimer = nil
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
    }

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
                _ = self.monitor.onFrame(text: text, nowMs: Self.nowMs())
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
        presentation = monitor.presentation(nowMs: Self.nowMs())
        snapshot = monitor.snapshot()
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

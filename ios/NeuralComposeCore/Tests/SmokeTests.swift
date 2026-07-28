// Swift smoke test through the UniFFI bindings: the Gate 4 sequence driven
// from Swift with synthetic monotonic timestamps. Mirrors (a subset of) the
// Rust gate4_state_machine contract test.

import XCTest
@testable import NeuralComposeCore

final class SmokeTests: XCTestCase {
    private func makeMonitor() -> StreamMonitor {
        StreamMonitor(
            config: MonitorConfig(
                keepSamples: 1280,
                staleAfterMs: 2000,
                maxReconnectAttempts: 3,
                backoffBaseMs: 500,
                backoffCapMs: 30000
            ))
    }

    func testGate4SequenceThroughBindings() throws {
        let m = makeMonitor()

        m.onSocketEvent(event: .connecting, nowMs: 0)
        XCTAssertEqual(m.phase(nowMs: 0), .connecting)

        m.onSocketEvent(event: .opened, nowMs: 10)
        XCTAssertEqual(m.phase(nowMs: 10), .openNoData)
        XCTAssertEqual(formatLabelEn(p: m.presentation(nowMs: 10)), "OPEN · NO DATA")

        let batch = #"[{"timestamp":0.1,"channels":[1,2,3,4]},{"timestamp":0.2,"channels":[5,6,7,8]}]"#
        XCTAssertEqual(m.onFrame(text: batch, nowMs: 100), 2)
        XCTAssertEqual(m.phase(nowMs: 100), .live)
        XCTAssertEqual(formatLabelEn(p: m.presentation(nowMs: 100)), "OPEN")

        // Open-but-silent → Stale with the oracle banner.
        XCTAssertEqual(m.phase(nowMs: 7100), .stale(ageMs: 7000))
        let p = m.presentation(nowMs: 7100)
        XCTAssertEqual(formatLabelEn(p: p), "STALE 7s")
        XCTAssertEqual(
            formatBannerEn(p: p),
            "Stream silent — no samples for 7s (socket still open)")

        // Malformed frames never refresh sample age.
        XCTAssertEqual(m.onFrame(text: "not json{", nowMs: 8000), 0)
        XCTAssertEqual(m.snapshot().lastReceivedAtMs, 100)

        // Close → retry decision 1000ms → reconnect.
        m.onSocketEvent(event: .closed, nowMs: 9000)
        XCTAssertEqual(m.phase(nowMs: 9000), .closed)
        XCTAssertEqual(m.reconnectDecision(), .retryAfterMs(delayMs: 1000))
        m.onSocketEvent(event: .connecting, nowMs: 10000)
        m.onSocketEvent(event: .opened, nowMs: 10100)

        // M5-A: the new generation has NO freshness — cached data and the
        // previous generation's receive time never authorize Live, and the
        // handshake did not reset the retry budget.
        XCTAssertEqual(m.phase(nowMs: 10100), .openNoData)
        var ss = m.streamSnapshot(nowMs: 10100)
        XCTAssertEqual(ss.connectionGeneration, 2)
        XCTAssertEqual(ss.receivedOnCurrentConnection, 0)
        XCTAssertNil(ss.lastReceivedAtCurrentMs)
        XCTAssertEqual(ss.lastReceivedAtAnyMs, 100)
        XCTAssertEqual(ss.cachedSampleCount, 2)
        XCTAssertEqual(ss.reconnectAttempts, 1)

        // First accepted frame of the generation: Live + budget reset.
        XCTAssertEqual(m.onFrame(text: #"{"timestamp":0.3,"channels":[9,9,9,9]}"#, nowMs: 10150), 1)
        XCTAssertEqual(m.phase(nowMs: 10150), .live)
        ss = m.streamSnapshot(nowMs: 10150)
        XCTAssertEqual(ss.reconnectAttempts, 0)
        XCTAssertEqual(ss.receivedOnCurrentConnection, 1)

        // Snapshot: 4 channels, fixed order.
        let snap = m.snapshot()
        XCTAssertEqual(snap.channels.count, 4)
        XCTAssertEqual(snap.channels.map { $0.last! }, [9, 9, 9, 9])
    }

    func testConfigResolutionNoSilentMockFallback() throws {
        let c = resolveClientMode(useMockRaw: "false", serverRaw: "", wsRaw: "")
        XCTAssertEqual(c.mode, .live)
        XCTAssertEqual(c.configError, "EXPO_PUBLIC_SERVER_URL is not set")

        let ok = resolveClientMode(
            useMockRaw: "false", serverRaw: "http://mac.example:8081/", wsRaw: nil)
        XCTAssertEqual(ok.eegWsUrl, "ws://mac.example:8081/api/eeg/stream")
        XCTAssertNil(ok.configError)
    }
}

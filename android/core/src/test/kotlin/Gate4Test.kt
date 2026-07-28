// Kotlin smoke test through the UniFFI bindings on the host JVM (JNA loads
// the mac cdylib from target/debug). Mirrors the Rust gate4_state_machine
// contract test — same semantics the future Compose shell will consume.

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import uniffi.neuralcompose_mobile_core.ClientMode
import uniffi.neuralcompose_mobile_core.MonitorConfig
import uniffi.neuralcompose_mobile_core.ReconnectDecision
import uniffi.neuralcompose_mobile_core.SocketEvent
import uniffi.neuralcompose_mobile_core.StreamMonitor
import uniffi.neuralcompose_mobile_core.StreamPhase
import uniffi.neuralcompose_mobile_core.formatBannerEn
import uniffi.neuralcompose_mobile_core.formatLabelEn
import uniffi.neuralcompose_mobile_core.resolveClientMode

class Gate4Test {
    private fun monitor() = StreamMonitor(
        MonitorConfig(
            keepSamples = 1280u,
            staleAfterMs = 2000uL,
            maxReconnectAttempts = 3u,
            backoffBaseMs = 500uL,
            backoffCapMs = 30000uL,
        ),
    )

    @Test
    fun gate4SequenceThroughBindings() {
        val m = monitor()

        m.onSocketEvent(SocketEvent.CONNECTING, 0uL)
        assertEquals(StreamPhase.Connecting, m.phase(0uL))

        m.onSocketEvent(SocketEvent.OPENED, 10uL)
        assertEquals(StreamPhase.OpenNoData, m.phase(10uL))
        assertEquals("OPEN · NO DATA", formatLabelEn(m.presentation(10uL)))

        val batch =
            """[{"timestamp":0.1,"channels":[1,2,3,4]},{"timestamp":0.2,"channels":[5,6,7,8]}]"""
        assertEquals(2u, m.onFrame(batch, 100uL))
        assertEquals(StreamPhase.Live, m.phase(100uL))
        assertEquals("OPEN", formatLabelEn(m.presentation(100uL)))

        // Open-but-silent → Stale with the oracle banner.
        assertEquals(StreamPhase.Stale(7000uL), m.phase(7100uL))
        val p = m.presentation(7100uL)
        assertEquals("STALE 7s", formatLabelEn(p))
        assertEquals("Stream silent — no samples for 7s (socket still open)", formatBannerEn(p))

        // Malformed frames never refresh sample age.
        assertEquals(0u, m.onFrame("not json{", 8000uL))
        assertEquals(100uL, m.snapshot().lastReceivedAtMs)

        // Close → retry 1000ms → reconnect → Live.
        m.onSocketEvent(SocketEvent.CLOSED, 9000uL)
        assertEquals(StreamPhase.Closed, m.phase(9000uL))
        assertEquals(ReconnectDecision.RetryAfterMs(1000uL), m.reconnectDecision())
        m.onSocketEvent(SocketEvent.CONNECTING, 10000uL)
        m.onSocketEvent(SocketEvent.OPENED, 10100uL)
        assertEquals(1u, m.onFrame("""{"timestamp":0.3,"channels":[9,9,9,9]}""", 10150uL))
        assertEquals(StreamPhase.Live, m.phase(10150uL))

        // Snapshot: 4 channels, fixed order.
        val snap = m.snapshot()
        assertEquals(4, snap.channels.size)
        assertEquals(listOf(9.0, 9.0, 9.0, 9.0), snap.channels.map { it.last() })
    }

    @Test
    fun configResolutionNeverSilentlyFallsBackToMock() {
        val c = resolveClientMode("false", "", "")
        assertEquals(ClientMode.LIVE, c.mode)
        assertEquals("EXPO_PUBLIC_SERVER_URL is not set", c.configError)

        val ok = resolveClientMode("false", "http://mac.example:8081/", null)
        assertEquals("ws://mac.example:8081/api/eeg/stream", ok.eegWsUrl)
        assertNull(ok.configError)
    }
}

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

        // Close → retry 1000ms → reconnect.
        m.onSocketEvent(SocketEvent.CLOSED, 9000uL)
        assertEquals(StreamPhase.Closed, m.phase(9000uL))
        assertEquals(ReconnectDecision.RetryAfterMs(1000uL), m.reconnectDecision())
        m.onSocketEvent(SocketEvent.CONNECTING, 10000uL)
        m.onSocketEvent(SocketEvent.OPENED, 10100uL)

        // M5-A: the new generation has no freshness — cached data and the
        // previous generation's receive time never authorize Live, and the
        // handshake did not reset the retry budget.
        assertEquals(StreamPhase.OpenNoData, m.phase(10100uL))
        var ss = m.streamSnapshot(10100uL)
        assertEquals(2uL, ss.connectionGeneration)
        assertEquals(0uL, ss.receivedOnCurrentConnection)
        assertEquals(null, ss.lastReceivedAtCurrentMs)
        assertEquals(100uL, ss.lastReceivedAtAnyMs)
        assertEquals(2u, ss.cachedSampleCount)
        assertEquals(1u.toUByte(), ss.reconnectAttempts)

        // First accepted frame of the generation: Live + budget reset.
        assertEquals(1u, m.onFrame("""{"timestamp":0.3,"channels":[9,9,9,9]}""", 10150uL))
        assertEquals(StreamPhase.Live, m.phase(10150uL))
        ss = m.streamSnapshot(10150uL)
        assertEquals(0u.toUByte(), ss.reconnectAttempts)
        assertEquals(1uL, ss.receivedOnCurrentConnection)

        // Snapshot: 4 channels, fixed order.
        val snap = m.snapshot()
        assertEquals(4, snap.channels.size)
        assertEquals(listOf(9.0, 9.0, 9.0, 9.0), snap.channels.map { it.last() })
    }

    @Test
    fun audioLifecycleThroughBindings() {
        val lc = uniffi.neuralcompose_mobile_core.AudioLifecycle()
        assertEquals(true, lc.onPermission(false, 5uL))
        assertEquals(false, lc.onRecordStart(10uL))
        assertEquals(0, lc.snapshot().manifests.size)

        assertEquals(true, lc.onPermission(true, 20uL))
        assertEquals(true, lc.onRecordStart(30uL))
        assertEquals(true, lc.onRecordStop(4030uL))
        val hash = uniffi.neuralcompose_mobile_core.sha256Hex("abc".toByteArray())
        assertEquals("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", hash)
        assertEquals(
            true,
            lc.onPersisted("r1", 1000uL, 4000uL, "m4a", 999uL, hash, 4100uL),
        )
        assertEquals(
            uniffi.neuralcompose_mobile_core.RecordingPhase.Recorded,
            lc.phase(),
        )
        assertEquals(true, lc.onPlayStart(5000uL))
        assertEquals(true, lc.onPlayStop(6000uL))
        assertEquals(1, lc.snapshot().manifests.size)

        val reloaded =
            uniffi.neuralcompose_mobile_core.AudioLifecycle.withManifests(lc.snapshot().manifests)
        assertEquals(uniffi.neuralcompose_mobile_core.RecordingPhase.Idle, reloaded.phase())
        assertEquals(false, reloaded.onRecordStart(1uL))
    }


    private fun makeEntry() = uniffi.neuralcompose_mobile_core.ModelPackCatalogEntry(
        schemaVersion = 1u, packId = "p", packVersion = "1.0.0",
        kind = uniffi.neuralcompose_mobile_core.ModelPackKind.GENERATION,
        modelFamily = "qwen", modelRevision = "r", quantization = null,
        artifactFormat = "gguf", licenseId = "Apache-2.0", sourceRepository = "x",
        runtimeAbi = "abi", minimumCoreVersion = "0.1.0",
        artifacts = listOf(
            uniffi.neuralcompose_mobile_core.ModelArtifact(
                "w", uniffi.neuralcompose_mobile_core.ModelArtifactKind.WEIGHTS,
                "m.gguf", 10uL, "aa".repeat(32),
            ),
            uniffi.neuralcompose_mobile_core.ModelArtifact(
                "t", uniffi.neuralcompose_mobile_core.ModelArtifactKind.TOKENIZER,
                "t.json", 5uL, "bb".repeat(32),
            ),
        ),
        requirements = uniffi.neuralcompose_mobile_core.DeviceRequirements(1024u, "phone"),
        generation = uniffi.neuralcompose_mobile_core.GenerationContract(
            "t", 2048u, "chatml", listOf("focused"),
        ),
        embedding = null,
    )

    @Test
    fun m7aContractsThroughBindings() {
        val entry = makeEntry()
        assertEquals(0, uniffi.neuralcompose_mobile_core.validateCatalogEntry(entry).size)
        val inst = uniffi.neuralcompose_mobile_core.ModelPackInstaller(
            entry, listOf("abi"), 1u, null,
        )
        assertEquals(true, inst.onQueued())
        assertEquals(true, inst.onDownloadComplete())
        assertEquals(false, inst.onPublished(1uL)) // bypass negative path
        val observed = listOf(
            uniffi.neuralcompose_mobile_core.ObservedArtifact("m.gguf", 10uL, "aa".repeat(32)),
            uniffi.neuralcompose_mobile_core.ObservedArtifact("t.json", 5uL, "bb".repeat(32)),
        )
        assertEquals(true, inst.verify(observed))
        assertEquals(true, inst.onPublished(2uL))
        assertEquals(64, inst.activeInstallation()!!.verifiedInventoryDigest.length)
        val rej = uniffi.neuralcompose_mobile_core.ModelPackInstaller(
            entry, listOf("abi"), 1u,
            uniffi.neuralcompose_mobile_core.RestoreResult.Rejected(
                uniffi.neuralcompose_mobile_core.RestoreFailure.TrustedCatalogEntryMissing,
            ),
        )
        assertEquals(false, rej.snapshot().hasUsableActiveInstallation)
        val id = uniffi.neuralcompose_mobile_core.resolveProviderIdentity(
            "nope", "m", "m", null, listOf(), listOf(), null, null,
        )
        assertEquals(null, id.transport)
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

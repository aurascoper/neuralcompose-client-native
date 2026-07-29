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

    private fun observedOk() = listOf(
        uniffi.neuralcompose_mobile_core.ObservedArtifact("m.gguf", 10uL, "aa".repeat(32)),
        uniffi.neuralcompose_mobile_core.ObservedArtifact("t.json", 5uL, "bb".repeat(32)),
    )

    private fun freshInstaller(entry: uniffi.neuralcompose_mobile_core.ModelPackCatalogEntry) =
        uniffi.neuralcompose_mobile_core.ModelPackInstaller(
            entry, listOf("abi"), 1u, null, listOf(), listOf(), listOf(1u),
        )

    @Test
    fun m7aContractsThroughBindings() {
        val entry = makeEntry()
        assertEquals(0, uniffi.neuralcompose_mobile_core.validateCatalogEntry(entry).size)
        val inst = freshInstaller(entry)
        assertEquals(true, inst.onQueued())
        assertEquals(true, inst.onDownloadComplete())
        assertEquals(false, inst.onPublished(1uL)) // bypass negative path
        assertEquals(true, inst.verify(observedOk()))
        assertEquals(true, inst.onPublished(2uL))
        val rec = inst.activeInstallation()!!
        assertEquals(64, rec.verifiedInventoryDigest.length)
        // Sealed restoration: no RestoreResult injection point exists.
        // Tampered on-disk bytes are rejected visibly and never activate.
        val tampered = listOf(
            uniffi.neuralcompose_mobile_core.ObservedArtifact("m.gguf", 10uL, "99".repeat(32)),
            uniffi.neuralcompose_mobile_core.ObservedArtifact("t.json", 5uL, "bb".repeat(32)),
        )
        val rej = uniffi.neuralcompose_mobile_core.ModelPackInstaller(
            entry, listOf("abi"), 1u, rec, tampered, listOf(entry), listOf(1u),
        )
        assertEquals(false, rej.snapshot().hasUsableActiveInstallation)
        assertEquals(
            uniffi.neuralcompose_mobile_core.RestoreFailure.OnDiskInventoryMismatch("m.gguf"),
            rej.snapshot().restoreFailure,
        )
        // Missing trusted entry is equally visible.
        val noTrust = uniffi.neuralcompose_mobile_core.ModelPackInstaller(
            entry, listOf("abi"), 1u, rec, observedOk(), listOf(), listOf(1u),
        )
        assertEquals(
            uniffi.neuralcompose_mobile_core.RestoreFailure.TrustedCatalogEntryMissing,
            noTrust.snapshot().restoreFailure,
        )
        assertEquals(false, noTrust.snapshot().hasUsableActiveInstallation)
        // Polarity: exact bytes + trusted entry restore as usable.
        val good = uniffi.neuralcompose_mobile_core.ModelPackInstaller(
            entry, listOf("abi"), 1u, rec, observedOk(), listOf(entry), listOf(1u),
        )
        assertEquals(true, good.snapshot().hasUsableActiveInstallation)
        val id = uniffi.neuralcompose_mobile_core.resolveProviderIdentity(
            "nope", "m", "m", null, listOf(), listOf(), null, null,
        )
        assertEquals(null, id.transport)
    }

    @Test
    fun m7aRemovalIntegrityThroughBindings() {
        // Dismissing a removal error must never reactivate the pack; only
        // fresh exact revalidation can.
        val inst = freshInstaller(makeEntry())
        assertEquals(true, inst.onQueued())
        assertEquals(true, inst.onDownloadComplete())
        assertEquals(true, inst.verify(observedOk()))
        assertEquals(true, inst.onPublished(1uL))
        assertEquals(true, inst.snapshot().hasUsableActiveInstallation)

        assertEquals(true, inst.onRemovalStarted())
        assertEquals(false, inst.snapshot().hasUsableActiveInstallation)
        assertEquals(true, inst.onRemovalFailed("fs busy"))
        assertEquals(false, inst.snapshot().hasUsableActiveInstallation)
        assertEquals(true, inst.acknowledgeOperationFailure())
        assertEquals(false, inst.snapshot().hasUsableActiveInstallation)
        assertEquals(true, inst.revalidateActive(observedOk()))
        assertEquals(true, inst.snapshot().hasUsableActiveInstallation)
        assertEquals(null, inst.snapshot().activeIntegrityFailure)
    }


    private fun m7a2Target(backend: String, accel: uniffi.neuralcompose_mobile_core.AcceleratorClass) =
        uniffi.neuralcompose_mobile_core.RuntimeTarget(
            os = "android", architecture = "arm64", acceleratorClass = accel,
            backendId = backend, runtimeAbi = "nc-gguf-v1", modelFormats = listOf("gguf"),
            minimumOsVersion = null, minimumBackendVersion = null, minimumDriverVersion = null,
            capabilities = uniffi.neuralcompose_mobile_core.RuntimeCapabilities(
                generation = true, embeddings = false, streaming = true,
                cancellation = true, structuredOutput = false,
            ),
        )

    private fun m7a2Variant(id: String, backend: String, accel: uniffi.neuralcompose_mobile_core.AcceleratorClass) =
        uniffi.neuralcompose_mobile_core.ModelVariant(
            schemaVersion = 1u, logicalModelId = "qwen2.5-0.5b-instruct", variantId = id,
            modelPackId = "local-dialogue-basic", runtimeTarget = m7a2Target(backend, accel),
            quantization = "q4_k_m", artifactFormat = "gguf",
            numericalContractId = "nc-gguf-q4-v1",
        )

    @Test
    fun m7a2SelectionNeverFallsBackThroughBindings() {
        val variants = listOf(
            m7a2Variant("cpu", "llama-cpp-cpu", uniffi.neuralcompose_mobile_core.AcceleratorClass.CPU),
            m7a2Variant("qnn", "windows-ml-qnn", uniffi.neuralcompose_mobile_core.AcceleratorClass.NPU),
        )
        val device = uniffi.neuralcompose_mobile_core.DeviceRuntimeProfile(
            os = "android", architecture = "arm64",
            installedBackendIds = listOf("llama-cpp-cpu"),
            supportedRuntimeAbis = listOf("nc-gguf-v1"),
        )
        val none = uniffi.neuralcompose_mobile_core.RequiredCapabilities(false, false, false)

        // Explicit backend absent → Unavailable, never a silent swap to CPU.
        val denied = uniffi.neuralcompose_mobile_core.selectRuntimeVariant(
            "qwen2.5-0.5b-instruct", variants, device,
            uniffi.neuralcompose_mobile_core.BackendRequirement.Explicit("windows-ml-qnn"), none,
        )
        assertEquals(
            uniffi.neuralcompose_mobile_core.RuntimeSelection.Unavailable(
                uniffi.neuralcompose_mobile_core.SelectionFailure.RequestedBackendNotInstalled(
                    "windows-ml-qnn",
                ),
            ),
            denied,
        )
        // Polarity: no explicit requirement resolves to the installed backend.
        val allowed = uniffi.neuralcompose_mobile_core.selectRuntimeVariant(
            "qwen2.5-0.5b-instruct", variants, device,
            uniffi.neuralcompose_mobile_core.BackendRequirement.AnySupported, none,
        )
        assertEquals(
            true,
            allowed is uniffi.neuralcompose_mobile_core.RuntimeSelection.Selected &&
                allowed.variant.runtimeTarget.backendId == "llama-cpp-cpu",
        )
    }

    @Test
    fun m7a2SupportPromotionAndPropertyLawThroughBindings() {
        // Compiling is not running.
        val built = uniffi.neuralcompose_mobile_core.SupportEvidence(
            contractsAndTestsPass = true, buildsOnNamedTarget = true,
            fixtureRuntimeExecuted = false, physicalDevice = null, osVersion = null,
            backendVersion = null, signedPackagingAccepted = false, acceptanceDocument = null,
        )
        assertEquals(
            uniffi.neuralcompose_mobile_core.SupportStatus.BUILD_VALIDATED,
            uniffi.neuralcompose_mobile_core.attainedSupportStatus(built),
        )
        assertEquals(
            false,
            uniffi.neuralcompose_mobile_core.supportsClaim(
                built, uniffi.neuralcompose_mobile_core.SupportStatus.DEVICE_VALIDATED,
            ),
        )
        // Channel permutation requires labels.
        assertEquals(
            uniffi.neuralcompose_mobile_core.ChannelOrderResult.Ordered(
                listOf(1.0, 2.0, 3.0, 4.0),
            ),
            uniffi.neuralcompose_mobile_core.toCanonicalChannelOrder(
                listOf(3.0, 1.0, 4.0, 2.0), listOf("AF8", "TP9", "TP10", "AF7"),
            ),
        )
        assertEquals(
            uniffi.neuralcompose_mobile_core.ChannelOrderResult.Rejected(
                uniffi.neuralcompose_mobile_core.ChannelPermutationError.LabelsMissing,
            ),
            uniffi.neuralcompose_mobile_core.toCanonicalChannelOrder(
                listOf(3.0, 1.0, 4.0, 2.0), listOf(),
            ),
        )
        // Idempotent indexing; different spaces never share an index.
        val key = uniffi.neuralcompose_mobile_core.IndexEntryKey("11".repeat(32), "22".repeat(32))
        assertEquals(1, uniffi.neuralcompose_mobile_core.dedupeIndexEntries(listOf(key, key)).size)
        assertEquals(
            false,
            uniffi.neuralcompose_mobile_core.sharesIndex(
                key, uniffi.neuralcompose_mobile_core.IndexEntryKey("11".repeat(32), "33".repeat(32)),
            ),
        )
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

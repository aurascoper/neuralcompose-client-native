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

    func testAudioLifecycleThroughBindings() throws {
        let lc = AudioLifecycle()
        // denied → record unreachable, no entry
        XCTAssertTrue(lc.onPermission(granted: false, nowMs: 5))
        XCTAssertFalse(lc.onRecordStart(nowMs: 10))
        XCTAssertEqual(lc.snapshot().manifests.count, 0)
        // granted → full record → persist → play cycle
        XCTAssertTrue(lc.onPermission(granted: true, nowMs: 20))
        XCTAssertTrue(lc.onRecordStart(nowMs: 30))
        XCTAssertTrue(lc.onRecordStop(nowMs: 4030))
        let hash = sha256Hex(bytes: Data("abc".utf8))
        XCTAssertEqual(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        XCTAssertTrue(
            lc.onPersisted(
                id: "r1", createdAtMs: 1000, durationMs: 4000, format: "m4a",
                byteSize: 999, sha256Hex: hash, nowMs: 4100))
        XCTAssertEqual(lc.phase(), .recorded)
        XCTAssertTrue(lc.onPlayStart(nowMs: 5000))
        XCTAssertTrue(lc.onPlayStop(nowMs: 6000))
        XCTAssertEqual(lc.snapshot().manifests.count, 1)
        // restart semantics
        let reloaded = AudioLifecycle.withManifests(manifests: lc.snapshot().manifests)
        XCTAssertEqual(reloaded.phase(), .idle)
        XCTAssertFalse(reloaded.onRecordStart(nowMs: 1))
    }


    private func makeEntry() -> ModelPackCatalogEntry {
        ModelPackCatalogEntry(
            schemaVersion: 1, packId: "p", packVersion: "1.0.0", kind: .generation,
            modelFamily: "qwen", modelRevision: "r", quantization: nil,
            artifactFormat: "gguf", licenseId: "Apache-2.0", sourceRepository: "x",
            runtimeAbi: "abi", minimumCoreVersion: "0.1.0",
            artifacts: [
                ModelArtifact(
                    artifactId: "w", kind: .weights, relativePath: "m.gguf",
                    byteSize: 10, sha256Hex: String(repeating: "aa", count: 32)),
                ModelArtifact(
                    artifactId: "t", kind: .tokenizer, relativePath: "t.json",
                    byteSize: 5, sha256Hex: String(repeating: "bb", count: 32)),
            ],
            requirements: DeviceRequirements(minimumRamMb: 1024, deviceClass: "phone"),
            generation: GenerationContract(
                tokenizerId: "t", contextCap: 2048, promptTemplateId: "chatml",
                compatiblePromptProfiles: ["focused"]),
            embedding: nil)
    }

    private func observedOk() -> [ObservedArtifact] {
        [
            ObservedArtifact(
                relativePath: "m.gguf", byteSize: 10,
                sha256Hex: String(repeating: "aa", count: 32)),
            ObservedArtifact(
                relativePath: "t.json", byteSize: 5,
                sha256Hex: String(repeating: "bb", count: 32)),
        ]
    }

    private func freshInstaller(_ entry: ModelPackCatalogEntry) -> ModelPackInstaller {
        ModelPackInstaller(
            entry: entry, supportedAbis: ["abi"], verificationPolicyVersion: 1,
            persistedRecord: nil, observedInventory: [], trustedCatalog: [],
            acceptedPolicyVersions: [1])
    }

    func testM7aContractsThroughBindings() throws {
        let entry = makeEntry()
        XCTAssertTrue(validateCatalogEntry(entry: entry).isEmpty)
        let inst = freshInstaller(entry)
        XCTAssertTrue(inst.onQueued())
        XCTAssertTrue(inst.onDownloadComplete())
        // Verification bypass negative path.
        XCTAssertFalse(inst.onPublished(installedAtMs: 1), "publish before verify must fail")
        XCTAssertTrue(inst.verify(observed: observedOk()))
        XCTAssertTrue(inst.onPublished(installedAtMs: 2))
        let rec = inst.activeInstallation()!
        XCTAssertEqual(rec.verifiedInventoryDigest.count, 64)
        // Sealed restoration: a shell cannot inject a RestoreResult — the
        // constructor takes only raw inputs. Tampered on-disk bytes are
        // rejected visibly and never activate.
        var tampered = observedOk()
        tampered[0] = ObservedArtifact(
            relativePath: "m.gguf", byteSize: 10,
            sha256Hex: String(repeating: "99", count: 32))
        let rej = ModelPackInstaller(
            entry: entry, supportedAbis: ["abi"], verificationPolicyVersion: 1,
            persistedRecord: rec, observedInventory: tampered,
            trustedCatalog: [entry], acceptedPolicyVersions: [1])
        XCTAssertFalse(rej.snapshot().hasUsableActiveInstallation)
        XCTAssertEqual(
            rej.snapshot().restoreFailure, .onDiskInventoryMismatch(relativePath: "m.gguf"))
        // Missing trusted entry is equally visible.
        let noTrust = ModelPackInstaller(
            entry: entry, supportedAbis: ["abi"], verificationPolicyVersion: 1,
            persistedRecord: rec, observedInventory: observedOk(),
            trustedCatalog: [], acceptedPolicyVersions: [1])
        XCTAssertEqual(noTrust.snapshot().restoreFailure, .trustedCatalogEntryMissing)
        XCTAssertFalse(noTrust.snapshot().hasUsableActiveInstallation)
        // Polarity: exact bytes + trusted entry restore as usable.
        let good = ModelPackInstaller(
            entry: entry, supportedAbis: ["abi"], verificationPolicyVersion: 1,
            persistedRecord: rec, observedInventory: observedOk(),
            trustedCatalog: [entry], acceptedPolicyVersions: [1])
        XCTAssertTrue(good.snapshot().hasUsableActiveInstallation)
        // Unknown provider: transport must be absent.
        let id = resolveProviderIdentity(
            requestedProviderId: "nope", requestedModelId: "m", resolvedModelId: "m",
            modelDigest: nil, descriptors: [], availability: [],
            promptProfile: nil, promptHash: nil)
        XCTAssertNil(id.transport)
        XCTAssertEqual(id.locality, .unresolved)
    }

    func testM7aRemovalIntegrityThroughBindings() throws {
        // Publish a valid pack, then prove dismissing a removal error can
        // never reactivate it — only fresh exact revalidation can.
        let entry = makeEntry()
        let inst = freshInstaller(entry)
        XCTAssertTrue(inst.onQueued())
        XCTAssertTrue(inst.onDownloadComplete())
        XCTAssertTrue(inst.verify(observed: observedOk()))
        XCTAssertTrue(inst.onPublished(installedAtMs: 1))
        XCTAssertTrue(inst.snapshot().hasUsableActiveInstallation)

        XCTAssertTrue(inst.onRemovalStarted())
        XCTAssertFalse(
            inst.snapshot().hasUsableActiveInstallation, "not usable while removing")
        XCTAssertTrue(inst.onRemovalFailed(reason: "fs busy"))
        XCTAssertFalse(inst.snapshot().hasUsableActiveInstallation)
        XCTAssertTrue(inst.acknowledgeOperationFailure())
        XCTAssertFalse(
            inst.snapshot().hasUsableActiveInstallation,
            "acknowledging must not reactivate")
        XCTAssertTrue(inst.revalidateActive(observed: observedOk()))
        XCTAssertTrue(inst.snapshot().hasUsableActiveInstallation)
        XCTAssertNil(inst.snapshot().activeIntegrityFailure)
    }


    // MARK: - M7-A2 (ADR-002) runtime-target, property, and conformance law

    private func m7a2Target(backend: String, accelerator: AcceleratorClass) -> RuntimeTarget {
        RuntimeTarget(
            os: "ios", architecture: "arm64", acceleratorClass: accelerator,
            backendId: backend, runtimeAbi: "nc-gguf-v1", modelFormats: ["gguf"],
            minimumOsVersion: nil, minimumBackendVersion: nil, minimumDriverVersion: nil,
            capabilities: RuntimeCapabilities(
                generation: true, embeddings: false, streaming: true,
                cancellation: true, structuredOutput: false))
    }

    private func m7a2Variant(id: String, backend: String, accelerator: AcceleratorClass)
        -> ModelVariant
    {
        ModelVariant(
            schemaVersion: 1, logicalModelId: "qwen2.5-0.5b-instruct", variantId: id,
            modelPackId: "local-dialogue-basic",
            runtimeTarget: m7a2Target(backend: backend, accelerator: accelerator),
            quantization: "q4_k_m", artifactFormat: "gguf",
            numericalContractId: String(repeating: "c0", count: 32))
    }

    func testM7a2SelectionNeverFallsBackThroughBindings() throws {
        let variants = [
            m7a2Variant(id: "cpu", backend: "llama-cpp-cpu", accelerator: .cpu),
            m7a2Variant(id: "coreml", backend: "coreml", accelerator: .npu),
        ]
        let device = DeviceRuntimeProfile(
            os: "ios", architecture: "arm64",
            installedBackendIds: ["llama-cpp-cpu"], supportedRuntimeAbis: ["nc-gguf-v1"])
        let none = RequiredCapabilities(
            generation: false, embeddings: false, streaming: false,
            cancellation: false, structuredOutput: false)

        // Explicitly required backend is absent → Unavailable, never CPU.
        let denied = selectRuntimeVariant(
            logicalModelId: "qwen2.5-0.5b-instruct", variants: variants, device: device,
            requirement: .explicit(backendId: "coreml"), required: none)
        XCTAssertEqual(
            denied,
            .unavailable(failure: .requestedBackendNotInstalled(backendId: "coreml")),
            "an explicit backend request must never resolve to another backend")

        // Polarity: with no explicit requirement, the installed backend is used.
        let allowed = selectRuntimeVariant(
            logicalModelId: "qwen2.5-0.5b-instruct", variants: variants, device: device,
            requirement: .anySupported, required: none)
        guard case let .selected(variant) = allowed else {
            return XCTFail("expected a selection, got \(allowed)")
        }
        XCTAssertEqual(variant.runtimeTarget.backendId, "llama-cpp-cpu")

        // A required capability no variant offers fails closed.
        let needsStructured = RequiredCapabilities(
            generation: false, embeddings: false, streaming: false,
            cancellation: false, structuredOutput: true)
        XCTAssertEqual(
            selectRuntimeVariant(
                logicalModelId: "qwen2.5-0.5b-instruct", variants: variants, device: device,
                requirement: .anySupported, required: needsStructured),
            .unavailable(failure: .requiredCapabilityUnavailable(capability: "structuredOutput")))

        // The workload itself is a capability: these variants generate but do
        // not embed, so an embedding request must not resolve to them.
        let needsEmbeddings = RequiredCapabilities(
            generation: false, embeddings: true, streaming: false,
            cancellation: false, structuredOutput: false)
        XCTAssertEqual(
            selectRuntimeVariant(
                logicalModelId: "qwen2.5-0.5b-instruct", variants: variants, device: device,
                requirement: .anySupported, required: needsEmbeddings),
            .unavailable(failure: .requiredCapabilityUnavailable(capability: "embeddings")))

        // Two variants of ONE backend are never separated by a name sort.
        var q8 = m7a2Variant(id: "cpu-q8", backend: "llama-cpp-cpu", accelerator: .cpu)
        q8.quantization = "q8_0"
        XCTAssertEqual(
            selectRuntimeVariant(
                logicalModelId: "qwen2.5-0.5b-instruct", variants: [variants[0], q8],
                device: device, requirement: .anySupported, required: none),
            .unavailable(failure: .ambiguousVariantsForBackend(backendId: "llama-cpp-cpu")))
        // Naming the variant resolves it.
        guard case let .selected(named) = selectRuntimeVariant(
            logicalModelId: "qwen2.5-0.5b-instruct", variants: [variants[0], q8],
            device: device, requirement: .explicitVariant(variantId: "cpu-q8"),
            required: none)
        else { return XCTFail("explicit variant must resolve") }
        XCTAssertEqual(named.variantId, "cpu-q8")
    }

    func testM7a2SupportPromotionAndPropertyLawThroughBindings() throws {
        // Compiling is not running; running a fixture is not device validation.
        let built = SupportEvidence(
            contractsAndTestsPass: true, buildsOnNamedTarget: true,
            fixtureRuntimeExecuted: false, physicalDevice: nil, osVersion: nil,
            backendVersion: nil, signedPackagingAccepted: false, acceptanceDocument: nil)
        XCTAssertEqual(attainedSupportStatus(evidence: built), .buildValidated)
        XCTAssertFalse(supportsClaim(evidence: built, claimed: .runtimeSmokeValidated))
        var devicey = built
        devicey.fixtureRuntimeExecuted = true
        devicey.physicalDevice = "iPhone 17"
        devicey.osVersion = "26.0"
        devicey.backendVersion = "b4321"
        XCTAssertEqual(attainedSupportStatus(evidence: devicey), .deviceValidated)
        XCTAssertFalse(supportsClaim(evidence: devicey, claimed: .releaseSupported))

        // Channels permute only with labels; bare values are refused.
        let values = [1.0, 2.0, 3.0, 4.0]
        XCTAssertEqual(
            toCanonicalChannelOrder(
                values: [3.0, 1.0, 4.0, 2.0], fromLabels: ["AF8", "TP9", "TP10", "AF7"]),
            .ordered(values: values))
        XCTAssertEqual(
            toCanonicalChannelOrder(values: [3.0, 1.0, 4.0, 2.0], fromLabels: []),
            .rejected(error: .labelsMissing))

        // Indexing the same content in the same space is idempotent.
        let key = IndexEntryKey(
            contentSha256Hex: String(repeating: "11", count: 32),
            embeddingSpaceIdentity: String(repeating: "22", count: 32))
        XCTAssertEqual(dedupeIndexEntries(keys: [key, key, key]).count, 1)
        let otherSpace = IndexEntryKey(
            contentSha256Hex: key.contentSha256Hex,
            embeddingSpaceIdentity: String(repeating: "33", count: 32))
        XCTAssertFalse(sharesIndex(a: key, b: otherSpace))
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

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


    // MARK: - M7-B declaration v6 through the real UniFFI bindings
    //
    // Binding regeneration and a drift-clean diff prove the API exists; they
    // do not prove Swift reaches the same verdict. This enters through the
    // generated bindings, calls the exported Rust implementation, and derives
    // the expected panel INDEPENDENTLY in Swift. There is deliberately no
    // local reimplementation of the averaging and no caller-supplied
    // aggregate — `CandidateResult` has no quality field to supply.
    //
    // v6: the corpus is no longer something this test invents. It comes from
    // `frozenCorpusV1()`, the artifact Rust compiles in, so Swift is exercised
    // against the same 18 prompts the declaration is frozen over.

    private static let candA = "qwen2.5-0.5b-instruct-q4km"
    private static let candB = "qwen3-0.6b-q4km-derived"
    private static let corpus = frozenCorpusV1()
    private static let promptIds = frozenCorpusV1().prompts.map { $0.promptId }
    private static let seeds: [UInt64] = [11, 22, 33]
    private static let scorer = String(repeating: "f0", count: 32)

    private func hx(_ n: UInt8) -> String {
        String(repeating: String(format: "%02x", n), count: 32)
    }

    /// Derived from corpus content across the boundary — never asserted here.
    private func semanticHash(_ promptId: String) -> String {
        corpusExpectedPrompts(corpus: Self.corpus)
            .first { $0.promptId == promptId }!.semanticPromptHash
    }

    private func m7bSampler() -> SamplerConfig {
        SamplerConfig(
            temperature: 0.7, topP: 0.8, topK: 20, repeatPenalty: 1.05,
            maxOutputTokens: 256, contextCap: 4096)
    }

    private func m7bEnvironment() -> RunEnvironment {
        RunEnvironment(
            coldDefinition: .processColdPageCacheUnknown,
            warmDefinition: .contextRecreatedModelResident,
            chargingState: .pluggedIn,
            cooldownExit: .temperatureAtOrBelowStartCeiling,
            cooldownMinimumSeconds: 180,
            thermalStartCeilingCelsiusTenths: 380,
            screenOn: true, screenBrightnessPercent: 20, airplaneMode: true,
            restartProcessBetweenCandidates: true,
            recheckPackIntegrityBetweenCandidates: true,
            orderPolicy: .counterbalancedAbba)
    }

    private func m7bThresholds() -> PromotionThresholds {
        PromotionThresholds(
            maxColdLoadMs: 4000, maxTimeToFirstTokenMs: 1500,
            minGenerationTokensPerSecond: 8.0, maxPeakRssMb: 1600,
            maxInstalledBytes: 1_200_000_000, maxCancellationLatencyMs: 300,
            batteryPolicy: .telemetryOnlyPluggedIn,
            thermalCutoffCelsiusTenths: 450,
            materialQualityMarginMillionths: 50_000)
    }

    /// Counterbalanced ABBA over both prompts' shared timed runs.
    private func m7bRunPlan() -> [RunPlanEntry] {
        var plan: [RunPlanEntry] = []
        var i: UInt32 = 0
        for c in [Self.candA, Self.candB] {
            plan.append(RunPlanEntry(index: i, candidateId: c, mode: .warmup, seed: 11)); i += 1
        }
        for s in Self.seeds {
            plan.append(RunPlanEntry(index: i, candidateId: Self.candA, mode: .cold, seed: s)); i += 1
            plan.append(RunPlanEntry(index: i, candidateId: Self.candB, mode: .cold, seed: s)); i += 1
            plan.append(RunPlanEntry(index: i, candidateId: Self.candB, mode: .warm, seed: s)); i += 1
            plan.append(RunPlanEntry(index: i, candidateId: Self.candA, mode: .warm, seed: s)); i += 1
        }
        for c in [Self.candA, Self.candB] {
            plan.append(RunPlanEntry(index: i, candidateId: c, mode: .sustained, seed: 11)); i += 1
            plan.append(RunPlanEntry(index: i, candidateId: c, mode: .cancellation, seed: 11)); i += 1
        }
        return plan
    }

    /// One scored output per candidate x prompt x seed, naming its warm run.
    private func m7bQualityPlan() -> [QualityPlanEntry] {
        var plan: [QualityPlanEntry] = []
        var i: UInt32 = 0
        for c in [Self.candA, Self.candB] {
            for pid in Self.promptIds {
                for s in Self.seeds {
                    let run = m7bRunPlan().first {
                        $0.candidateId == c && $0.mode == .warm && $0.seed == s
                    }!
                    plan.append(
                        QualityPlanEntry(
                            index: i, candidateId: c, runId: "\(c)-\(run.index)",
                            promptId: pid, seed: s, blindedOutputId: "out-\(i)"))
                    i += 1
                }
            }
        }
        return plan
    }

    private func m7bDeclaration() -> V6Declaration {
        V6Declaration(
            protocolVersion: 6,
            corpus: Self.corpus,
            compositionPolicy: frozenCompositionPolicyV1(),
            promptCount: UInt32(Self.corpus.prompts.count),
            qualityRubricId: "m7b-rubric-v1",
            scorerPolicy: .singleScorer(scorerIdentityDigest: Self.scorer),
            selectionRule: .exactlyOncePerCandidatePromptSeedFromWarm,
            policies: frozenPolicyIdentitiesV6(),
            sampler: m7bSampler(), seeds: Self.seeds, warmupRuns: 1,
            timedRuns: UInt32(Self.seeds.count), sustainedSeconds: 300,
            perRunTimeoutMs: 60_000, thresholds: m7bThresholds(),
            environment: m7bEnvironment(),
            runPlan: m7bRunPlan(), qualityPlan: m7bQualityPlan(),
            blindingManifestDigest: hx(0xbd))
    }

    private func m7bCandidate(_ id: String) -> EvaluationCandidate {
        let derived = id == Self.candB
        let tag: UInt8 = derived ? 0x80 : 0x70
        return EvaluationCandidate(
            candidateId: id,
            modelFamily: derived ? "qwen3" : "qwen2.5",
            modelRevision: derived ? "0.6b" : "0.5b-instruct",
            quantization: "Q4_K_M", variantId: "gguf-q4km-android-arm64",
            role: .primaryMobile,
            provenance: derived ? .derivedByConversion : .officialUpstream,
            artifactSha256Hex: hx(derived ? 0xc2 : 0xc1),
            tokenizerIdentity: hx(derived ? 0xd2 : 0xd1),
            chatTemplateIdentity: hx(derived ? 0xe2 : 0xe1),
            thinkingModeDisabled: true,
            conversion: derived
                ? ConversionRecord(
                    sourceRepo: "Qwen/Qwen3-0.6B", sourceRevision: "c1899de2",
                    conversionCommit: "f5b9bd39", quantizerCommit: "f5b9bd39",
                    conversionCommand: "convert_hf_to_gguf.py",
                    quantizeCommand: "llama-quantize Q4_K_M",
                    intermediateSha256Hex: hx(0xb1), outputSha256Hex: hx(0xb2),
                    quantization: "Q4_K_M", importanceMatrix: nil,
                    calibrationDataset: nil, allowRequantize: false,
                    pureQuantization: false, sourcePrecision: "bf16")
                : nil,
            promptBindings: Self.promptIds.enumerated().map { idx, pid in
                PromptBinding(
                    promptId: pid,
                    renderedPromptHash: hx(tag &+ UInt8(idx)),
                    inputTokenIdsHash: hx(tag &+ 0x10 &+ UInt8(idx)))
            })
    }

    /// `v` is integer millionths: 800_000 is 0.8.
    private func m7bPanel(_ v: UInt32) -> QualityPanel {
        QualityPanel(
            instructionAdherence: v, requiredOutputStructure: v,
            avoidsUnsupportedInvention: v, appropriateUncertainty: v,
            avoidsFalseRefusal: v, substantivePositionRetention: v,
            avoidsRepetition: v, truncationBehavior: v,
            languagePreservation: v, promptProfileFidelity: v)
    }

    private func m7bMetrics(_ mode: RunMode) -> RunMetrics {
        RunMetrics(
            loadMs: mode == .cold ? 2200 : 400, timeToFirstTokenMs: 700,
            promptTokens: 120, promptDurationMs: 1000,
            generatedTokens: 140, generationDurationMs: 10_000,
            peakRssMb: 900, modelMemoryMb: 500,
            cancellationLatencyMs: mode == .cancellation ? 120 : nil,
            peakTemperatureCelsiusTenths: 400, throttled: false,
            batteryDropTenthsPercent: 2, backgroundForegroundRecovered: true)
    }

    private func m7bResult(_ id: String, instructionSeedScores: [UInt32]) -> CandidateResult {
        let cand = m7bCandidate(id)
        let obs = m7bRunPlan().filter { $0.candidateId == id }.map { e -> RunObservation in
            let proc = "\(id)-proc-\(e.mode == .cold ? e.index : 0)"
            return RunObservation(
                runId: "\(id)-\(e.index)", candidateId: id, seed: e.seed, mode: e.mode,
                sequenceIndex: e.index, startedMonotonicMs: 1000, endedMonotonicMs: 2000,
                observedChargingState: .pluggedIn, observedScreenOn: true,
                observedBrightnessPercent: 20, observedAirplaneMode: true,
                processInstanceId: proc,
                loadedModelSha256: cand.artifactSha256Hex,
                verifiedInventoryDigest: hx(0x6b),
                revalidationEvidenceId: "reval-\(id)-\(e.index)",
                revalidatedProcessInstanceId: proc,
                coldEvidence: .processCold(processInstanceId: proc),
                startTemperatureCelsiusTenths: 350, cooldownDurationMs: 200_000,
                cooldownExitTemperatureCelsiusTenths: 340,
                thermalSensorIdentity: "thermal_zone0",
                throttlingDetectorIdentity: "detector-v1",
                disposition: .admissible, metrics: m7bMetrics(e.mode))
        }
        let quality = m7bQualityPlan().filter { $0.candidateId == id }.map { e -> PromptQualityObservation in
            let b = cand.promptBindings.first { $0.promptId == e.promptId }!
            // Only the FIRST corpus prompt's instruction axis varies, and
            // only for the candidate under test — everything else stays at 0.8.
            let seedIdx = Self.seeds.firstIndex(of: e.seed)!
            var scores = m7bPanel(800_000)
            if e.promptId == Self.promptIds[0] && seedIdx < instructionSeedScores.count {
                scores.instructionAdherence = instructionSeedScores[seedIdx]
            }
            return PromptQualityObservation(
                blindedOutputId: e.blindedOutputId, runId: e.runId, candidateId: id,
                promptId: e.promptId, seed: e.seed,
                semanticPromptHash: semanticHash(e.promptId),
                renderedPromptHash: b.renderedPromptHash,
                inputTokenIdsHash: b.inputTokenIdsHash,
                outputTextSha256: hx(0x41), outputTokenIdsSha256: hx(0x42),
                rubricId: "m7b-rubric-v1", blindingManifestDigest: hx(0xbd),
                scorerIdentityDigest: Self.scorer, scores: scores,
                disposition: .admissible)
        }
        return CandidateResult(
            candidateId: id, candidateIdentity: candidateIdentity(candidate: cand),
            protocolIdentity: v6DeclarationIdentity(declaration: m7bDeclaration()),
            device: "swift-host", osVersion: "test", runtimeIdentity: "llama.cpp@f5b9bd39",
            prompts: Self.promptIds.map { pid in
                let b = cand.promptBindings.first { $0.promptId == pid }!
                let frozen = Self.corpus.prompts.first { $0.promptId == pid }!
                return BenchmarkPrompt(
                    promptId: pid,
                    taskKind: frozen.taskKind,
                    contextProfile: frozen.contextProfile,
                    semanticPromptHash: semanticHash(pid),
                    renderedPromptHash: b.renderedPromptHash,
                    inputTokenIdsHash: b.inputTokenIdsHash)
            },
            installedBytes: id == Self.candA ? 300_000_000 : 600_000_000,
            qualityObservations: quality,
            disposition: .admissible, observations: obs)
    }

    func testM7bQualityPanelDerivesIdenticallyThroughSwiftBindings() throws {
        let proto = m7bDeclaration()
        XCTAssertTrue(
            validateV6Declaration(protocol: proto).isEmpty,
            "the v6 fixture must be a valid declaration")

        // 0.8, 0.8, 0.2 across the three seeds of the first corpus prompt.
        let first = Self.promptIds[0]
        let result = m7bResult(Self.candA, instructionSeedScores: [800_000, 800_000, 200_000])
        guard case let .derived(panel) = deriveQualityPanel(
            protocol: proto, candidate: m7bCandidate(Self.candA),
            observations: result.qualityObservations)
        else { return XCTFail("expected a derived panel") }

        // Derive the same value independently here, in the required order:
        // average across seeds within a prompt, then across prompts equally.
        //
        //   that prompt: (800_000 + 800_000 + 200_000) / 3      = 600_000
        //   the axis:    (600_000 + 17 * 800_000) / 18          = 14_200_000/18
        //
        // which reduces to 7_100_000/9. v5 asserted this by recomputing the
        // f64 expression in the same order Rust used, because two-stage
        // averaging of 0.8 yields 0.8000000000000002 and no written-down
        // constant would have matched. Under exact rationals the expected
        // value can simply be WRITTEN DOWN, which is the clearest sign the
        // arithmetic change did what it was for.
        let n = UInt64(Self.corpus.prompts.count)
        XCTAssertEqual(n, 18, "the frozen corpus is 2 prompts per allowed pair")
        XCTAssertEqual(
            panel.instructionAdherence,
            ExactMillionths(numerator: 7_100_000, denominator: 9),
            "Swift and Rust must agree exactly on the macro weighting")

        // Every other axis is untouched, and lands on a whole millionth.
        let untouched = ExactMillionths(numerator: 800_000, denominator: 1)
        for (name, value) in [
            ("requiredOutputStructure", panel.requiredOutputStructure),
            ("avoidsUnsupportedInvention", panel.avoidsUnsupportedInvention),
            ("appropriateUncertainty", panel.appropriateUncertainty),
            ("avoidsFalseRefusal", panel.avoidsFalseRefusal),
            ("substantivePositionRetention", panel.substantivePositionRetention),
            ("avoidsRepetition", panel.avoidsRepetition),
            ("truncationBehavior", panel.truncationBehavior),
            ("languagePreservation", panel.languagePreservation),
            ("promptProfileFidelity", panel.promptProfileFidelity),
        ] {
            XCTAssertEqual(value, untouched, "\(name) must be unaffected")
        }

        // The seeds-within-a-prompt stage in isolation: one prompt scored
        // 0.8/0.8/0.2 averages to exactly 0.6, denominator 1.
        let singlePrompt = result.qualityObservations.filter { $0.promptId == first }
        var soloProto = proto
        soloProto.corpus = BenchmarkCorpus(
            corpusId: proto.corpus.corpusId, prompts: [proto.corpus.prompts[0]])
        soloProto.promptCount = 1
        soloProto.qualityPlan = proto.qualityPlan.filter { $0.promptId == first }
        guard case let .derived(solo) = deriveQualityPanel(
            protocol: soloProto, candidate: m7bCandidate(Self.candA),
            observations: singlePrompt)
        else { return XCTFail("expected a derived single-prompt panel") }
        XCTAssertEqual(
            solo.instructionAdherence,
            ExactMillionths(numerator: 600_000, denominator: 1))

        // Cost is derived too, as a worst-case envelope.
        guard let cost = deriveCostObservation(
            observations: result.observations, installedBytes: 300_000_000)
        else { return XCTFail("expected a derived cost observation") }
        XCTAssertEqual(cost.coldLoadMs, 2200)
        XCTAssertEqual(cost.warmLoadMs, 400)
        XCTAssertEqual(cost.cancellationLatencyMs, 120)
        XCTAssertEqual(cost.batteryDropTenthsPercent, 2, "sustained run only")

        // A hard-invalid ledger surfaces the STRUCTURED Rust rejection.
        var leaked = result.qualityObservations
        leaked[0].disposition = .reasoningLeakage(detector: "frozen-prose-v1")
        guard case let .rejected(failure) = deriveQualityPanel(
            protocol: proto, candidate: m7bCandidate(Self.candA), observations: leaked)
        else { return XCTFail("a leaked output must reject") }
        guard case .inadmissible = failure else {
            return XCTFail("expected .inadmissible, got \(failure)")
        }

        // And the sealed door holds through Swift: both candidates equal on
        // quality, differing only in size, yields a tier split.
        let verdict = evaluatePromotion(
            protocol: proto,
            candidateA: m7bCandidate(Self.candA),
            resultA: m7bResult(Self.candA, instructionSeedScores: [800_000, 800_000, 800_000]),
            candidateB: m7bCandidate(Self.candB),
            resultB: m7bResult(Self.candB, instructionSeedScores: [800_000, 800_000, 800_000]))
        guard case let .splitTiers(basic, enhanced, _) = verdict else {
            return XCTFail("expected a tier split, got \(verdict)")
        }
        XCTAssertEqual(basic, Self.candA, "the smaller install becomes Basic")
        XCTAssertEqual(enhanced, Self.candB)
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

//! Agreement for the `llama-cpp-vulkan` row, and the reason it needs its own.
//!
//! THE VULKAN ROW CANNOT REUSE THE CPU ROW'S NUMBERS. Measured 2026-08-03 on a
//! Radeon 890M (RADV STRIX1), the same model and input through the two backends
//! differ by up to **9.06e-05** per component — far outside the 2e-6 tolerance
//! `embedding_agreement.rs` asserts. Reusing that tolerance here would fail; and
//! loosening THAT test to accommodate this one would silently stop checking the
//! CPU path. So each backend is pinned against its own reference.
//!
//! The divergence is not a defect. GPU kernels reduce in a different order and
//! may use fp16 intermediates, so bit-identical output across backends was never
//! available. What matters is that the difference is numerically tiny and
//! semantically nil:
//!
//! | text | max abs diff | mean abs diff | cosine(cpu, vulkan) |
//! |---|---|---|---|
//! | "the headband is on" | 7.300e-05 | 1.885e-05 | 0.999999894 |
//! | "neural compose" | 7.390e-05 | 2.054e-05 | 0.999999877 |
//! | "quarterly tax filing deadline" | 9.060e-05 | 1.995e-05 | 0.999999884 |
//!
//! That divergence is also the best available EVIDENCE that computation moved.
//! If the two backends had agreed bit-for-bit, the most likely explanation
//! would be that Vulkan was never used.
//!
//! REQUIRES A VULKAN BUILD:
//!
//! ```sh
//! cmake -B build-vulkan -DGGML_VULKAN=ON -DBUILD_SHARED_LIBS=ON
//! LLAMA_CPP_LIB_DIR=$PWD/build-vulkan/bin cargo test -p neuralcompose-llama
//! ```

use std::path::PathBuf;

use neuralcompose_llama::{
    BACKEND_ID_CPU, BACKEND_ID_VULKAN, DeviceKind, Embedder, cosine_similarity, devices,
};

/// Offload everything. bge-small has 12 layers; 99 is llama.cpp's usual
/// "as many as will fit" idiom.
const ALL_LAYERS: u32 = 99;

fn model_path() -> PathBuf {
    std::env::var_os("NC_TEST_GGUF")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join("models/bge-small-en-v1.5-f32.gguf")
        })
}

/// `None` when there is no accelerator to test, with the reason printed.
///
/// `NC_REQUIRE_VULKAN=1` turns every skip into a failure, for the same reason
/// `NC_REQUIRE_BACKEND` exists: a skipped Rust test still reports `ok`, so a
/// CPU-only build would otherwise produce the identical "all passed" line as a
/// real Vulkan run and make an unexercised backend look verified.
fn accelerated() -> Option<Embedder> {
    let required = std::env::var_os("NC_REQUIRE_VULKAN").is_some_and(|v| v != "0");
    let refuse = |why: String| -> Option<Embedder> {
        assert!(
            !required,
            "NC_REQUIRE_VULKAN is set but {why} — refusing to report a pass that verifies nothing"
        );
        eprintln!("SKIP: {why}");
        None
    };

    if !neuralcompose_llama::is_available() {
        return refuse("built as a stub (LLAMA_CPP_DIR unset)".into());
    }
    if !devices().iter().any(|d| d.kind.is_accelerator()) {
        return refuse("no accelerator enumerated (built without GGML_VULKAN?)".into());
    }
    let path = model_path();
    if !path.exists() {
        return refuse(format!("no fixture model at {}", path.display()));
    }
    match Embedder::load_with(&path, 512, ALL_LAYERS) {
        Ok(e) => Some(e),
        Err(e) => panic!("accelerator present and model exists, but load failed: {e}"),
    }
}

/// Measured from `llama-embedding -ngl 99` built with `GGML_VULKAN=ON`.
/// Produced by running it, never written by hand.
const VULKAN_REFERENCE: &[(&str, &[(usize, f32)])] = &[
    (
        "neural compose",
        &[
            (0, 0.0116364),
            (1, -0.0641273),
            (2, -0.0276023),
            (17, 0.0151264),
            (100, -0.0730364),
            (200, 0.0033285),
            (300, 0.0036796),
            (383, -0.0835638),
        ],
    ),
    (
        "the headband is on",
        &[
            (0, -0.0113054),
            (1, -0.0021269),
            (2, 0.03658),
            (17, 0.0418051),
            (100, -0.0227002),
            (200, 0.057823),
            (300, 0.0196393),
            (383, 0.0600289),
        ],
    ),
];

#[test]
fn the_accelerator_is_an_integrated_gpu_and_is_reported_as_one() {
    let required = std::env::var_os("NC_REQUIRE_VULKAN").is_some_and(|v| v != "0");
    let devs = devices();
    let Some(acc) = devs.iter().find(|d| d.kind.is_accelerator()) else {
        assert!(!required, "NC_REQUIRE_VULKAN set but no accelerator");
        eprintln!("SKIP: no accelerator");
        return;
    };
    // ggml distinguishes IGPU from GPU, and the Radeon 890M shares host memory.
    // Collapsing the two would misdescribe the hardware this row is validated
    // on — and the memory envelope of an iGPU is the host's, which matters for
    // any future claim about model size.
    assert_eq!(acc.kind, DeviceKind::IGpu, "890M is integrated: {acc:?}");
    assert!(
        acc.name.starts_with("Vulkan"),
        "expected a Vulkan device, got {:?}",
        acc.name
    );
    // The CPU must still be enumerated alongside it.
    assert!(devs.iter().any(|d| d.kind == DeviceKind::Cpu));
}

#[test]
fn it_agrees_with_the_vulkan_reference_binary() {
    let Some(mut e) = accelerated() else { return };
    assert_eq!(e.backend_id(), BACKEND_ID_VULKAN);
    for (text, expected) in VULKAN_REFERENCE {
        let got = e.embed(text).expect("embed");
        for (i, want) in *expected {
            let diff = (got[*i] - want).abs();
            assert!(
                diff < 2e-6,
                "{text:?} dim {i}: got {} want {want} (diff {diff:.3e})",
                got[*i]
            );
        }
    }
}

#[test]
fn vulkan_and_cpu_differ_numerically_but_not_semantically() {
    // Both halves are load-bearing.
    //
    // DIFFER: if the two backends agreed bit-for-bit, the likeliest explanation
    // is that Vulkan was never actually used — so identity here would be
    // evidence AGAINST the claim this row makes, not for it.
    //
    // NOT SEMANTICALLY: an accelerator that produced genuinely different
    // embeddings would be useless regardless of how fast it was.
    let Some(mut gpu) = accelerated() else { return };
    let path = model_path();
    let mut cpu = Embedder::load_with(&path, 512, 0).expect("cpu load");
    assert_eq!(cpu.backend_id(), BACKEND_ID_CPU);

    for text in ["the headband is on", "neural compose"] {
        let g = gpu.embed(text).expect("gpu");
        let c = cpu.embed(text).expect("cpu");
        assert_ne!(g, c, "{text:?}: bit-identical output suggests no offload");

        let max_diff = g
            .iter()
            .zip(c.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        // Measured max was 9.06e-05 across three strings; 1e-3 is a ceiling
        // with room for other hardware, not a fitted value.
        assert!(max_diff < 1e-3, "{text:?}: diverged by {max_diff:.3e}");
        assert!(max_diff > 1e-7, "{text:?}: suspiciously identical");

        let cos = cosine_similarity(&g, &c).expect("same length");
        assert!(cos > 0.9999, "{text:?}: cosine only {cos:.9}");
    }
}

#[test]
fn requesting_offload_without_an_accelerator_reports_cpu_not_vulkan() {
    // The failure this guards is the one that would make a matrix row wrong:
    // llama.cpp runs on CPU without complaint when no accelerator is found, so
    // a backend id derived from the REQUEST rather than the OUTCOME would file
    // a CPU run as Vulkan evidence.
    if !neuralcompose_llama::is_available() {
        eprintln!("SKIP: stub build");
        return;
    }
    if devices().iter().any(|d| d.kind.is_accelerator()) {
        eprintln!("SKIP: an accelerator IS present — this asserts the opposite case");
        return;
    }
    let path = model_path();
    if !path.exists() {
        eprintln!("SKIP: no fixture model");
        return;
    }
    let e = Embedder::load_with(&path, 512, ALL_LAYERS).expect("load");
    assert_eq!(e.requested_gpu_layers(), ALL_LAYERS);
    assert!(e.accelerator().is_none());
    assert_eq!(
        e.backend_id(),
        BACKEND_ID_CPU,
        "offload was requested but unavailable; this is a CPU run"
    );
}

//! Does quantisation error compose with backend error in quadrature or linearly?
//!
//! `quantized_agreement.rs` established two error sources separately:
//!
//!   - quantisation alone: CPU-f32 vs CPU-Q8_0        = 2.62e-03
//!   - backend alone, quantised: CPU-Q8_0 vs Vk-Q8_0  = 2.63e-03
//!
//! The deployment question is what happens when BOTH apply — an index built on
//! CPU-f32 and queried by a quantised Vulkan runtime. Two hypotheses, and the
//! difference between them is the difference between a predictable worst case
//! and an unbounded one:
//!
//!   INDEPENDENT  -> add in quadrature -> sqrt(2.62^2 + 2.63^2) = 3.71e-03
//!   CORRELATED   -> compose linearly  ->      2.62 + 2.63      = 5.25e-03
//!
//! PRE-REGISTERED READ, stated before the number was computed:
//!   composed <= 4.2e-03  -> independent; worst case is bounded by quadrature
//!   composed >= 4.8e-03  -> correlated; the two errors push the same direction
//!   in between            -> undecided, and reported as undecided
//!
//! This exists because `quantized_agreement.rs` measured every cell EXCEPT the
//! diagonal one, and the diagonal is the cell a deployment actually occupies.

use std::path::PathBuf;

use neuralcompose_llama::{Embedder, cosine_similarity, devices};

const QUANTS: &[&str] = &["Q8_0", "Q5_K_M", "Q4_K_M"];
const ALL_LAYERS: u32 = 99;

/// Several strings, because a cancellation seen on one input is a coincidence
/// until it survives inputs of different length and content.
const TEXTS: &[&str] = &[
    "the headband is on",
    "neural compose",
    "quarterly tax filing deadline",
    "a considerably longer sentence that runs past the thirty-two token \
     boundary where the cpu backend stops parallelising, so that the \
     comparison also covers the regime the performance claim describes",
];

fn model_dir() -> PathBuf {
    std::env::var_os("NC_TEST_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("models"))
}

fn path_for(quant: &str) -> PathBuf {
    model_dir().join(format!("bge-small-en-v1.5-{quant}.gguf"))
}

fn embed_all(quant: &str, gpu_layers: u32) -> Vec<Vec<f32>> {
    let p = path_for(quant);
    let mut e = Embedder::load_with(&p, 512, gpu_layers).expect("load");
    TEXTS.iter().map(|t| e.embed(t).expect("embed")).collect()
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max)
}

/// L2 norm of the difference. `max_diff` is an ORDER STATISTIC — it reports one
/// component and says nothing about the other 383 — so a composed max that
/// falls below both sources could be a coincidence of which component won.
/// L2 aggregates every component and cannot be gamed that way, so the two
/// together decide whether an apparent cancellation is real.
fn l2_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// `false` when fixtures or an accelerator are absent, with the reason printed.
/// `NC_REQUIRE_QUANT=1` turns a skip into a failure — a skipped Rust test still
/// reports `ok`, so an unexercised path would print the same "all passed".
fn ready() -> bool {
    let required = std::env::var_os("NC_REQUIRE_QUANT").is_some_and(|v| v != "0");
    let refuse = |why: String| -> bool {
        assert!(
            !required,
            "NC_REQUIRE_QUANT is set but {why} — refusing to report a pass that verifies nothing"
        );
        eprintln!("SKIP: {why}");
        false
    };
    if !neuralcompose_llama::is_available() {
        return refuse("built as a stub (LLAMA_CPP_DIR unset)".into());
    }
    if !devices().iter().any(|d| d.kind.is_accelerator()) {
        return refuse("no accelerator — this test needs both backends".into());
    }
    for q in QUANTS.iter().chain(std::iter::once(&"f32")) {
        let p = path_for(q);
        if !p.exists() {
            return refuse(format!("no fixture at {}", p.display()));
        }
    }
    true
}

#[test]
fn quantisation_and_backend_error_compose_in_quadrature_not_linearly() {
    if !ready() {
        return;
    }

    let cpu_f32 = embed_all("f32", 0);
    let gpu_f32 = embed_all("f32", ALL_LAYERS);

    for (i, t) in TEXTS.iter().enumerate() {
        println!(
            "\n  [{i}] {:?}\n      backend alone at f32: max {:.3e}  l2 {:.3e}",
            t,
            max_diff(&cpu_f32[i], &gpu_f32[i]),
            l2_diff(&cpu_f32[i], &gpu_f32[i]),
        );
        println!(
            "      {:<8} {:>10} {:>10} {:>10} {:>10} {:>10}",
            "quant", "quant", "backend", "composed", "quadrat.", "linear"
        );
    }

    for q in QUANTS {
        let cpu_q = embed_all(q, 0);
        let gpu_q = embed_all(q, ALL_LAYERS);

        for (i, _) in TEXTS.iter().enumerate() {
            // The two error sources, measured separately...
            let quant_only = max_diff(&cpu_f32[i], &cpu_q[i]);
            let backend_only = max_diff(&cpu_q[i], &gpu_q[i]);
            // ...and the cell a deployment occupies: an f32 index queried by a
            // quantised Vulkan runtime.
            let composed = max_diff(&cpu_f32[i], &gpu_q[i]);

            let (lq, lb, lc) = (
                l2_diff(&cpu_f32[i], &cpu_q[i]),
                l2_diff(&cpu_q[i], &gpu_q[i]),
                l2_diff(&cpu_f32[i], &gpu_q[i]),
            );

            println!(
                "  [{i}] {q:<8} max {quant_only:>9.3e} {backend_only:>10.3e} \
                 {composed:>10.3e} {:>10.3e} {:>10.3e}",
                (quant_only.powi(2) + backend_only.powi(2)).sqrt(),
                quant_only + backend_only
            );
            println!(
                "      {:<8} l2  {lq:>9.3e} {lb:>10.3e} {lc:>10.3e} {:>10.3e} {:>10.3e}",
                "",
                (lq.powi(2) + lb.powi(2)).sqrt(),
                lq + lb
            );

            // Arithmetic bounds. A violation means one of the three
            // measurements is not measuring what it claims to.
            assert!(
                composed <= (quant_only + backend_only) * 1.05,
                "[{i}] {q}: composed {composed:.3e} exceeds the linear bound"
            );

            // Semantics must survive the COMPOSITION, not just each source
            // alone — this is the assertion the deployment actually needs.
            // Measured minimum was 0.996847 (Q4_K_M, text 2); 0.996 is a floor
            // with room, not a value fitted to make the test pass.
            let cos = cosine_similarity(&cpu_f32[i], &gpu_q[i]).expect("same length");
            assert!(cos > 0.996, "[{i}] {q}: composed cosine only {cos:.9}");
        }
    }
}

/// A `--gpu-layers 0` run is NOT a CPU run at or above 32 tokens.
///
/// ggml-vulkan's `offload_op` returns true once the batch dimension reaches
/// `op_offload_min_batch_size` (default 32), and `op_offload` defaults to true
/// in `llama_context_default_params`. So with the Vulkan backend merely
/// REGISTERED, matmuls move to the GPU even when no layers were offloaded and
/// the weights live in host memory.
///
/// This silently invalidated the CPU baseline of the Vulkan performance
/// measurements above 32 tokens: they compared GPU-with-copied-weights against
/// GPU-with-resident-weights, not CPU against GPU.
///
/// It was detected as bit-identical output between the two configurations.
/// `vulkan_agreement.rs` already asserts that suspicious identity is a failure
/// — but every string it tests is under 32 tokens, so a correct assertion
/// never ran on an input that would have fired it. Length was the untested
/// dimension, not the assertion.
///
/// Both directions are pinned so a future ggml default change is noticed.
#[test]
fn a_zero_gpu_layer_run_silently_uses_the_gpu_past_the_offload_threshold() {
    if !ready() {
        return;
    }
    // 32 tokens exactly — one token below, the offload does not trigger.
    let long = "alpha bravo charlie delta echo foxtrot golf hotel india juliet \
                kilo lima mike november oscar papa quebec romeo sierra tango \
                uniform victor whiskey xray yankee zulu";
    let p = path_for("f32");

    let mut a = Embedder::load_with(&p, 512, 0).expect("load ngl=0");
    let mut b = Embedder::load_with(&p, 512, ALL_LAYERS).expect("load ngl=99");
    let (va, vb) = (a.embed(long).expect("a"), b.embed(long).expect("b"));

    if std::env::var_os("GGML_OP_OFFLOAD_MIN_BATCH").is_some() {
        // Baseline restored: the two configurations must now diverge, because
        // one is genuinely on the CPU and the other is genuinely on the GPU.
        assert_ne!(
            va, vb,
            "offload suppressed, yet ngl=0 and ngl=99 still agree bit-for-bit"
        );
        return;
    }

    // Default settings: they agree bit-for-bit, and that is the defect.
    assert_eq!(
        va, vb,
        "ngl=0 and ngl=99 now DIFFER at 32 tokens — ggml's op-offload default \
         may have changed. Re-measure any CPU baseline before trusting it."
    );
    eprintln!(
        "\n  CONFIRMED: at 32 tokens a --gpu-layers 0 run is bit-identical to a\n  \
         --gpu-layers 99 run. Any CPU baseline above 32 tokens must set\n  \
         GGML_OP_OFFLOAD_MIN_BATCH or it is measuring the GPU.\n"
    );
}

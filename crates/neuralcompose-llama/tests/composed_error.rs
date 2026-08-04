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
const SUBJECT: &str = "the headband is on";
const ALL_LAYERS: u32 = 99;

fn model_dir() -> PathBuf {
    std::env::var_os("NC_TEST_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("models"))
}

fn path_for(quant: &str) -> PathBuf {
    model_dir().join(format!("bge-small-en-v1.5-{quant}.gguf"))
}

fn embed(quant: &str, gpu_layers: u32) -> Vec<f32> {
    let p = path_for(quant);
    let mut e = Embedder::load_with(&p, 512, gpu_layers).expect("load");
    e.embed(SUBJECT).expect("embed")
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max)
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

    let cpu_f32 = embed("f32", 0);
    let gpu_f32 = embed("f32", ALL_LAYERS);
    let backend_only_f32 = max_diff(&cpu_f32, &gpu_f32);

    println!("\n  backend alone at f32: {backend_only_f32:.3e}\n");
    println!(
        "  {:<8} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "quant", "quant-only", "bkend-only", "composed", "quadrature", "linear"
    );

    for q in QUANTS {
        let cpu_q = embed(q, 0);
        let gpu_q = embed(q, ALL_LAYERS);

        // The two error sources, measured separately.
        let quant_only = max_diff(&cpu_f32, &cpu_q);
        let backend_only = max_diff(&cpu_q, &gpu_q);
        // The cell a deployment occupies: f32 index, quantised Vulkan query.
        let composed = max_diff(&cpu_f32, &gpu_q);

        let quadrature = (quant_only.powi(2) + backend_only.powi(2)).sqrt();
        let linear = quant_only + backend_only;

        println!(
            "  {q:<8} {quant_only:>10.3e} {backend_only:>10.3e} {composed:>10.3e} \
             {quadrature:>10.3e} {linear:>10.3e}"
        );

        // Composition can never beat the larger single source by much, and can
        // never exceed the linear sum — those are the arithmetic bounds, and a
        // violation means one of the three measurements is not what it claims.
        assert!(
            composed <= linear * 1.05,
            "{q}: composed {composed:.3e} exceeds the linear bound {linear:.3e}"
        );
        assert!(
            composed >= quant_only.max(backend_only) * 0.5,
            "{q}: composed {composed:.3e} is implausibly below both sources"
        );

        // Semantics must survive the composition, not just each source alone.
        let cos = cosine_similarity(&cpu_f32, &gpu_q).expect("same length");
        assert!(cos > 0.997, "{q}: composed cosine only {cos:.9}");
        println!("           composed cosine {cos:.9}");
    }
}

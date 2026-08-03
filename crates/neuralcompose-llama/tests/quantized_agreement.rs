//! Quantised models — a code path neither promoted row had ever exercised.
//!
//! Both `linux/x86_64` rows reached `RuntimeSmokeValidated` on **f32 only**,
//! on a device reporting `bf16: 0`. Q4_K/Q5_K/Q8_0 are different kernels in
//! both the CPU and Vulkan backends, and any production model is quantised, so
//! this was a larger untested surface than model variety.
//!
//! THE FINDING THAT MATTERS
//!
//! Quantisation degrades **cross-backend agreement** far more than it degrades
//! the embedding itself. Measured 2026-08-03 on a Radeon 890M:
//!
//! | quant | cos vs f32 (cpu) | cos cpu~vulkan |
//! |---|---|---|
//! | f32 | 1.000000000 | 0.999999894 |
//! | Q8_0 | 0.999844 | 0.999897 |
//! | Q5_K_M | 0.998434 | 0.999884 |
//! | Q4_K_M | 0.997892 | 0.999868 |
//!
//! Max per-component divergence between backends goes from 7.3e-5 at f32 to
//! ~2.5e-3 at every quantisation — roughly **35x worse**. For Q8_0 that
//! cross-backend difference (2.63e-3) is the same size as the quantisation
//! error itself (2.62e-3): choosing a different backend perturbs the vector as
//! much as quantising did.
//!
//! The practical consequence is that CPU and Vulkan are NOT interchangeable for
//! a quantised embedding index. A vector written by one backend and queried by
//! the other differs by more than quantisation noise, and an "identical across
//! runtimes" bar stated at six decimal places — cos 1.000000 — is met by f32
//! (0.999999894 rounds to 1.000000) and **failed by every quantisation here**.
//!
//! WHAT SURVIVES
//!
//! Determinism within a backend is bit-identical, so `RuntimeSmokeValidated`'s
//! "deterministic" holds for quantised models. Semantic ordering survives
//! intact: the related/unrelated cosine margin is 0.377-0.382 across every
//! quantisation and both backends, against f32's 0.380.
//!
//! FIXTURES ARE NOT IN THE REPOSITORY. Produce them from the f32 model with
//! llama.cpp's own quantiser, which is what these numbers were measured on:
//!
//! ```sh
//! llama-quantize bge-small-en-v1.5-f32.gguf bge-small-en-v1.5-Q4_K_M.gguf Q4_K_M
//! ```

use std::path::{Path, PathBuf};

use neuralcompose_llama::{Embedder, cosine_similarity, devices};

/// Quantisations under test, with the size each produced from the 133.6 MB f32.
const QUANTS: &[(&str, f64)] = &[("Q8_0", 36.8), ("Q5_K_M", 30.5), ("Q4_K_M", 29.2)];

fn model_dir() -> PathBuf {
    std::env::var_os("NC_TEST_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("models"))
}

fn path_for(quant: &str) -> PathBuf {
    model_dir().join(format!("bge-small-en-v1.5-{quant}.gguf"))
}

/// `false` when the quantised fixtures are absent, with the reason printed.
///
/// `NC_REQUIRE_QUANT=1` turns every skip into a failure, for the reason
/// `NC_REQUIRE_BACKEND` exists: a skipped Rust test still reports `ok`, so an
/// unexercised quantised path would print the same "all passed" as a real run.
fn quant_fixtures_present() -> bool {
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
    for (q, _) in QUANTS {
        let p = path_for(q);
        if !p.exists() {
            return refuse(format!("no quantised fixture at {}", p.display()));
        }
    }
    if !path_for("f32").exists() {
        return refuse("no f32 baseline to compare against".into());
    }
    true
}

fn embed(path: &Path, gpu_layers: u32, text: &str) -> Vec<f32> {
    let mut e = Embedder::load_with(path, 512, gpu_layers).expect("load");
    e.embed(text).expect("embed")
}

const SUBJECT: &str = "the headband is on";
const RELATED: &str = "the EEG headset is being worn";
const UNRELATED: &str = "quarterly tax filing deadline";

#[test]
fn every_quantisation_loads_and_produces_the_same_dimensionality() {
    if !quant_fixtures_present() {
        return;
    }
    for (q, _) in QUANTS {
        let v = embed(&path_for(q), 0, SUBJECT);
        assert_eq!(v.len(), 384, "{q} changed dimensionality");
        assert!(
            v.iter().all(|x| x.is_finite()),
            "{q} produced a non-finite component"
        );
    }
}

#[test]
fn quantised_output_is_deterministic_within_a_backend() {
    // `RuntimeSmokeValidated` means "a deterministic fixture model executed
    // successfully". If quantised kernels were not deterministic the rung
    // could not be claimed for a quantised model, so this is the load-bearing
    // property rather than a nicety.
    if !quant_fixtures_present() {
        return;
    }
    for (q, _) in QUANTS {
        let a = embed(&path_for(q), 0, SUBJECT);
        let b = embed(&path_for(q), 0, SUBJECT);
        assert_eq!(a, b, "{q} on CPU is not bit-identical across runs");
    }
}

#[test]
fn quantisation_preserves_semantic_ordering() {
    // A vector that is numerically close to f32 but semantically scrambled
    // would pass a cosine-to-f32 check and be useless. Measured margins were
    // 0.3789-0.3819 across every quantisation and both backends, against
    // f32's 0.3800 — asserted with room, not pinned to those exact values.
    if !quant_fixtures_present() {
        return;
    }
    for (q, _) in QUANTS {
        let p = path_for(q);
        let a = embed(&p, 0, SUBJECT);
        let b = embed(&p, 0, RELATED);
        let c = embed(&p, 0, UNRELATED);
        let related = cosine_similarity(&a, &b).expect("same length");
        let unrelated = cosine_similarity(&a, &c).expect("same length");
        assert!(
            related > unrelated + 0.2,
            "{q}: related {related:.4} vs unrelated {unrelated:.4} — ordering degraded"
        );
    }
}

#[test]
fn quantisation_error_grows_as_precision_falls() {
    // Ordering, not absolute values: Q8_0 must be closer to f32 than Q4_K_M.
    // A build in which they were equal would mean the quantisation type was
    // being ignored, which is a defect no per-quant threshold would catch.
    if !quant_fixtures_present() {
        return;
    }
    let reference = embed(&path_for("f32"), 0, SUBJECT);
    let mut previous = 1.0f32;
    for (q, _) in QUANTS {
        let v = embed(&path_for(q), 0, SUBJECT);
        let c = cosine_similarity(&reference, &v).expect("same length");
        assert!(
            c > 0.99,
            "{q}: cosine {c:.6} against f32 is too low to be usable"
        );
        assert!(
            c < 1.0,
            "{q}: cosine {c:.9} against f32 is suspiciously exact — is the quant applied?"
        );
        assert!(
            c <= previous,
            "{q}: cosine {c:.6} exceeds the higher-precision quantisation above it"
        );
        previous = c;
    }
}

#[test]
fn cross_backend_divergence_is_much_worse_once_quantised() {
    // PINS THE DEGRADATION AS A MEASURED FACT, so a silently tightened
    // tolerance or a changed kernel is noticed. The f32 backends agree to
    // 7.3e-5; every quantisation diverges by ~2.5e-3, about 35x worse. For
    // Q8_0 that is the same size as the quantisation error itself.
    if !quant_fixtures_present() {
        return;
    }
    if !devices().iter().any(|d| d.kind.is_accelerator()) {
        eprintln!("SKIP: no accelerator — this test compares CPU against one");
        return;
    }

    let f32_cpu = embed(&path_for("f32"), 0, SUBJECT);
    let f32_gpu = embed(&path_for("f32"), 99, SUBJECT);
    let f32_div = f32_cpu
        .iter()
        .zip(f32_gpu.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(f32_div < 2e-4, "f32 cross-backend divergence {f32_div:.2e}");

    for (q, _) in QUANTS {
        let p = path_for(q);
        let cpu = embed(&p, 0, SUBJECT);
        let gpu = embed(&p, 99, SUBJECT);
        let div = cpu
            .iter()
            .zip(gpu.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);

        // Worse than f32 by a wide margin — the finding this test exists for.
        assert!(
            div > f32_div * 5.0,
            "{q}: cross-backend divergence {div:.2e} is not materially worse than \
             f32's {f32_div:.2e} — has a kernel changed?"
        );
        // But still small enough to be usable, and semantically intact.
        assert!(
            div < 1e-2,
            "{q}: cross-backend divergence {div:.2e} too large"
        );
        let c = cosine_similarity(&cpu, &gpu).expect("same length");
        assert!(c > 0.999, "{q}: cross-backend cosine only {c:.9}");

        // And explicitly NOT the "identical across runtimes" bar that f32
        // meets. Stated as an assertion so nobody later claims it does.
        assert!(
            c < 0.999999,
            "{q}: cross-backend cosine {c:.9} would round to 1.000000 — if this \
             now holds, the claim in this file's header must be revised"
        );
    }
}

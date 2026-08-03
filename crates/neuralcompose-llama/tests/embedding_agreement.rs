//! Agreement with llama.cpp's own `llama-embedding` binary.
//!
//! The hand-written FFI in this crate is only trustworthy if it produces the
//! same numbers as the reference implementation on the same input. This is the
//! same discipline `band_power` uses against the Python original, and for the
//! same reason: an earlier draft of THAT test carried plausible-looking
//! constants written by hand, and they were wrong by a third. Every value below
//! was produced by running the reference binary and pasting its output.
//!
//! HOW TO REPRODUCE THE REFERENCE VALUES
//!
//! ```sh
//! LD_LIBRARY_PATH=build-cpu/bin ./build-cpu/bin/llama-embedding \
//!   -m ~/models/bge-small-en-v1.5-f32.gguf -p "neural compose" \
//!   --embd-output-format json
//! ```
//!
//! SKIPS RATHER THAN FAILS when the backend or the model is absent, because
//! neither is in the repository and CI has no GPU, no C++ toolchain guarantee,
//! and no 133 MB model. A skipped test prints why — a silent pass would make an
//! unbuilt backend look verified, which is the failure mode that matters here.

use std::path::PathBuf;

use neuralcompose_llama::{BACKEND_ID, Embedder, RUNTIME_ABI, cosine_similarity};

/// Where the fixture model lives. Overridable so the path is not baked in.
fn model_path() -> PathBuf {
    std::env::var_os("NC_TEST_GGUF")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join("models/bge-small-en-v1.5-f32.gguf")
        })
}

/// `None` when the test cannot run, with the reason already printed.
///
/// A skipped test still reports `ok`, so a stub build produces the SAME
/// "6 passed" line as a real one — which would make an unbuilt backend look
/// verified. That is the exact failure mode this file exists to prevent, so
/// `NC_REQUIRE_BACKEND=1` converts every skip into a hard failure. Any run
/// that produces acceptance evidence must set it; without it these tests are
/// a convenience, not a claim.
fn embedder() -> Option<Embedder> {
    let required = std::env::var_os("NC_REQUIRE_BACKEND").is_some_and(|v| v != "0");
    if !neuralcompose_llama::is_available() {
        assert!(
            !required,
            "NC_REQUIRE_BACKEND is set but this is a stub build (LLAMA_CPP_DIR unset) — \
             refusing to report a pass that verifies nothing"
        );
        eprintln!("SKIP: built as a stub (LLAMA_CPP_DIR unset)");
        return None;
    }
    let path = model_path();
    if !path.exists() {
        assert!(
            !required,
            "NC_REQUIRE_BACKEND is set but no fixture model at {} — \
             refusing to report a pass that verifies nothing",
            path.display()
        );
        eprintln!("SKIP: no fixture model at {}", path.display());
        return None;
    }
    match Embedder::load(&path, 512) {
        Ok(e) => Some(e),
        Err(e) => panic!("backend is available and model exists, but load failed: {e}"),
    }
}

/// Reference values from `llama-embedding --embd-output-format json`, sampled
/// across the vector so a systematic offset or a truncated copy is visible.
/// bge-small-en-v1.5 is 384-dimensional, L2-normalised.
const REFERENCE: &[(&str, &[(usize, f32)])] = &[
    (
        "neural compose",
        &[
            (0, 0.0116078),
            (1, -0.064148),
            (2, -0.0275825),
            (17, 0.0151465),
            (100, -0.0730316),
            (200, 0.0033153),
            (300, 0.0036848),
            (383, -0.0835864),
        ],
    ),
    (
        "the headband is on",
        &[
            (0, -0.0113142),
            (1, -0.0021324),
            (2, 0.0365841),
            (17, 0.0418104),
            (100, -0.0227055),
            (200, 0.0578048),
            (300, 0.0196327),
            (383, 0.0600167),
        ],
    ),
];

#[test]
fn it_agrees_with_the_reference_llama_embedding_binary() {
    let Some(mut e) = embedder() else { return };
    assert_eq!(e.dimensions(), 384, "bge-small-en-v1.5 is 384-dimensional");

    for (text, expected) in REFERENCE {
        let got = e.embed(text).expect("embed");
        assert_eq!(got.len(), 384);
        for (i, want) in *expected {
            let diff = (got[*i] - want).abs();
            // The reference JSON prints ~7 significant figures, so agreement is
            // asserted at that resolution and no tighter. A looser bound would
            // let a genuinely different pooling strategy pass.
            assert!(
                diff < 2e-6,
                "{text:?} dim {i}: got {} want {want} (diff {diff:.3e})",
                got[*i]
            );
        }
    }
}

#[test]
fn the_same_input_produces_byte_identical_output() {
    // Determinism is what ADR-002's `RuntimeSmokeValidated` rung means by "a
    // deterministic fixture model executed successfully". If two runs of the
    // same input differed, no evidence produced by this backend would be
    // reproducible and the rung could not be claimed.
    let Some(mut e) = embedder() else { return };
    let a = e.embed("neural compose").expect("first");
    let b = e.embed("neural compose").expect("second");
    assert_eq!(a, b, "identical input must give identical output");
}

#[test]
fn the_vector_is_l2_normalised() {
    let Some(mut e) = embedder() else { return };
    let v = e.embed("neural compose").expect("embed");
    let norm: f64 = v
        .iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt();
    assert!((norm - 1.0).abs() < 1e-5, "expected unit norm, got {norm}");

    // And the un-normalised path must NOT be unit norm, or `normalize` would be
    // a no-op and this test would be asserting nothing.
    let raw = e.embed_with("neural compose", false).expect("embed raw");
    let raw_norm: f64 = raw
        .iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt();
    assert!(
        (raw_norm - 1.0).abs() > 1e-3,
        "un-normalised output happened to be unit norm ({raw_norm}) — \
         the normalize flag is not doing anything"
    );
}

#[test]
fn related_text_scores_higher_than_unrelated_text() {
    // Anchors measured from the reference binary: 0.818246 related,
    // 0.438293 unrelated. Asserted as an ordering with margin rather than as
    // exact values, because this is testing that the embedding carries meaning
    // at all — a backend that returned a constant vector would pass every
    // numeric test above and fail this one.
    let Some(mut e) = embedder() else { return };
    let on = e.embed("the headband is on").expect("a");
    let worn = e.embed("the EEG headset is being worn").expect("b");
    let tax = e.embed("quarterly tax filing deadline").expect("c");

    let related = cosine_similarity(&on, &worn).expect("same length");
    let unrelated = cosine_similarity(&on, &tax).expect("same length");
    assert!(
        related > unrelated + 0.2,
        "related {related:.4} should clearly exceed unrelated {unrelated:.4}"
    );
    assert!(
        (related - 0.818_246).abs() < 0.01,
        "related was {related:.6}"
    );
    assert!(
        (unrelated - 0.438_293).abs() < 0.01,
        "unrelated was {unrelated:.6}"
    );
}

#[test]
fn empty_input_is_refused_rather_than_embedded() {
    let Some(mut e) = embedder() else { return };
    assert!(e.embed("").is_err(), "empty text must not produce a vector");
}

#[test]
fn the_identifiers_this_evidence_would_be_filed_under_are_correct() {
    // Evidence from this crate is only attributable to a support-matrix row if
    // these match the row's columns exactly.
    assert_eq!(BACKEND_ID, "llama-cpp-cpu");
    assert_eq!(RUNTIME_ABI, "nc-gguf-v1");
}

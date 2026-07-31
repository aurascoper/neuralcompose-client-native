//! Deterministic fixture generator. Writes, to the directory given as arg 1
//! (default: contracts/fixtures/):
//!
//!   * `eeg-frame-batch-8.json` — the first 8-sample batch of the Gate 4
//!     stub's sine formula (sampleIndex 0);
//!   * `m7b-corpus-v1.json` — the canonical serialization of the M7-B
//!     generation-eval corpus **as Rust parsed it**.
//!
//! CI runs this into a temp dir and diffs against the frozen files
//! (scripts/check-fixtures.sh), proving the Rust core owns fixture generation.
//!
//! The corpus case is a round trip rather than a construction: the bytes are
//! `include_bytes!`d, parsed, and re-serialized. A diff therefore means the
//! committed artifact and the value the crate compiled in have come apart —
//! an unparsed field, a reordered prompt, a hand edit that serde would not
//! reproduce. That is the whole point of a corpus whose identity is derived
//! from content: the reviewable file and the compiled value must be the same
//! thing, and this is what proves it rather than assuming it.

use neuralcompose_mobile_core::generation_eval::frozen_corpus_v1;
use neuralcompose_mobile_core::EEGSample;

/// `f64::sin` delegates to the platform libm; macOS and glibc can differ in
/// the last ulp, which changes the shortest-round-trip JSON bytes. Rounding
/// to 1e-12 absorbs that (the golden test tolerance is 1e-12) and makes the
/// fixture byte-identical across platforms.
fn stable(v: f64) -> f64 {
    (v * 1e12).round() / 1e12
}

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("{}/../../contracts/fixtures", env!("CARGO_MANIFEST_DIR")));

    let tau = std::f64::consts::TAU;
    let batch: Vec<EEGSample> = (0..8)
        .map(|n| {
            let t = n as f64 / 256.0;
            EEGSample {
                timestamp: t,
                channels: [
                    stable(45.0 * (tau * 8.0 * t).sin()),
                    stable(32.0 * (tau * 10.0 * t + 0.5).sin()),
                    stable(36.0 * (tau * 12.0 * t + 1.0).sin()),
                    stable(42.0 * (tau * 6.0 * t + 1.5).sin()),
                ],
            }
        })
        .collect();

    let json = serde_json::to_string_pretty(&batch).expect("serialize");
    let path = format!("{out_dir}/eeg-frame-batch-8.json");
    std::fs::write(&path, json + "\n").expect("write fixture");
    println!("wrote {path}");

    let corpus = serde_json::to_string_pretty(&frozen_corpus_v1()).expect("serialize corpus");
    let corpus_path = format!("{out_dir}/m7b-corpus-v1.json");
    std::fs::write(&corpus_path, corpus + "\n").expect("write corpus fixture");
    println!("wrote {corpus_path}");
}

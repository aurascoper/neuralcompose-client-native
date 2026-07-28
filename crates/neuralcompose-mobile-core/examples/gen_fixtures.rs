//! Deterministic fixture generator. Writes `eeg-frame-batch-8.json` — the
//! first 8-sample batch of the Gate 4 stub's sine formula (sampleIndex 0) —
//! to the directory given as arg 1 (default: contracts/fixtures/).
//! CI runs this into a temp dir and diffs against the frozen file
//! (scripts/check-fixtures.sh), proving the Rust core owns fixture generation.

use neuralcompose_mobile_core::EEGSample;

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
                    45.0 * (tau * 8.0 * t).sin(),
                    32.0 * (tau * 10.0 * t + 0.5).sin(),
                    36.0 * (tau * 12.0 * t + 1.0).sin(),
                    42.0 * (tau * 6.0 * t + 1.5).sin(),
                ],
            }
        })
        .collect();

    let json = serde_json::to_string_pretty(&batch).expect("serialize");
    let path = format!("{out_dir}/eeg-frame-batch-8.json");
    std::fs::write(&path, json + "\n").expect("write fixture");
    println!("wrote {path}");
}

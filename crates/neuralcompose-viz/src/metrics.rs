use serde::{Deserialize, Serialize};

pub fn percentile(values: &[f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut copy = values.to_vec();
    copy.sort_by(f64::total_cmp);
    Some(
        copy[((q * copy.len() as f64).ceil() as usize)
            .saturating_sub(1)
            .min(copy.len() - 1)],
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub schema: String,
    pub case: String,
    pub repeat: u32,
    pub commit: String,
    pub executable_sha256: String,
    pub clean_tree: bool,
    pub quotable: bool,
    pub build_configuration: String,
    pub gpu: String,
    pub framebuffer: [u32; 2],
    pub duration_s: f64,
    pub rendered_frames: usize,
    pub accepted_samples: u64,
    pub fps: f64,
    pub frame_p95_ms: Option<f64>,
    pub age_p95_ms: Option<f64>,
    pub frame_intervals_ms: Vec<f64>,
    pub sample_to_submit_ms: Vec<f64>,
    pub live_fraction: f64,
    pub within_budget_fraction: f64,
    pub clipped_fraction: f64,
    pub skipped_notifications: u64,
}
impl Run {
    pub fn meets_targets(&self) -> bool {
        self.quotable
            && self.frame_p95_ms.is_some_and(|v| v <= 20.0)
            && self.age_p95_ms.is_some_and(|v| v <= 100.0)
    }
}
pub fn verdict(runs: &[Run]) -> &'static str {
    if runs.len() != 3
        || runs.iter().any(|r| !r.quotable || !r.clean_tree)
        || !(1..=3).all(|repeat| runs.iter().filter(|r| r.repeat == repeat).count() == 1)
        || runs.iter().any(|r| {
            r.case != runs[0].case
                || r.commit != runs[0].commit
                || r.executable_sha256 != runs[0].executable_sha256
        })
    {
        return "unresolved";
    }
    match runs.iter().filter(|r| r.meets_targets()).count() {
        3 => "accepted",
        0 => "targets not met",
        _ => "unresolved",
    }
}

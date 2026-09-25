use neuralcompose_viz::metrics::{percentile, Run};

pub struct Recorder {
    pub warmup: f64,
    pub seconds: f64,
    pub framebuffer: [u32; 2],
    pub frames: Vec<f64>,
    pub ages: Vec<f64>,
    times: Vec<f64>,
    turns: Vec<f64>,
    first: Option<f64>,
    last: Option<f64>,
    first_received: Option<u64>,
    received: u64,
    frame_count: usize,
    live: usize,
    clipped: usize,
    examined: usize,
    skipped: u64,
}
impl Recorder {
    pub fn new(warmup: f64, seconds: f64) -> Self {
        Self {
            warmup,
            seconds,
            framebuffer: [0; 2],
            frames: Vec::new(),
            ages: Vec::new(),
            times: Vec::new(),
            turns: Vec::new(),
            first: None,
            last: None,
            first_received: None,
            received: 0,
            frame_count: 0,
            live: 0,
            clipped: 0,
            examined: 0,
            skipped: 0,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn submit(
        &mut self,
        now: f64,
        age: Option<f64>,
        received: u64,
        live: bool,
        clipped: usize,
        examined: usize,
        skipped: u64,
    ) {
        if !self.in_window(now) {
            return;
        }
        self.first.get_or_insert(now);
        self.first_received.get_or_insert(received);
        if let Some(last) = self.last {
            self.frames.push((now - last) * 1000.0);
        }
        self.last = Some(now);
        self.times.push(now);
        self.received = received;
        self.frame_count += 1;
        self.live += usize::from(live);
        if let Some(age) = age {
            self.ages.push(age);
        }
        self.clipped += clipped;
        self.examined += examined;
        self.skipped = skipped;
        // Interactive sessions keep bounded measurement storage too.
        if self.seconds == 0.0 && self.frames.len() > 36000 {
            self.frames.drain(..18000);
            self.ages.drain(..self.ages.len().saturating_sub(18000));
            self.times.drain(..18000);
            self.turns.retain(|t| *t >= self.times[0]);
        }
    }
    fn in_window(&self, now: f64) -> bool {
        now >= self.warmup && (self.seconds == 0.0 || now <= self.warmup + self.seconds)
    }
    pub fn turn(&mut self, now: f64) {
        if self.in_window(now) {
            self.turns.push(now);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn finish(
        &self,
        case: String,
        repeat: u32,
        gpu: String,
        sha: String,
        clean: bool,
        recorded: bool,
        ended: &str,
    ) -> Run {
        let duration = self.last.unwrap_or(0.0) - self.first.unwrap_or(0.0);
        Run {
            schema: "neuralcompose.phase-space.run.v2".into(),
            case,
            repeat,
            commit: env!("NC_VIZ_COMMIT").into(),
            executable_sha256: sha,
            clean_tree: clean,
            quotable: recorded
                && ended == "completed"
                && clean
                && duration >= 59.9
                && self.warmup == 5.0
                && self.seconds == 60.0,
            build_configuration: format!(
                "{}; {}; Cargo.lock; requested 1280x800 logical points; measured scene extent in framebuffer field; vsync; 60 Hz redraw request",
                env!("NC_VIZ_PROFILE"),
                env!("NC_VIZ_RUSTC")
            ),
            gpu,
            framebuffer: self.framebuffer,
            duration_s: duration,
            rendered_frames: self.frame_count,
            accepted_samples: self
                .received
                .saturating_sub(self.first_received.unwrap_or(0)),
            fps: if duration > 0.0 {
                self.frames.len() as f64 / duration
            } else {
                0.0
            },
            frame_p95_ms: percentile(&self.frames, 0.95),
            age_p95_ms: percentile(&self.ages, 0.95),
            frame_intervals_ms: self.frames.clone(),
            sample_to_submit_ms: self.ages.clone(),
            live_fraction: self.live as f64 / self.frame_count.max(1) as f64,
            within_budget_fraction: self.frames.iter().filter(|x| **x <= 20.0).count() as f64
                / self.frames.len().max(1) as f64,
            clipped_fraction: self.clipped as f64 / self.examined.max(1) as f64,
            skipped_notifications: self.skipped,
            frame_submit_s: self.times.clone(),
            turn_received_s: self.turns.clone(),
            ended: ended.into(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::Recorder;

    #[test]
    fn turn_and_frame_times_stay_inside_the_measurement_window() {
        let mut r = Recorder::new(1.0, 2.0);
        r.turn(0.5); // warmup
        for now in [0.9, 1.0, 1.5, 3.0, 3.1] {
            r.submit(now, None, 0, true, 0, 0, 0);
        }
        r.turn(2.0);
        r.turn(3.5); // after the window
        let run = r.finish(
            "c".into(),
            1,
            String::new(),
            String::new(),
            true,
            false,
            "completed",
        );
        assert_eq!(run.frame_submit_s, vec![1.0, 1.5, 3.0]);
        assert_eq!(run.turn_received_s, vec![2.0]);
        assert_eq!(run.frame_intervals_ms.len(), run.frame_submit_s.len() - 1);
        assert_eq!(run.schema, "neuralcompose.phase-space.run.v2");
    }

    #[test]
    fn only_a_completed_run_is_quotable() {
        let mut r = Recorder::new(5.0, 60.0);
        for now in [5.0, 35.0, 65.0] {
            r.submit(now, Some(1.0), 0, true, 0, 0, 0);
        }
        let run = |ended| {
            r.finish(
                "c".into(),
                1,
                String::new(),
                String::new(),
                true,
                true,
                ended,
            )
        };
        assert!(run("completed").quotable);
        let stopped = run("stopped drawing");
        assert!(!stopped.quotable);
        assert_eq!(stopped.ended, "stopped drawing");
    }
}

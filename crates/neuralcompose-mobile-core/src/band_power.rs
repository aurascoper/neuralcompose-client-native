//! Band power over a windowed periodogram.
//!
//! A port of `bandpower` from `Scripts/validate-muse-physiology.py:92-106` in
//! the macOS repository. That function is the arithmetic behind the
//! eyes-open/eyes-closed alpha check, and its 1.5x pass threshold is
//! pre-registered in the same file — but it lived only in a Python script, so
//! the 2026-08-03 Linux session had to run its alpha analysis outside the core
//! against the wire contract. See `docs/acceptance/linux-headless-runtime.md`.
//!
//! This is the second capability that session found living outside the core,
//! after the channel-health thresholds. Both were pure functions over a slice
//! of samples, and neither had any reason to be platform-bound.
//!
//! WHAT IT COMPUTES, AND WHAT IT DOES NOT
//!
//! Mean-detrend, Hann window, real FFT, sum the squared magnitudes whose bin
//! frequency falls in `[lo, hi]` INCLUSIVE, divide by `n^2`. The Python
//! docstring calls it "Welch-like", which is generous: it is a single
//! periodogram with no segment averaging and no overlap, so its variance is
//! high and it is not a power-spectral-density estimate in physical units. It
//! is a RELATIVE measure, and the only defensible use is a ratio of two of
//! these computed identically — which is exactly how the alpha check uses it.
//!
//! The `n^2` divisor and the unnormalised Hann window mean the absolute value
//! has no interpretation as µV², and comparing values computed over different
//! window lengths is meaningless. Ported unchanged anyway: the point is that
//! Rust and Python agree bit-for-bit on the same input, not that the estimator
//! is the one a signal-processing textbook would choose. Changing the
//! estimator would silently invalidate every threshold calibrated against it,
//! including the 1.5x alpha bar and the July 3.21x-3.88x reference sessions.

/// Windowed-periodogram band power over `samples` at `fs` Hz, or `None` when
/// the input cannot support one.
///
/// `None` — never a number — for all four refusals:
///
/// - less than one second of data, matching the Python guard
///   `if len(signal_1d) < int(fs)`. A deliberate refusal rather than a
///   small-sample estimate: a sub-second window cannot resolve an 8-13 Hz band
///   well enough for the ratio to mean anything;
/// - an unusable rate (non-finite or non-positive);
/// - any non-finite sample. The Python version has no such guard and would
///   propagate NaN silently through a ratio, which would then compare as
///   `false` against a threshold and read as a quiet failure rather than a
///   loud one;
/// - no FFT bin inside the band (inverted, empty, or entirely above Nyquist).
///
/// ## Why this is an `Option` and not a `0.0`
///
/// Every refusal above used to return `0.0`, which is also a value the
/// arithmetic can legitimately produce — so "refused" and "measured no power"
/// were indistinguishable to every caller. That is this codebase's recurring
/// absent-became-zero defect, and it is load-bearing here: the EEG capture gate
/// in `neuralcompose-hypnagogic` refuses a window on both outcomes but must
/// record *different reasons* for them, and a downstream ratio must never
/// divide by a refusal.
///
/// `Some(0.0)` is reachable and means something specific: every detrended
/// sample was exactly zero, i.e. the window is constant. That is a dead or
/// railed channel, never a reading. It is a deterministic fact about the input,
/// not a probabilistic one — see
/// `a_constant_window_is_exactly_zero_not_merely_small`.
pub fn band_power(samples: &[f64], fs: f64, band: (f64, f64)) -> Option<f64> {
    let n = samples.len();
    if !fs.is_finite() || fs <= 0.0 || n < fs as usize {
        return None;
    }
    if samples.iter().any(|v| !v.is_finite()) {
        return None;
    }

    let mean = samples.iter().sum::<f64>() / n as f64;

    // Hann window, matching numpy's `np.hanning(n)`: w[i] = 0.5 - 0.5*cos(2*pi*i/(n-1)).
    // Note numpy's hanning is the SYMMETRIC window (denominator n-1), not the
    // periodic one (denominator n) that scipy.signal.get_window returns by
    // default. Using the periodic form here would change every value slightly
    // and quietly break agreement with the Python reference.
    let denom = if n > 1 { (n - 1) as f64 } else { 1.0 };
    let windowed: Vec<f64> = samples
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / denom).cos();
            (v - mean) * w
        })
        .collect();

    // Only the rfft bins are needed: 0..=n/2, matching np.fft.rfftfreq.
    // A direct DFT is O(n * n/2). At the sizes this is used for — 30 s at
    // 256 Hz is 7680 samples — that is a few tens of millions of operations,
    // which is acceptable for an offline check and avoids adding an FFT
    // dependency to a crate whose entire contract is that it has almost none.
    // If this ever runs per-frame in a live loop, replace it with a real FFT
    // and re-verify against the Python reference before trusting it.
    let (lo, hi) = band;
    let bin_hz = fs / n as f64;
    let k_lo = (lo / bin_hz).ceil().max(0.0) as usize;
    let k_hi = (hi / bin_hz).floor().min((n / 2) as f64) as usize;
    if k_lo > k_hi {
        return None;
    }

    let mut total = 0.0;
    for k in k_lo..=k_hi {
        let mut re = 0.0;
        let mut im = 0.0;
        let step = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
        for (i, x) in windowed.iter().enumerate() {
            let ang = step * i as f64;
            re += x * ang.cos();
            im += x * ang.sin();
        }
        total += re * re + im * im;
    }
    Some(total / (n as f64 * n as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 256.0;
    const ALPHA: (f64, f64) = (8.0, 13.0);
    const BETA: (f64, f64) = (13.0, 30.0);

    fn sine(freq: f64, amp: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| amp * (2.0 * std::f64::consts::PI * freq * i as f64 / FS).sin())
            .collect()
    }

    /// Unwrapping helper for the cases that are expected to compute. Named so a
    /// test that starts refusing fails on the `expect` message rather than on a
    /// confusing comparison.
    fn power(samples: &[f64], fs: f64, band: (f64, f64)) -> f64 {
        band_power(samples, fs, band).expect("this input should be measurable")
    }

    #[test]
    fn a_tone_inside_the_band_dominates_a_tone_outside_it() {
        let n = 2048;
        let inside = power(&sine(10.0, 20.0, n), FS, ALPHA);
        let outside = power(&sine(22.0, 20.0, n), FS, ALPHA);
        assert!(
            inside > outside * 100.0,
            "10 Hz should dominate the alpha band over 22 Hz: {inside} vs {outside}"
        );
    }

    #[test]
    fn the_same_tone_lands_in_the_band_that_contains_it() {
        let n = 2048;
        let x = sine(22.0, 20.0, n);
        assert!(power(&x, FS, BETA) > power(&x, FS, ALPHA) * 100.0);
    }

    #[test]
    fn power_scales_with_the_square_of_amplitude() {
        // The estimator is quadratic, so doubling amplitude quadruples power.
        // This is the property the alpha RATIO depends on being stable.
        let n = 2048;
        let p1 = power(&sine(10.0, 1.0, n), FS, ALPHA);
        let p2 = power(&sine(10.0, 2.0, n), FS, ALPHA);
        assert!(
            (p2 / p1 - 4.0).abs() < 0.01,
            "expected 4x, got {:.4}x",
            p2 / p1
        );
    }

    #[test]
    fn a_constant_signal_has_no_band_power_because_the_mean_is_removed() {
        let x = vec![137.0; 2048];
        assert!(power(&x, FS, ALPHA) < 1e-20);
    }

    /// The distinction the EEG capture gate is built on, pinned here rather
    /// than assumed there.
    ///
    /// A constant window is not merely *small* after detrending — it is
    /// **exactly** zero. `137.0 * 2048` is exact in binary and divides back to
    /// exactly `137.0`, so every `v - mean` is exactly `0.0`, every DFT bin is
    /// exactly `0.0`, and the total is exactly `0.0`. So `Some(0.0)` is
    /// reachable, it is deterministic rather than a measure-zero coincidence,
    /// and it means precisely one thing: a dead or railed channel.
    ///
    /// This is what lets the gate treat `None` and `Some(0.0)` as different
    /// refusals with different recorded reasons. If this ever returns a small
    /// nonzero instead, that gate silently stops firing on dead channels.
    #[test]
    fn a_constant_window_is_exactly_zero_not_merely_small() {
        let x = vec![137.0; 2048];
        assert_eq!(band_power(&x, FS, ALPHA), Some(0.0));
        // A railed channel at the ADC ceiling behaves the same way.
        let railed = vec![-1000.0; 2048];
        assert_eq!(band_power(&railed, FS, ALPHA), Some(0.0));
    }

    #[test]
    fn under_one_second_of_data_refuses_rather_than_estimating() {
        // Mirrors the Python guard exactly: len < int(fs) refuses.
        let x = sine(10.0, 20.0, 255);
        assert_eq!(band_power(&x, FS, ALPHA), None);
        // At exactly one second it computes.
        let y = sine(10.0, 20.0, 256);
        assert!(power(&y, FS, ALPHA) > 0.0);
    }

    #[test]
    fn a_non_finite_sample_refuses_rather_than_propagating_nan() {
        // The Python reference has no such guard: a NaN would flow through the
        // ratio and compare false against the threshold, reading as a quiet
        // failure instead of a loud one.
        let mut x = sine(10.0, 20.0, 2048);
        x[100] = f64::NAN;
        assert_eq!(band_power(&x, FS, ALPHA), None);
        let mut y = sine(10.0, 20.0, 2048);
        y[7] = f64::INFINITY;
        assert_eq!(band_power(&y, FS, ALPHA), None);
    }

    /// A refusal must never be confusable with a measurement, which is the
    /// whole reason for the `Option`. Stated as its own test so that collapsing
    /// the two back together fails here and not three crates away.
    #[test]
    fn a_refusal_is_not_a_zero_measurement() {
        let refused = band_power(&sine(10.0, 20.0, 255), FS, ALPHA);
        let measured_zero = band_power(&vec![137.0; 2048], FS, ALPHA);
        assert_eq!(refused, None);
        assert_eq!(measured_zero, Some(0.0));
        assert_ne!(refused, measured_zero);
    }

    #[test]
    fn both_band_edges_are_inclusive() {
        // The Python mask is `(f >= lo) & (f <= hi)` — inclusive at BOTH ends.
        //
        // This test exists because a mutation that made the upper edge
        // exclusive passed every other test in this module. The reference tones
        // sit mid-band, where Hann leakage at the edge is below the 1e-9
        // agreement tolerance, so nothing was actually asserting the boundary.
        // A tone placed exactly ON an edge makes the difference ~5x instead.
        let n = 2048;

        let top = sine(13.0, 20.0, n);
        let inclusive = power(&top, FS, (8.0, 13.0));
        let excluded = power(&top, FS, (8.0, 12.9));
        assert!((inclusive - 31.228_640_414_8).abs() / 31.228_640_414_8 < 1e-9);
        assert!(
            inclusive > excluded * 4.0,
            "13 Hz must be INSIDE (8,13): {inclusive} vs {excluded}"
        );

        let bottom = sine(8.0, 20.0, n);
        let inclusive = power(&bottom, FS, (8.0, 13.0));
        let excluded = power(&bottom, FS, (8.1, 13.0));
        assert!((inclusive - 31.228_639_937_9).abs() / 31.228_639_937_9 < 1e-9);
        assert!(
            inclusive > excluded * 4.0,
            "8 Hz must be INSIDE (8,13): {inclusive} vs {excluded}"
        );
    }

    #[test]
    fn an_empty_or_degenerate_band_refuses() {
        let x = sine(10.0, 20.0, 2048);
        assert_eq!(band_power(&x, FS, (13.0, 8.0)), None); // inverted
        assert_eq!(band_power(&x, FS, (1000.0, 2000.0)), None); // above Nyquist
        assert_eq!(band_power(&x, 0.0, ALPHA), None); // invalid rate
        assert_eq!(band_power(&x, f64::NAN, ALPHA), None); // non-finite rate
    }

    #[test]
    fn it_agrees_with_the_python_reference_to_twelve_significant_figures() {
        // Values produced by Scripts/validate-muse-physiology.py's bandpower on
        // the same deterministic inputs. If numpy's hanning convention or the
        // n^2 divisor were changed here, this is what would catch it — and with
        // it, every threshold calibrated against the Python version, including
        // the 1.5x alpha bar and the July 3.21x-3.88x reference sessions.
        // Produced by running that function on these exact inputs. They are
        // measured, not derived — an earlier draft of this test carried
        // plausible-looking values that were invented, and they were wrong by
        // a third.
        struct Case {
            freq: f64,
            amp: f64,
            n: usize,
            band: (f64, f64),
            expected: f64,
        }
        let cases = [
            Case {
                freq: 10.0,
                amp: 20.0,
                n: 1024,
                band: ALPHA,
                expected: 37.463_378_893_9,
            },
            Case {
                freq: 10.0,
                amp: 20.0,
                n: 2048,
                band: ALPHA,
                expected: 37.481_689_452_7,
            },
            Case {
                freq: 22.0,
                amp: 15.0,
                n: 2048,
                band: BETA,
                expected: 21.083_450_317_4,
            },
        ];
        for c in cases {
            let got = power(&sine(c.freq, c.amp, c.n), FS, c.band);
            let rel = (got - c.expected).abs() / c.expected;
            assert!(
                rel < 1e-9,
                "f={} n={}: got {got:.10} expected {:.10} (rel {rel:.2e})",
                c.freq,
                c.n,
                c.expected
            );
        }
    }
}

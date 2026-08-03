//! Why a channel is bad, not just that it is.
//!
//! `ChannelHealthThresholds` answers "is this RMS in range" and stops there.
//! Its own documentation says `saturated` covers "muscle / motion contamination
//! or 50/60 Hz interference, sometimes an analog front-end fault" — three causes
//! behind one number, with three different remedies. A user reading
//! `AF7 297.32 saturated` learns that something is wrong and nothing about what
//! to do, which in practice meant asking an assistant to run a spectrum by hand.
//! This module is that hand analysis, moved into the core.
//!
//! THE DISCRIMINATOR
//!
//! An electrode with poor skin contact has high impedance and couples ambient
//! mains into the amplifier; a well-contacted one shunts it. So mains-band power
//! separates contact problems from the other causes of a high RMS, and it does
//! it on an axis the amplitude threshold cannot see. Muscle is broadband, motion
//! is low-frequency, mains is a line — three signatures, one measurement each.
//!
//! Measured on 2026-08-03 (Muse S, Ubuntu 26.04, three recordings), AF7 against
//! AF8 — same electrode type, opposite sides of the same head, same instant:
//!
//! | band | AF7 / AF8 |
//! |---|---|
//! | drift 0.5-4 Hz | 0.2x |
//! | beta 13-30 Hz | 51.8x |
//! | mains 59-61 Hz | **7665.8x** |
//!
//! Drift being LOWER on the bad channel is what rules out motion, and the
//! narrow 59-61 band holding most of the 60-100 Hz energy is what rules out
//! muscle. Neither conclusion is reachable from RMS.
//!
//! THE THRESHOLDS ARE CALIBRATION-LIMITED AND THE GREY ZONE IS REAL
//!
//! Derived from one subject, one session, three recordings, one mains
//! environment. Clean channels measured 0.79, 2.52, 5.45, 10.61, 17.80, 23.41;
//! contaminated ones 45.4, 125.0, 780.3, 1806.0, 6059.8. The gap between the
//! worst clean and the best contaminated observation is a factor of two, which
//! is not enough to justify a sharp line — so there is a `Elevated` tier
//! between them rather than a pretence of one. These carry the same caveat as
//! the 2 / 200 uV amplitude thresholds they sit beside: engineering heuristics,
//! NOT physiologically validated cutoffs, to be tuned as recordings accumulate.
//!
//! One retrospective check exists and it is not validation. In the eyes-open /
//! eyes-closed session, the two channels this module would call clean (TP10,
//! TP9) produced alpha ratios of 1.98 and 1.90, and AF7 — the worst mains
//! channel in every recording — produced 0.72, no effect. Mains power predicted
//! which channels carried the physiology. That is a single session, post hoc,
//! and it is recorded as a consistency check rather than evidence.
//!
//! ABSOLUTE, NOT FRACTIONAL. An earlier draft normalised mains power by total
//! band power. That inverts on quiet channels: in the eyes-closed recording TP10
//! had a mains FRACTION of 0.41 against AF7's 0.16, because dividing a small
//! absolute hum by a small total inflates it — and TP10 is the channel that
//! passed the alpha check while AF7 failed it. A fractional threshold would have
//! condemned the best channel in the session. `band_power`'s scaling is very
//! nearly independent of window length (37.4634 at n=1024 vs 37.4817 at n=2048
//! for the same tone), so absolute values are comparable across runs at a fixed
//! sample rate, which is what makes the absolute form usable at all.

use crate::band_power::band_power;
use crate::channel_health::{ChannelHealthStatus, ChannelHealthThresholds};

/// Half-width of the mains band, in Hz. A line is narrow; widening this pulls
/// in neighbouring EEG and blunts the discriminator.
const MAINS_HALF_WIDTH_HZ: f64 = 1.0;

/// The two mains frequencies in use worldwide. Both are measured and the larger
/// reported, so the check needs no configuration and works on either grid.
const MAINS_CANDIDATES_HZ: [f64; 2] = [50.0, 60.0];

/// What is wrong with a channel, and therefore what to do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElectrodeVerdict {
    /// Amplitude in range, mains low.
    Ok,
    /// Mains power in the grey zone between the cleanest and dirtiest
    /// observations used to calibrate this. Usable, worth watching.
    Elevated,
    /// Mains power high: the electrode is coupling line noise, which means
    /// high contact impedance.
    MainsPickup,
    /// Amplitude above the saturated threshold WITHOUT a mains line, so the
    /// excess is broadband or low-frequency — muscle, jaw, or movement.
    MuscleOrMotion,
    /// Amplitude below the dead threshold: lifted off the skin, or a dry pad.
    Lifted,
    /// Not enough samples, a non-finite sample, or a sample rate too low to
    /// resolve the mains band at all.
    Unknown,
}

impl ElectrodeVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Elevated => "elevated",
            Self::MainsPickup => "mains-pickup",
            Self::MuscleOrMotion => "muscle-or-motion",
            Self::Lifted => "lifted",
            Self::Unknown => "unknown",
        }
    }

    /// What the user should physically do. Empty for `Ok`, because advice
    /// attached to a working channel trains people to ignore advice.
    pub fn advice(self) -> &'static str {
        match self {
            Self::Ok => "",
            Self::Elevated => "usable; if the session matters, reseat this pad",
            Self::MainsPickup => {
                "poor skin contact — moisten the pad, clear hair beneath it, press it down"
            }
            Self::MuscleOrMotion => "relax the jaw and hold still; this is not a contact fault",
            Self::Lifted => "the pad is off the skin — reseat it",
            Self::Unknown => "not enough clean data to judge",
        }
    }

    /// Whether this verdict should stop a user from recording.
    pub fn is_blocking(self) -> bool {
        matches!(self, Self::MainsPickup | Self::Lifted)
    }
}

/// Mains-band cutoffs, in `band_power` units.
///
/// See the module docs: derived 2026-08-03 from one subject and one mains
/// environment. NOT validated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainsThresholds {
    /// At or below this, the channel is treated as clean.
    pub watch: f64,
    /// Above this, the channel is treated as coupling mains.
    pub high: f64,
}

impl Default for MainsThresholds {
    fn default() -> Self {
        // 25 sits just above the worst clean observation (23.41) and 100 just
        // below the best confidently-contaminated one (125.0). Everything
        // between is reported as `Elevated` rather than forced to a side,
        // because the calibration data does not support a sharper split.
        Self {
            watch: 25.0,
            high: 100.0,
        }
    }
}

impl MainsThresholds {
    pub fn new(watch: f64, high: f64) -> Option<Self> {
        if !watch.is_finite() || !high.is_finite() {
            return None;
        }
        if watch < 0.0 || high <= watch {
            return None;
        }
        Some(Self { watch, high })
    }
}

/// Everything the check measured for one channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElectrodeReport {
    pub rms: f64,
    pub health: ChannelHealthStatus,
    /// Power in the winning mains band.
    pub mains_power: f64,
    /// Which line frequency carried more power. `None` when neither band is
    /// resolvable at this sample rate.
    pub line_hz: Option<f64>,
    pub verdict: ElectrodeVerdict,
}

/// Assess one channel from its raw samples.
///
/// Pure, like the two modules it builds on: no I/O, no clock, no state. The
/// caller supplies a contiguous window; 5-15 s at 256 Hz is what this was
/// calibrated on, and `band_power`'s own one-second floor applies below that.
///
/// ORDER OF PRECEDENCE, and it is deliberate. `Lifted` outranks everything
/// because a detached electrode's mains reading means nothing. `MainsPickup`
/// outranks `MuscleOrMotion` because contact is the fixable cause and because
/// a mains line inflates RMS enough to trip the saturation threshold on its
/// own — tonight AF7 read 285 uV with 6060 of that being line noise, and
/// calling it muscle would have sent the user to relax their jaw instead of
/// reseating the pad.
pub fn assess_channel(
    samples: &[f64],
    fs: f64,
    health: ChannelHealthThresholds,
    mains: MainsThresholds,
) -> ElectrodeReport {
    let n = samples.len();
    let rms = if n == 0 {
        f64::NAN
    } else {
        (samples.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt()
    };
    let status = health.status(rms, n as u64);

    // Measure both grids and keep the larger. Bands above Nyquist yield 0.0
    // from `band_power`, which is why `line_hz` is an Option: a rate too low to
    // see either line must report "cannot tell", never "clean".
    //
    // Finiteness is established BEFORE the band comparison rather than folded
    // into it, for the reason `ChannelHealthThresholds::new` gives: writing the
    // guard as a negated comparison would also reject NaN, but a reader cannot
    // tell whether that is deliberate. It is deliberate — a NaN sample rate
    // must reach `Unknown`, never `Ok` — so it is stated separately. Setting
    // `nyquist` to 0.0 for an invalid rate makes every candidate band fail the
    // positive test below and leaves `best` as `None`.
    let nyquist = if fs.is_finite() && fs > 0.0 {
        fs / 2.0
    } else {
        0.0
    };
    let mut best: Option<(f64, f64)> = None;
    for f0 in MAINS_CANDIDATES_HZ {
        if f0 + MAINS_HALF_WIDTH_HZ >= nyquist {
            continue;
        }
        let p = band_power(
            samples,
            fs,
            (f0 - MAINS_HALF_WIDTH_HZ, f0 + MAINS_HALF_WIDTH_HZ),
        );
        if best.is_none_or(|(bp, _)| p > bp) {
            best = Some((p, f0));
        }
    }

    let (mains_power, line_hz) = match best {
        Some((p, f0)) => (p, Some(f0)),
        None => (0.0, None),
    };

    let verdict = if status == ChannelHealthStatus::Unknown || line_hz.is_none() {
        ElectrodeVerdict::Unknown
    } else if status == ChannelHealthStatus::Dead {
        ElectrodeVerdict::Lifted
    } else if mains_power > mains.high {
        ElectrodeVerdict::MainsPickup
    } else if status == ChannelHealthStatus::Saturated {
        ElectrodeVerdict::MuscleOrMotion
    } else if mains_power > mains.watch {
        ElectrodeVerdict::Elevated
    } else {
        ElectrodeVerdict::Ok
    };

    ElectrodeReport {
        rms,
        health: status,
        mains_power,
        line_hz,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 256.0;

    fn health() -> ChannelHealthThresholds {
        ChannelHealthThresholds::default()
    }

    fn tone(freq: f64, amp: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| amp * (2.0 * std::f64::consts::PI * freq * i as f64 / FS).sin())
            .collect()
    }

    fn mixed(components: &[(f64, f64)], n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| {
                components
                    .iter()
                    .map(|(f, a)| a * (2.0 * std::f64::consts::PI * f * i as f64 / FS).sin())
                    .sum()
            })
            .collect()
    }

    fn assess(x: &[f64]) -> ElectrodeReport {
        assess_channel(x, FS, health(), MainsThresholds::default())
    }

    #[test]
    fn a_clean_alpha_channel_is_ok() {
        let x = mixed(&[(10.0, 15.0), (20.0, 5.0)], 2560);
        let r = assess(&x);
        assert_eq!(r.verdict, ElectrodeVerdict::Ok, "{r:?}");
        assert!(r.mains_power < 1e-6, "no line present: {}", r.mains_power);
    }

    #[test]
    fn a_sixty_hertz_line_reads_as_mains_pickup_and_names_the_frequency() {
        let x = mixed(&[(10.0, 15.0), (60.0, 40.0)], 2560);
        let r = assess(&x);
        assert_eq!(r.verdict, ElectrodeVerdict::MainsPickup, "{r:?}");
        assert_eq!(r.line_hz, Some(60.0));
    }

    #[test]
    fn a_fifty_hertz_line_is_detected_on_the_other_grid() {
        // The check is not configured for a region; it measures both.
        let x = mixed(&[(10.0, 15.0), (50.0, 40.0)], 2560);
        let r = assess(&x);
        assert_eq!(r.verdict, ElectrodeVerdict::MainsPickup, "{r:?}");
        assert_eq!(r.line_hz, Some(50.0));
    }

    #[test]
    fn a_high_amplitude_channel_with_no_line_is_muscle_not_contact() {
        // THIS is the distinction the RMS threshold alone cannot make, and the
        // one that decides whether the user reseats a pad or relaxes their jaw.
        let x = mixed(&[(35.0, 200.0), (45.0, 200.0), (75.0, 180.0)], 2560);
        let r = assess(&x);
        assert_eq!(r.health, ChannelHealthStatus::Saturated);
        assert_eq!(r.verdict, ElectrodeVerdict::MuscleOrMotion, "{r:?}");
    }

    #[test]
    fn mains_outranks_saturation_when_both_fire() {
        // AF7 on 2026-08-03: 285 uV RMS, saturated, with 6060 of mains. Calling
        // that muscle would send the user to relax their jaw instead of
        // reseating the pad, so the ordering is asserted rather than assumed.
        let x = mixed(&[(60.0, 400.0), (35.0, 60.0)], 2560);
        let r = assess(&x);
        assert_eq!(r.health, ChannelHealthStatus::Saturated);
        assert_eq!(r.verdict, ElectrodeVerdict::MainsPickup, "{r:?}");
    }

    #[test]
    fn a_flat_channel_is_lifted_and_lifted_outranks_everything() {
        let x = tone(10.0, 0.5, 2560);
        let r = assess(&x);
        assert_eq!(r.health, ChannelHealthStatus::Dead);
        assert_eq!(r.verdict, ElectrodeVerdict::Lifted);
        // Even with a line present, a detached electrode is reported as
        // detached: its mains reading describes the air, not the skin.
        let y = mixed(&[(60.0, 1.0), (10.0, 0.2)], 2560);
        let r2 = assess_channel(&y, FS, health(), MainsThresholds::new(1e-9, 2e-9).unwrap());
        assert_eq!(r2.health, ChannelHealthStatus::Dead);
        assert_eq!(r2.verdict, ElectrodeVerdict::Lifted, "{r2:?}");
    }

    #[test]
    fn too_few_samples_is_unknown_never_ok() {
        let r = assess(&tone(10.0, 15.0, 16));
        assert_eq!(r.verdict, ElectrodeVerdict::Unknown);
        let r = assess(&[]);
        assert_eq!(r.verdict, ElectrodeVerdict::Unknown);
    }

    #[test]
    fn a_sample_rate_too_low_to_see_the_line_reports_unknown_not_clean() {
        // At 90 Hz, Nyquist is 45 and neither 50 nor 60 is resolvable. The
        // failure mode this guards is the dangerous one: returning `Ok`
        // because no mains power was found, when none COULD have been found.
        let x: Vec<f64> = (0..900)
            .map(|i| 15.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 90.0).sin())
            .collect();
        let r = assess_channel(&x, 90.0, health(), MainsThresholds::default());
        assert_eq!(r.line_hz, None);
        assert_eq!(r.verdict, ElectrodeVerdict::Unknown, "{r:?}");
    }

    #[test]
    fn a_non_finite_or_zero_sample_rate_is_unknown_not_ok() {
        // The dangerous direction: with no valid rate there is no band to look
        // in, so no mains power is found, and a naive implementation calls that
        // clean. Both the NaN and the zero case must reach Unknown.
        let x = tone(10.0, 15.0, 2560);
        for fs in [f64::NAN, 0.0, -256.0, f64::INFINITY] {
            let r = assess_channel(&x, fs, health(), MainsThresholds::default());
            assert_eq!(r.line_hz, None, "fs={fs}");
            assert_eq!(r.verdict, ElectrodeVerdict::Unknown, "fs={fs}");
        }
    }

    #[test]
    fn a_non_finite_sample_is_unknown_not_ok() {
        let mut x = tone(10.0, 15.0, 2560);
        x[9] = f64::NAN;
        let r = assess(&x);
        assert_eq!(r.verdict, ElectrodeVerdict::Unknown, "{r:?}");
    }

    #[test]
    fn the_grey_zone_exists_and_is_not_collapsed_to_either_side() {
        // A mutation making `Elevated` unreachable — by folding it into `Ok` or
        // into `MainsPickup` — passes every other test in this module, because
        // every other fixture sits far from the boundary. The calibration data
        // has only a 2x gap between the worst clean and best dirty observation,
        // so the middle tier carries real information and is pinned here.
        let t = MainsThresholds::default();
        let mid = (t.watch * t.high).sqrt();
        let x = mixed(&[(10.0, 15.0), (60.0, 1.0)], 2560);
        let scale = (mid / assess(&x).mains_power).sqrt();
        let y = mixed(&[(10.0, 15.0), (60.0, scale)], 2560);
        let r = assess(&y);
        assert_eq!(r.verdict, ElectrodeVerdict::Elevated, "{r:?}");
        assert!(
            r.mains_power > t.watch && r.mains_power <= t.high,
            "fixture must land between the tiers, got {}",
            r.mains_power
        );
    }

    #[test]
    fn the_tier_boundaries_are_strict_in_both_directions() {
        // Pinned so that flipping `>` to `>=` on either comparison fails here.
        let t = MainsThresholds::default();
        let below = MainsThresholds::new(t.watch, 1e12).unwrap();
        let x = mixed(&[(10.0, 15.0), (60.0, 6.0)], 2560);
        let p = assess(&x).mains_power;
        // Exactly at `watch` is clean, a hair above is elevated.
        let at = MainsThresholds::new(p, p * 10.0).unwrap();
        assert_eq!(
            assess_channel(&x, FS, health(), at).verdict,
            ElectrodeVerdict::Ok
        );
        let just_under = MainsThresholds::new(p * 0.999, p * 10.0).unwrap();
        assert_eq!(
            assess_channel(&x, FS, health(), just_under).verdict,
            ElectrodeVerdict::Elevated
        );
        // Exactly at `high` is elevated, not pickup.
        let at_high = MainsThresholds::new(p * 0.5, p).unwrap();
        assert_eq!(
            assess_channel(&x, FS, health(), at_high).verdict,
            ElectrodeVerdict::Elevated
        );
        let _ = below;
    }

    #[test]
    fn invalid_thresholds_are_rejected_rather_than_panicking() {
        assert!(MainsThresholds::new(-1.0, 100.0).is_none());
        assert!(MainsThresholds::new(100.0, 100.0).is_none()); // not >
        assert!(MainsThresholds::new(100.0, 50.0).is_none());
        assert!(MainsThresholds::new(f64::NAN, 100.0).is_none());
    }

    #[test]
    fn only_contact_and_lift_faults_block_a_recording() {
        // Muscle and motion are the user's posture, not the hardware's fault,
        // and a check that blocks on them would cry wolf every time someone
        // swallowed. Elevated is explicitly usable.
        assert!(ElectrodeVerdict::MainsPickup.is_blocking());
        assert!(ElectrodeVerdict::Lifted.is_blocking());
        assert!(!ElectrodeVerdict::MuscleOrMotion.is_blocking());
        assert!(!ElectrodeVerdict::Elevated.is_blocking());
        assert!(!ElectrodeVerdict::Ok.is_blocking());
        assert!(!ElectrodeVerdict::Unknown.is_blocking());
        // Only a working channel gets no advice.
        assert!(ElectrodeVerdict::Ok.advice().is_empty());
        for v in [
            ElectrodeVerdict::Elevated,
            ElectrodeVerdict::MainsPickup,
            ElectrodeVerdict::MuscleOrMotion,
            ElectrodeVerdict::Lifted,
            ElectrodeVerdict::Unknown,
        ] {
            assert!(!v.advice().is_empty(), "{v:?} needs advice");
        }
    }

    #[test]
    fn the_2026_08_03_session_classifies_as_diagnosed_by_hand() {
        // Absolute mains-band powers measured from the three recordings of that
        // session, fed through the same thresholds. This is the regression that
        // a threshold change has to confront: it encodes which channels were
        // actually good, including the retrospective fact that the two channels
        // called clean here (TP10, TP9-at-rest) are the two that produced the
        // 1.98x and 1.90x alpha effects, and AF7 — dirty in all three — gave
        // 0.72 and no effect at all.
        let t = MainsThresholds::default();
        let clean = [
            ("AF8 diag", 0.79),
            ("TP10 diag", 2.52),
            ("AF8 closed", 5.45),
            ("TP9 open", 8.11),
            ("AF8 open", 10.61),
            ("TP10 closed", 17.80),
            ("TP10 open", 23.41),
        ];
        for (label, p) in clean {
            assert!(p <= t.watch, "{label} at {p} should be clean");
        }
        let dirty = [
            ("AF7 closed", 125.01),
            ("AF7 open", 780.26),
            ("TP9 diag", 1805.97),
            ("AF7 diag", 6059.77),
        ];
        for (label, p) in dirty {
            assert!(p > t.high, "{label} at {p} should be mains pickup");
        }
        // TP9 in the eyes-closed recording is the one observation in the grey
        // zone, and it is recorded as such rather than forced to a side.
        assert!(45.41 > t.watch && 45.41 <= t.high, "TP9 closed is elevated");
    }
}

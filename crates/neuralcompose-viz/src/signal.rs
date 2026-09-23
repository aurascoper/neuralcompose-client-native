//! Deterministic display transforms. Geometry is not a cognitive classifier.
use neuralcompose_mobile_core::{band_power, EEGSample};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mapping {
    Channels,
    Delay,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub mapping: Mapping,
    pub channel: usize,
    pub delay: usize,
    pub rate: f64,
    pub scale: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            mapping: Mapping::Channels,
            channel: 1,
            delay: 16,
            rate: 256.0,
            scale: 250.0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub channels: [f32; 4],
    pub life: f32,
}
#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    pub strips: Vec<Range<u32>>,
    pub clipped: usize,
    pub examined: usize,
}

pub fn contiguous(samples: &[EEGSample], rate: f64) -> Vec<Range<usize>> {
    if samples.is_empty() || !rate.is_finite() || rate <= 0.0 {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut start = 0;
    for i in 1..samples.len() {
        let dt = samples[i].timestamp - samples[i - 1].timestamp;
        if !dt.is_finite() || dt <= 0.0 || dt > 2.0 / rate {
            spans.push(start..i);
            start = i;
        }
    }
    spans.push(start..samples.len());
    spans
}

pub fn geometry(samples: &[EEGSample], config: Config) -> Geometry {
    let mut result = Geometry::default();
    if samples.is_empty()
        || config.channel >= 4
        || config.delay == 0
        || !config.scale.is_finite()
        || config.scale <= 0.0
    {
        return result;
    }
    let n = samples.len() as f64;
    let mut mean = [0.0; 4];
    for sample in samples {
        for (j, m) in mean.iter_mut().enumerate() {
            *m += sample.channels[j] / n;
        }
    }
    for span in contiguous(samples, config.rate) {
        let lag = if config.mapping == Mapping::Delay {
            config.delay.saturating_mul(2)
        } else {
            0
        };
        if span.len() <= lag {
            continue;
        }
        let mut start = result.vertices.len() as u32;
        for i in span.start..span.end - lag {
            let newest = i + lag;
            let mut values = samples[i].channels;
            if config.mapping == Mapping::Delay {
                let ch = config.channel;
                values = [
                    samples[newest].channels[ch],
                    samples[i].channels[ch],
                    samples[i + config.delay].channels[ch],
                    samples[newest].channels[3],
                ];
            }
            let normalized: [f32; 4] = std::array::from_fn(|j| {
                let m = if config.mapping == Mapping::Delay && j < 3 {
                    mean[config.channel]
                } else {
                    mean[j]
                };
                ((values[j] - m) / config.scale) as f32
            });
            result.examined += 1;
            if normalized.iter().any(|v| !v.is_finite()) {
                let end = result.vertices.len() as u32;
                if end > start {
                    result.strips.push(start..end);
                }
                start = end;
                continue;
            }
            result.clipped += usize::from(normalized.iter().any(|v| v.abs() > 1.0));
            let life = if samples.len() == 1 {
                1.0
            } else {
                newest as f32 / (samples.len() - 1) as f32
            };
            result.vertices.push(Vertex {
                channels: normalized,
                life,
            });
        }
        let end = result.vertices.len() as u32;
        if end > start {
            result.strips.push(start..end);
        }
    }
    result
}

/// Fixed-window relative band contributions; never physical power units.
pub fn band_ratio(samples: &[EEGSample], rate: f64) -> Option<f32> {
    let span = contiguous(samples, rate).pop()?;
    if span.len() < 512 {
        return None;
    }
    let window = &samples[span.end - 512..span.end];
    let mut alpha = 0.0;
    let mut theta = 0.0;
    for ch in 0..4 {
        let values: Vec<f64> = window.iter().map(|s| s.channels[ch]).collect();
        alpha += band_power(&values, rate, (8.0, 13.0))?;
        theta += band_power(&values, rate, (4.0, 8.0_f64.next_down()))?;
    }
    let total = alpha + theta;
    (total.is_finite() && total > 0.0).then_some((alpha / total) as f32)
}

/// Fixed seed, independently reproducible from sample index. Synthetic only.
pub fn fixture(i: u64, kind: &str) -> EEGSample {
    let t = i as f64 / 256.0;
    let channels = std::array::from_fn(|ch| match kind {
        "noise" => {
            let mut x = i
                .wrapping_mul(6364136223846793005)
                .wrapping_add((ch as u64 + 1) * 1442695040888963407);
            x ^= x >> 21;
            x = x.wrapping_mul(2862933555777941757);
            x ^= x >> 33;
            ((x >> 32) as f64 / u32::MAX as f64 - 0.5) * 200.0
        }
        "impulse" => {
            if (i + ch as u64 * 13) % 256 < 3 {
                380.0
            } else {
                0.0
            }
        }
        _ => {
            75.0 * (std::f64::consts::TAU * 10.0 * t + ch as f64 * 0.55).sin()
                + 35.0 * (std::f64::consts::TAU * 6.0 * t + ch as f64).sin()
        }
    });
    EEGSample {
        timestamp: t,
        channels,
    }
}

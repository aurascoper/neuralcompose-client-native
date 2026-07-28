//! Bounded rolling sample buffer — pinned to the Expo client's
//! `src/hooks/eegBuffer.ts` semantics (parity over elegance; a fixed ring is a
//! later optimization, the trim-at-2×keep behavior is the contract).

use crate::types::{EEGSample, CHANNEL_COUNT};

#[derive(Debug, Clone)]
pub struct SampleBuffer {
    samples: Vec<EEGSample>,
    keep: usize,
}

impl SampleBuffer {
    pub fn new(keep: u32) -> Self {
        let keep = keep.max(1) as usize;
        Self {
            samples: Vec::with_capacity(keep * 2 + 1),
            keep,
        }
    }

    /// Append a sample; once the buffer exceeds 2×keep it is trimmed to the
    /// newest `keep`.
    pub fn push(&mut self, s: EEGSample) {
        self.samples.push(s);
        if self.samples.len() > self.keep * 2 {
            self.samples.drain(..self.samples.len() - self.keep);
        }
    }

    /// Newest `keep` samples split into per-channel arrays in fixed
    /// TP9, AF7, AF8, TP10 order. Never reorders channels.
    pub fn channel_arrays(&self) -> [Vec<f64>; CHANNEL_COUNT] {
        let start = self.samples.len().saturating_sub(self.keep);
        let tail = &self.samples[start..];
        let mut out: [Vec<f64>; CHANNEL_COUNT] = [
            Vec::with_capacity(tail.len()),
            Vec::with_capacity(tail.len()),
            Vec::with_capacity(tail.len()),
            Vec::with_capacity(tail.len()),
        ];
        for s in tail {
            for (i, ch) in out.iter_mut().enumerate() {
                ch.push(s.channels[i]);
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

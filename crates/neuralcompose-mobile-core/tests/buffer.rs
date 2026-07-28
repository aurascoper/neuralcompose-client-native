// Port of the Expo client's src/hooks/__tests__/eegBuffer.test.ts, plus
// property tests for boundedness.

use neuralcompose_mobile_core::{EEGSample, SampleBuffer};
use proptest::prelude::*;

fn sample(t: f64) -> EEGSample {
    EEGSample {
        timestamp: t,
        channels: [t, t + 1.0, t + 2.0, t + 3.0],
    }
}

#[test]
fn never_grows_beyond_2x_keep() {
    let keep = 8u32;
    let mut buf = SampleBuffer::new(keep);
    for i in 0..1000 {
        buf.push(sample(i as f64));
        assert!(
            buf.len() <= (keep as usize) * 2,
            "len {} at push {}",
            buf.len(),
            i
        );
    }
}

#[test]
fn keeps_newest_samples_in_order_after_trimming() {
    let keep = 4u32;
    let mut buf = SampleBuffer::new(keep);
    for i in 0..100 {
        buf.push(sample(i as f64));
    }
    let chans = buf.channel_arrays();
    let timestamps = &chans[0]; // channel 0 mirrors timestamp by construction
    assert_eq!(*timestamps.last().unwrap(), 99.0);
    let mut sorted = timestamps.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(&sorted, timestamps, "order preserved");
}

#[test]
fn splits_into_fixed_channel_order() {
    let mut buf = SampleBuffer::new(100);
    buf.push(sample(10.0));
    buf.push(sample(20.0));
    let chans = buf.channel_arrays();
    assert_eq!(chans[0], vec![10.0, 20.0]);
    assert_eq!(chans[1], vec![11.0, 21.0]);
    assert_eq!(chans[2], vec![12.0, 22.0]);
    assert_eq!(chans[3], vec![13.0, 23.0]);
}

#[test]
fn windows_to_newest_keep_samples() {
    let mut buf = SampleBuffer::new(10);
    for i in 0..15 {
        buf.push(sample(i as f64));
    }
    let chans = buf.channel_arrays();
    assert_eq!(chans[0].len(), 10);
    assert_eq!(chans[0][9], 14.0);
}

proptest! {
    #[test]
    fn prop_bounded_for_any_push_sequence(keep in 1u32..64, count in 0usize..2000) {
        let mut buf = SampleBuffer::new(keep);
        for i in 0..count {
            buf.push(sample(i as f64));
            prop_assert!(buf.len() <= (keep as usize) * 2);
        }
        let chans = buf.channel_arrays();
        prop_assert!(chans[0].len() <= keep as usize);
        for c in &chans {
            prop_assert_eq!(c.len(), chans[0].len());
        }
    }
}

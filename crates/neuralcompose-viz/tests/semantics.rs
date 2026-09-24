use neuralcompose_hypnagogic::embedding::Embedding;
use neuralcompose_mobile_core::{EEGSample, SocketEvent, StreamMonitor, StreamPhase};
use neuralcompose_viz::{
    metrics::{verdict, Run},
    semantic::{cosine, position},
    signal::{self, Config, Mapping},
};

#[test]
fn axes_use_named_channels_and_delay_uses_only_contiguous_samples() {
    let samples: Vec<_> = (0..6)
        .map(|i| EEGSample {
            timestamp: i as f64 / 256.0,
            channels: [i as f64, 2.0 * i as f64, 3.0 * i as f64, 4.0 * i as f64],
        })
        .collect();
    let g = signal::geometry(
        &samples,
        Config {
            scale: 1.0,
            ..Default::default()
        },
    );
    assert_eq!(g.vertices.last().unwrap().channels, [2.5, 5.0, 7.5, 10.0]);
    let mut cfg = Config {
        mapping: Mapping::Delay,
        delay: 1,
        scale: 1.0,
        ..Default::default()
    };
    let d = signal::geometry(&samples, cfg);
    assert_eq!(d.vertices.len(), 4);
    assert_eq!(d.vertices[0].channels, [-1.0, -5.0, -3.0, -2.0]);
    let mut gap = samples.clone();
    for s in &mut gap[3..] {
        s.timestamp += 1.0;
    }
    let d = signal::geometry(&gap, cfg);
    assert_eq!(d.strips.len(), 2);
    assert_eq!(d.vertices.len(), 2);
    cfg.delay = 4;
    assert!(signal::geometry(&gap, cfg).vertices.is_empty());
}
#[test]
fn old_generation_never_supplies_current_geometry_or_freshness() {
    let m = StreamMonitor::with_defaults();
    m.on_socket_event(SocketEvent::Opened, 0);
    m.on_frame(r#"{"timestamp":0,"channels":[1,2,3,4]}"#.into(), 1);
    assert_eq!(m.signal_snapshot(1).samples.len(), 1);
    m.on_socket_event(SocketEvent::Closed, 2);
    m.on_socket_event(SocketEvent::Opened, 3);
    let s = m.signal_snapshot(3);
    assert!(s.samples.is_empty());
    assert_eq!(s.phase, StreamPhase::OpenNoData);
    m.on_frame(r#"{"timestamp":1,"channels":[5,6,7,8]}"#.into(), 4);
    let s = m.signal_snapshot(4);
    assert_eq!(s.samples[0].channels, [5.0, 6.0, 7.0, 8.0]);
    assert_eq!(s.generation, 2);
    assert!(matches!(
        m.signal_snapshot(2005).phase,
        StreamPhase::Stale { .. }
    ));
}
#[test]
fn constants_and_extremes_never_become_nan_vertices_or_false_bands() {
    let constant = vec![
        EEGSample {
            timestamp: 0.0,
            channels: [10.0; 4]
        };
        512
    ];
    let g = signal::geometry(&constant, Config::default());
    assert!(g.vertices.iter().all(|v| v.channels == [0.0; 4]));
    assert_eq!(signal::band_ratio(&constant, 256.0), None);
    let huge: Vec<_> = (0..6)
        .map(|i| EEGSample {
            timestamp: i as f64 / 256.0,
            channels: [if i % 2 == 0 { 1e300 } else { -1e300 }; 4],
        })
        .collect();
    assert!(signal::geometry(&huge, Config::default())
        .vertices
        .is_empty());
}
#[test]
fn absent_invalid_and_incompatible_are_not_orthogonal() {
    let a = Embedding::new(vec![1.0, 0.0], "a");
    let b = Embedding::new(vec![0.0, 1.0], "a");
    assert_eq!(cosine(Some(&a), Some(&b)), Some(0.0));
    for e in [
        Embedding::new(vec![1.0, 0.0], "b"),
        Embedding::new(vec![0.0, 0.0], "a"),
        Embedding::new(vec![f32::NAN, 1.0], "a"),
        Embedding::new(vec![1.0], "a"),
    ] {
        assert_eq!(cosine(Some(&a), Some(&e)), None);
    }
    assert_eq!(cosine(None, Some(&a)), None);
    assert_eq!(position(3, 2), [3.0, 2.0]);
}
#[test]
fn repeated_run_verdicts_are_fixed_and_dirty_runs_cannot_pass() {
    let base = Run {
        schema: String::new(),
        case: "a".into(),
        repeat: 1,
        commit: "c".into(),
        executable_sha256: "d".into(),
        clean_tree: true,
        quotable: true,
        build_configuration: String::new(),
        gpu: String::new(),
        framebuffer: [1280, 800],
        duration_s: 60.0,
        rendered_frames: 3600,
        accepted_samples: 15360,
        fps: 60.0,
        frame_p95_ms: Some(17.0),
        age_p95_ms: Some(30.0),
        frame_intervals_ms: vec![],
        sample_to_submit_ms: vec![],
        live_fraction: 1.0,
        within_budget_fraction: 1.0,
        clipped_fraction: 0.0,
        skipped_notifications: 0,
    };
    let mut runs = vec![base; 3];
    runs[1].repeat = 2;
    runs[2].repeat = 3;
    assert_eq!(verdict(&runs), "accepted");
    runs[0].frame_p95_ms = Some(30.0);
    assert_eq!(verdict(&runs), "unresolved");
    runs[1].frame_p95_ms = Some(30.0);
    assert_eq!(verdict(&runs), "unresolved");
    runs[2].frame_p95_ms = Some(30.0);
    assert_eq!(verdict(&runs), "targets not met");
    runs[0].quotable = false;
    assert_eq!(verdict(&runs), "unresolved");
    assert_eq!(verdict(&runs[..2]), "unresolved");
    runs[0].quotable = true;
    runs[0].repeat = 2;
    assert_eq!(verdict(&runs), "unresolved");
}

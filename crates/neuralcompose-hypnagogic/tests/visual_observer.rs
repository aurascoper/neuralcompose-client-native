use neuralcompose_hypnagogic::{
    dialectic::{DialecticConfig, DialecticLoop},
    embedding::Embedding,
    profile::ContextProfile,
    role::waking_roles,
    seams::*,
    visual::{TurnVisual, VisualPublisher},
};
use std::sync::{mpsc, Arc, Barrier, Mutex};
use std::time::Duration;

type Trace = Arc<Mutex<Vec<String>>>;
struct Listener {
    n: usize,
    trace: Trace,
}
impl Listening for Listener {
    fn listen(&mut self) -> SeamResult<Option<String>> {
        self.trace.lock().unwrap().push("listen".into());
        let text = if self.n == 0 {
            "biofilm origin"
        } else {
            "mechanical whirring"
        };
        self.n += 1;
        Ok(Some(text.into()))
    }
}
struct Generator {
    trace: Trace,
}
impl TextGenerating for Generator {
    fn generate(&mut self, system: &str, prompt: &str, _: GenerationParams) -> SeamResult<String> {
        self.trace
            .lock()
            .unwrap()
            .push(format!("generate:{system}:{prompt}"));
        Ok("A shared repeated reply with identical words".into())
    }
}
struct Embedder {
    trace: Trace,
}
impl SentenceEmbedding for Embedder {
    fn embed(&mut self, t: &str) -> SeamResult<Embedding> {
        self.trace.lock().unwrap().push(format!("embed:{t}"));
        Ok(Embedding::new(
            if t.contains("biofilm") {
                vec![1.0, 0.0]
            } else {
                vec![0.0, 1.0]
            },
            "fixture",
        ))
    }
}
struct Speaker {
    trace: Trace,
}
impl Speaking for Speaker {
    fn speak(&mut self, t: &str, _: Prosody) -> SeamResult<()> {
        self.trace.lock().unwrap().push(format!("speak:{t}"));
        Ok(())
    }
}
struct Draws {
    trace: Trace,
}
impl SelectionDraws for Draws {
    fn next_draw(&mut self) -> f64 {
        self.trace.lock().unwrap().push("draw".into());
        0.5
    }
}
fn run(publisher: Option<&VisualPublisher>) -> (Vec<String>, Vec<String>) {
    let trace: Trace = Default::default();
    let mut l = DialecticLoop::new(
        Listener {
            n: 0,
            trace: trace.clone(),
        },
        Generator {
            trace: trace.clone(),
        },
        Speaker {
            trace: trace.clone(),
        },
        Embedder {
            trace: trace.clone(),
        },
        Draws {
            trace: trace.clone(),
        },
        waking_roles().to_vec(),
        ContextProfile::Focused,
        DialecticConfig::default(),
    );
    let mut logs = Vec::new();
    for _ in 0..8 {
        let t = match publisher {
            Some(p) => l.turn_with_observer(&mut |s| p.publish(s)),
            None => l.turn(),
        }
        .unwrap()
        .unwrap();
        logs.push(
            serde_json::to_string(&t.to_turn_line("focused", l.method_identity(), "fixture"))
                .unwrap(),
        );
    }
    let events = trace.lock().unwrap().clone();
    (events, logs)
}
#[test]
fn full_publisher_cannot_block_or_change_drift_prompts_draws_or_reply_ring() {
    let baseline = run(None);
    assert_eq!(baseline.0[1], "embed:biofilm origin");
    assert!(baseline
        .0
        .iter()
        .any(|s| s.contains("this exchange began with")));
    assert!(
        baseline
            .1
            .iter()
            .any(|s| s.contains("\"repetitionForcedSilence\":true")),
        "fixture must exercise reply-text history"
    );
    let (publisher, receiver) = VisualPublisher::new("parity".into());
    publisher.publish(&TurnVisual::new(99)); // Prefill the only notification slot.
    let barrier = Arc::new(Barrier::new(2));
    let held = barrier.clone();
    let consumer = std::thread::spawn(move || {
        held.wait();
        receiver.wake.recv().unwrap();
        receiver.latest().unwrap()
    });
    let (tx, rx) = mpsc::channel();
    let producer = std::thread::spawn(move || {
        let result = run(Some(&publisher));
        tx.send((result, publisher.full_count())).unwrap();
    });
    // Release only AFTER the turn result arrives. A blocking send cannot pass.
    let result = rx.recv_timeout(Duration::from_secs(5));
    barrier.wait();
    let packet = consumer.join().unwrap();
    let (observed, full) = result.expect("generation waited for the stalled consumer");
    producer.join().unwrap();
    assert!(full > 0, "the full-channel branch was never exercised");
    assert_eq!(observed, baseline);
    assert_eq!(packet.state.stage, "completed");
    assert_eq!(packet.state.turn, 7);
}
#[test]
fn disconnected_and_draining_observers_preserve_the_same_trace() {
    let baseline = run(None);
    let (p, r) = VisualPublisher::new("gone".into());
    drop(r);
    assert_eq!(run(Some(&p)), baseline);
    let (p, r) = VisualPublisher::new("drain".into());
    let consumer = std::thread::spawn(move || {
        while r.wake.recv().is_ok() {
            let _ = r.latest();
        }
    });
    assert_eq!(run(Some(&p)), baseline);
    drop(p);
    consumer.join().unwrap();
}
#[test]
fn protocol_rejects_malformed_or_nonfinite_values() {
    let mut s = TurnVisual::new(1);
    assert!(s.validate());
    s.schema = "unknown".into();
    assert!(!s.validate());
    s.schema = neuralcompose_hypnagogic::visual::VISUAL_SCHEMA.into();
    s.heard_embedding = Some(Embedding::new(vec![f32::NAN], "x"));
    assert!(!s.validate());
}

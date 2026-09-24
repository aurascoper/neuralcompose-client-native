//! Ephemeral observation values and a bounded, nonblocking latest-state mailbox.
//! No clock, socket, filesystem, model calls, or selection draws live here.
use crate::{dynamics::ScoredCandidate, embedding::Embedding};
use arc_swap::ArcSwapOption;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{sync_channel, Receiver, SyncSender, TrySendError},
    Arc,
};

pub const VISUAL_SCHEMA: &str = "neuralcompose.dialectic.visual.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VisualCandidate {
    pub role: String,
    pub text: String,
    pub embedding: Option<Embedding>,
    pub potential: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TurnVisual {
    pub schema: String,
    pub turn: u64,
    pub stage: String,
    pub heard: String,
    pub heard_embedding: Option<Embedding>,
    pub candidates: Vec<VisualCandidate>,
    pub weights: Option<Vec<f32>>,
    pub outcome: Option<String>,
    pub final_text: Option<String>,
    pub repetition_forced_silence: bool,
    pub reanchored: bool,
    pub error: Option<String>,
}

impl TurnVisual {
    pub fn new(turn: u64) -> Self {
        Self {
            schema: VISUAL_SCHEMA.into(),
            turn,
            stage: "listening".into(),
            heard: String::new(),
            heard_embedding: None,
            candidates: Vec::new(),
            weights: None,
            outcome: None,
            final_text: None,
            repetition_forced_silence: false,
            reanchored: false,
            error: None,
        }
    }
    pub fn scored(&mut self, candidates: &[ScoredCandidate], weights: Vec<f32>) {
        self.candidates = candidates
            .iter()
            .map(|s| VisualCandidate {
                role: s.candidate.role_id.clone(),
                text: s.candidate.text.clone(),
                embedding: Some(s.candidate.embedding.clone()),
                potential: Some(s.potential),
            })
            .collect();
        self.weights = Some(weights);
        self.stage = "scored".into();
    }
    /// Refuse corrupt/oversized snapshots at the local protocol boundary.
    pub fn validate(&self) -> bool {
        let valid_embedding = |e: &Embedding| {
            !e.model_id.is_empty()
                && !e.values.is_empty()
                && e.values.len() <= 32768
                && e.values.iter().all(|v| v.is_finite())
        };
        self.schema == VISUAL_SCHEMA
            && self.candidates.len() <= 16
            && self.heard.len() <= 65536
            && self.heard_embedding.as_ref().is_none_or(valid_embedding)
            && self.candidates.iter().all(|c| {
                c.text.len() <= 65536
                    && c.embedding.as_ref().is_none_or(valid_embedding)
                    && c.potential.is_none_or(f32::is_finite)
            })
            && self.weights.as_ref().is_none_or(|w| {
                w.len() == self.candidates.len()
                    && w.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x))
                    && (w.iter().sum::<f32>() - 1.0).abs() < 0.001
            })
    }
}

/// Wire wrapper. Epochs are intentionally separate from EEG source time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VisualPacket {
    pub session: String,
    pub sequence: u64,
    pub skipped_notifications: u64,
    pub state: TurnVisual,
}

pub struct VisualPublisher {
    latest: Arc<ArcSwapOption<VisualPacket>>,
    wake: SyncSender<()>,
    sequence: AtomicU64,
    full: AtomicU64,
    session: String,
}
pub struct VisualReceiver {
    pub wake: Receiver<()>,
    latest: Arc<ArcSwapOption<VisualPacket>>,
}
impl VisualPublisher {
    pub fn new(session: String) -> (Self, VisualReceiver) {
        let (wake, rx) = sync_channel(1);
        let latest = Arc::new(ArcSwapOption::empty());
        (
            Self {
                latest: latest.clone(),
                wake,
                sequence: AtomicU64::new(0),
                full: AtomicU64::new(0),
                session,
            },
            VisualReceiver { wake: rx, latest },
        )
    }
    pub fn publish(&self, state: &TurnVisual) {
        let packet = VisualPacket {
            session: self.session.clone(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            skipped_notifications: self.full.load(Ordering::Relaxed),
            state: state.clone(),
        };
        self.latest.store(Some(Arc::new(packet)));
        if let Err(TrySendError::Full(())) = self.wake.try_send(()) {
            self.full.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn full_count(&self) -> u64 {
        self.full.load(Ordering::Relaxed)
    }
}
impl VisualReceiver {
    pub fn latest(&self) -> Option<Arc<VisualPacket>> {
        self.latest.load_full()
    }
}

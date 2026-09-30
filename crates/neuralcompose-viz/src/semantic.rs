//! Position carries turn order and role. Cosine exists only on explicit edges.
use neuralcompose_hypnagogic::{
    embedding::Embedding,
    visual::{TurnVisual, VisualCandidate},
};

pub fn cosine(a: Option<&Embedding>, b: Option<&Embedding>) -> Option<f32> {
    let (a, b) = (a?, b?);
    if !a.is_comparable_with(b) {
        return None;
    }
    // f64 accumulation avoids f32 overflow; invalid/zero vectors stay absent.
    let mut dot = 0.0_f64;
    let mut aa = 0.0_f64;
    let mut bb = 0.0_f64;
    for (&x, &y) in a.values.iter().zip(&b.values) {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (x, y) = (x as f64, y as f64);
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa <= 0.0 || bb <= 0.0 {
        return None;
    }
    let value = dot / (aa.sqrt() * bb.sqrt());
    value.is_finite().then_some(value.clamp(-1.0, 1.0) as f32)
}

pub fn position(turn_slot: usize, role_slot: usize) -> [f32; 2] {
    [turn_slot as f32, role_slot as f32]
}

/// Clearly labeled semantic fixture, never substituted for an unavailable feed.
pub fn demo(turn: u64) -> TurnVisual {
    let embedding = |basis: [f32; 3]| {
        Embedding::new(
            (0..384).map(|i| basis[i % 3]).collect(),
            "synthetic-demo-384",
        )
    };
    let mut v = TurnVisual::new(turn);
    v.stage = "completed".into();
    v.heard = "How can two views describe the same signal?".into();
    v.heard_embedding = Some(embedding([1.0, 0.0, 0.0]));
    v.candidates = vec![
        VisualCandidate {
            role: "coherence".into(),
            text: "A shared signal can support several measured views.".into(),
            embedding: Some(embedding([0.9, 0.4, 0.1])),
            potential: Some(0.8),
        },
        VisualCandidate {
            role: "displacement".into(),
            text: "Each view also chooses which relations remain visible.".into(),
            embedding: Some(embedding([0.5, 0.7, 0.3])),
            potential: Some(0.6),
        },
    ];
    v.weights = Some(vec![0.6, 0.4]);
    v.outcome = Some("spoke:coherence".into());
    v.final_text = Some(v.candidates[0].text.clone());
    v
}

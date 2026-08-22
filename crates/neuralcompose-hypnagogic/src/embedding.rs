//! Minimal sentence-embedding value type — the input currency of the whole
//! dialectic.
//!
//! Defined here rather than reused from `neuralcompose-llama`: that crate is
//! the *producer* (it links llama.cpp through a hand-written C shim), and the
//! lib half of this crate must stay pure so the dialectic is testable with
//! fixed vectors and no model present. The shell wraps the real `Embedder` and
//! hands `Embedding` values in.
//!
//! Port of `Sources/BCICore/Models/Embedding.swift`, with **two deliberate
//! divergences**, both on error paths only:
//!
//! 1. **Comparability is enforced, not documented.** `docs/architecture/embedding_contract.md`
//!    §2.7/§3.3 and macOS repo ADR-010 both state that two embeddings with different
//!    `modelID`s are not comparable even when their dimensions match — but the
//!    Swift `cosineSimilarity(to:)` checks only `values.count`, so the rule is
//!    carried by a doc comment and nothing else. Here it is a type-level fact:
//!    an incomparable pair yields `None`.
//! 2. **`None`, never a sentinel.** Returning `0.0` for "incomparable" would be
//!    indistinguishable from a genuine orthogonal pair from the *same* model,
//!    which is a perfectly legal similarity. A mismatched candidate would then
//!    score as merely unaligned, lose the softmax, and the loop would carry on
//!    emitting plausible output forever. That is the exact silent-failure shape
//!    this port is trying not to reproduce, so the incomparable case is not
//!    representable as a score at all.
//!
//! On the happy path — one embedder, one dimension, L2-normalized values — this
//! agrees with the Swift exactly, which is what the conformance fixture pins.
//! The Swift relies on its stated L2-normalization invariant and takes a plain
//! dot product; this divides by the magnitudes, which is identical on unit
//! vectors and merely more forgiving off them.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    /// The vector. Producers are expected to emit L2-normalized values, as in
    /// the Swift; nothing here depends on it, because the similarity divides by
    /// the magnitudes anyway.
    pub values: Vec<f32>,
    /// Identity of the space these values live in. Comparisons across two
    /// different `model_id`s are not a number — see the module header.
    pub model_id: String,
}

impl Embedding {
    pub fn new(values: Vec<f32>, model_id: impl Into<String>) -> Self {
        Self {
            values,
            model_id: model_id.into(),
        }
    }

    pub fn dimension(&self) -> usize {
        self.values.len()
    }

    /// Whether these two live in the same embedding space at the same
    /// dimension. Cheap enough to call as an upfront guard when a caller would
    /// rather fail loudly once than thread `Option` through a scoring pass.
    pub fn is_comparable_with(&self, other: &Embedding) -> bool {
        self.model_id == other.model_id
            && self.values.len() == other.values.len()
            && !self.values.is_empty()
    }

    /// Cosine similarity in `[-1, 1]`, or `None` when the two are **not
    /// comparable**: a different embedding space, a different dimension, or
    /// either side degenerate (empty, or zero magnitude, which has no
    /// direction and therefore no angle to another vector).
    ///
    /// `None` is not a failure to compute — it is the statement that no
    /// similarity exists to compute. Callers decide whether that is a panic, a
    /// skipped candidate, or an aborted turn; what they cannot do is mistake it
    /// for a low score.
    pub fn cosine_similarity(&self, other: &Embedding) -> Option<f32> {
        if !self.is_comparable_with(other) {
            return None;
        }
        let mut dot = 0.0f32;
        let mut norm_a = 0.0f32;
        let mut norm_b = 0.0f32;
        for (a, b) in self.values.iter().zip(other.values.iter()) {
            dot += a * b;
            norm_a += a * a;
            norm_b += b * b;
        }
        let denom = norm_a.sqrt() * norm_b.sqrt();
        if denom <= 1e-6 || !denom.is_finite() {
            return None;
        }
        let cos = dot / denom;
        cos.is_finite().then_some(cos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(values: Vec<f32>, model: &str) -> Embedding {
        Embedding::new(values, model)
    }

    #[test]
    fn identical_vectors_are_maximally_similar() {
        let a = e(vec![1.0, 2.0, 3.0], "bge-small");
        assert!((a.cosine_similarity(&a).unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn opposed_vectors_are_minimally_similar() {
        let a = e(vec![1.0, 0.0], "bge-small");
        let b = e(vec![-1.0, 0.0], "bge-small");
        assert!((a.cosine_similarity(&b).unwrap() + 1.0).abs() < 1e-6);
    }

    /// The distinction the sentinel destroyed: orthogonal-and-comparable is a
    /// real score of 0.0, and incomparable is not a score at all. If these two
    /// ever compare equal again, a mismatched embedding space becomes
    /// indistinguishable from an unaligned candidate.
    #[test]
    fn orthogonal_is_a_score_and_incomparable_is_not() {
        let a = e(vec![1.0, 0.0], "bge-small");
        let orthogonal = e(vec![0.0, 1.0], "bge-small");
        let other_space = e(vec![1.0, 0.0], "all-MiniLM-L6-v2");

        assert_eq!(a.cosine_similarity(&orthogonal), Some(0.0));
        assert_eq!(a.cosine_similarity(&other_space), None);
        assert_ne!(
            a.cosine_similarity(&orthogonal),
            a.cosine_similarity(&other_space)
        );
    }

    /// Same numbers, same dimension, different space. A similarity here would
    /// be a fabricated comparison — the rule macOS repo ADR-010 states and the Swift
    /// enforces nowhere.
    #[test]
    fn different_model_ids_are_not_comparable() {
        let a = e(vec![1.0, 2.0, 3.0], "bge-small");
        let b = e(vec![1.0, 2.0, 3.0], "all-MiniLM-L6-v2");
        assert!(!a.is_comparable_with(&b));
        assert_eq!(a.cosine_similarity(&b), None);
    }

    #[test]
    fn dimension_mismatch_and_degenerate_inputs_are_not_comparable() {
        let a = e(vec![1.0, 2.0, 3.0], "bge-small");
        assert_eq!(a.cosine_similarity(&e(vec![1.0, 2.0], "bge-small")), None);
        // A zero vector has no direction, so there is no angle to report.
        assert_eq!(
            a.cosine_similarity(&e(vec![0.0, 0.0, 0.0], "bge-small")),
            None
        );
        assert_eq!(
            e(vec![], "bge-small").cosine_similarity(&e(vec![], "bge-small")),
            None
        );
    }

    /// Matches the Swift on its own stated invariant: for L2-normalized
    /// operands the similarity is the plain dot product.
    #[test]
    fn agrees_with_a_dot_product_on_normalized_vectors() {
        let inv = 1.0 / 2.0f32.sqrt();
        let a = e(vec![inv, inv], "bge-small");
        let b = e(vec![1.0, 0.0], "bge-small");
        let dot: f32 = a
            .values
            .iter()
            .zip(b.values.iter())
            .map(|(x, y)| x * y)
            .sum();
        assert!((a.cosine_similarity(&b).unwrap() - dot).abs() < 1e-6);
    }
}

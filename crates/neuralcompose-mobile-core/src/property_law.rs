//! Executable property law (M7-A2, ADR-002). Deterministic, effect-free.
//!
//! Three properties get first-class contract support here because getting
//! them wrong corrupts data silently rather than loudly:
//!
//! - **Idempotence** — indexing the same content under the same embedding
//!   identity twice yields one entry, never two.
//! - **Window-shift equivariance** — shifting an indexed window by N samples
//!   shifts detected spans by N; it never changes what was detected.
//! - **Channel-permutation equivariance** — reordering channels is legal
//!   only when labels travel with the values. Bare values in a new order are
//!   rejected, because TP9/AF7/AF8/TP10 are anatomy, not slots.
//!
//! Invariance and non-invariance live with the identities they govern
//! (`runtime_target`, `conformance`, `model_pack`); the tests pin them.

use serde::{Deserialize, Serialize};

const INDEX_KEY_DOMAIN: &str = "neuralcompose.index-entry.v1";

// ---------- idempotent indexing ----------

/// One indexed record: *what* was embedded and *by which embedding space*.
/// Two records with the same pair are the same entry; two with different
/// embedding identities are never the same entry, however similar the text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct IndexEntryKey {
    pub content_sha256_hex: String,
    pub embedding_space_identity: String,
}

/// Canonical index-entry identity.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn index_entry_identity(key: IndexEntryKey) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Doc {
        domain: &'static str,
        key: IndexEntryKey,
    }
    let doc = Doc {
        domain: INDEX_KEY_DOMAIN,
        key,
    };
    crate::audio::sha256_hex(serde_json::to_vec(&doc).expect("serialize"))
}

/// Collapse repeated indexing of identical content into one entry, keeping
/// first-seen order. Idempotent by construction: feeding the output back in
/// is a fixed point.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn dedupe_index_entries(keys: Vec<IndexEntryKey>) -> Vec<IndexEntryKey> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for k in keys {
        if seen.insert(index_entry_identity(k.clone())) {
            out.push(k);
        }
    }
    out
}

/// Do these two entries belong in the same index? Only when the embedding
/// space is identical — mixing spaces silently poisons retrieval.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn shares_index(a: IndexEntryKey, b: IndexEntryKey) -> bool {
    a.embedding_space_identity == b.embedding_space_identity
}

// ---------- window-shift equivariance ----------

/// A half-open sample range `[start, end)` within one recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SampleWindow {
    pub start_sample: u64,
    pub end_sample: u64,
}

/// A detected span: *where* it is, plus *what* it is and under which frozen
/// detector parameters. Shifting must move the range and nothing else.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct EventSpan {
    pub event_kind: String,
    pub window: SampleWindow,
    pub detector_parameters_digest: String,
}

/// Shift a span by `delta` samples. `None` on overflow — a silently wrapped
/// index would be a fabricated location.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn shift_event_span(span: EventSpan, delta: i64) -> Option<EventSpan> {
    let shift = |v: u64| -> Option<u64> {
        if delta >= 0 {
            v.checked_add(delta as u64)
        } else {
            v.checked_sub(delta.unsigned_abs())
        }
    };
    Some(EventSpan {
        window: SampleWindow {
            start_sample: shift(span.window.start_sample)?,
            end_sample: shift(span.window.end_sample)?,
        },
        ..span
    })
}

/// Does `shifted` equal `original` moved by exactly `delta`, with kind and
/// detector parameters untouched? The equivariance predicate itself.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn is_window_shift_equivariant(original: EventSpan, shifted: EventSpan, delta: i64) -> bool {
    shift_event_span(original, delta).is_some_and(|expected| expected == shifted)
}

// ---------- channel-permutation equivariance ----------

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ChannelPermutationError {
    /// Values arrived without a matching label set — the caller is asking us
    /// to guess anatomy from position. Refused.
    LabelsMissing,
    LengthMismatch,
    DuplicateLabel {
        label: String,
    },
    UnknownLabel {
        label: String,
    },
    /// The supplied labels are not a permutation of the canonical montage.
    NotAPermutation,
}

/// Outcome of a labelled reorder. An enum rather than `Result` so the shape
/// crosses the FFI boundary unchanged (same convention as `RestoreResult`).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ChannelOrderResult {
    Ordered { values: Vec<f64> },
    Rejected { error: ChannelPermutationError },
}

/// Reorder `values`, labelled by `from_labels`, into the canonical
/// TP9/AF7/AF8/TP10 order. Equivariance in the honest direction: permute the
/// labels with the values and the output permutes correspondingly.
///
/// Values without labels are rejected (`LabelsMissing`) — this function is
/// deliberately unable to reorder anonymous channels.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn to_canonical_channel_order(
    values: Vec<f64>,
    from_labels: Vec<String>,
) -> ChannelOrderResult {
    use crate::types::{CHANNEL_COUNT, CHANNEL_ORDER};

    let rejected = |error| ChannelOrderResult::Rejected { error };

    if from_labels.is_empty() {
        return rejected(ChannelPermutationError::LabelsMissing);
    }
    if values.len() != CHANNEL_COUNT || from_labels.len() != CHANNEL_COUNT {
        return rejected(ChannelPermutationError::LengthMismatch);
    }
    let mut seen = std::collections::HashSet::new();
    for label in &from_labels {
        if !seen.insert(label.clone()) {
            return rejected(ChannelPermutationError::DuplicateLabel {
                label: label.clone(),
            });
        }
        if !CHANNEL_ORDER.contains(&label.as_str()) {
            return rejected(ChannelPermutationError::UnknownLabel {
                label: label.clone(),
            });
        }
    }
    let mut out = Vec::with_capacity(CHANNEL_COUNT);
    for canonical in CHANNEL_ORDER {
        match from_labels.iter().position(|l| l == canonical) {
            Some(idx) => out.push(values[idx]),
            None => return rejected(ChannelPermutationError::NotAPermutation),
        }
    }
    ChannelOrderResult::Ordered { values: out }
}

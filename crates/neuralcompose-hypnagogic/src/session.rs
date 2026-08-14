//! What was true of the machine and the radio while a session was recorded.
//!
//! Every field here has already cost a session or a wrong conclusion. The
//! headband disconnects differently on battery than on AC; the governor decides
//! whether a turn takes two seconds or nine; an rfkill'd adapter looks exactly
//! like a headband that is out of range. None of it is recoverable after the
//! fact, and all of it is cheap to record at the time.
//!
//! ## Two assertion kinds, and the line between them
//!
//! What this process **reads** — `/sys/class/power_supply`, `cpufreq`,
//! `/sys/class/bluetooth` — is [`AssertionKind::Observed`]. The kernel is the
//! observer and the file is the observation.
//!
//! What the **operator types** — the board id, the BrainFlow preset — is
//! [`AssertionKind::ExternalClaim`]. This binary cannot see either one: the
//! bridge sends `{timestamp, channels}` and never announces the board it opened
//! (`tools/muse-ble-bridge/bridge.py`). Defaulting them to "Muse S" because
//! that is what is usually plugged in would be inventing a measurement, which
//! is the exact failure the provenance vocabulary exists to prevent. Absent by
//! default; an `ExternalClaim` when supplied.
//!
//! ## Absent is absent
//!
//! Every field is an `Option` and every absent one is `null`. A missing
//! governor is not `"unknown"`, a missing capacity is not `0`, and an
//! unreadable rfkill state is not `false`. Five instances of that defect are on
//! record in this project; documenting the rule did not prevent them and the
//! type did.

use neuralcompose_mobile_core::provenance::{
    present_option, AssertionKind, MethodIdentity, ProvenanceEnvelope, PROVENANCE_ENVELOPE_SCHEMA,
};
use serde::{Deserialize, Serialize};

pub const SESSION_RECORD_SCHEMA: &str = "neuralcompose.hypnagogic.session.v1";

/// Power and thermal state, read from `/sys`.
///
/// Field shape follows `tools/spoken-loop/dialectic-relay/relay.py`'s
/// `power_state()`, which is the only structured host-state capture already in
/// this repository. Reusing its shape rather than inventing a second one means
/// the two are comparable.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerState {
    /// `true` when a mains supply reports `online`. `None` when no supply of
    /// type `Mains` was readable — which is different from being on battery.
    #[serde(deserialize_with = "present_option")]
    pub on_ac: Option<bool>,
    #[serde(deserialize_with = "present_option")]
    pub battery_status: Option<String>,
    #[serde(deserialize_with = "present_option")]
    pub battery_capacity: Option<u8>,
    #[serde(deserialize_with = "present_option")]
    pub scaling_governor: Option<String>,
    #[serde(deserialize_with = "present_option")]
    pub scaling_driver: Option<String>,
    #[serde(deserialize_with = "present_option")]
    pub platform_profile: Option<String>,
}

/// The Bluetooth adapter the bridge is presumably using.
///
/// "Presumably" is load-bearing: this records the state of the adapters this
/// machine has, not proof that the bridge opened any particular one. The bridge
/// runs in another process and does not report which adapter it bound.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HciAdapter {
    pub name: String,
    /// The device the adapter hangs off, resolved from
    /// `/sys/class/bluetooth/hciN/device` — e.g. a USB path. This is what
    /// distinguishes the built-in radio from a dongle, which is the question
    /// worth answering later.
    ///
    /// **There is deliberately no `address` field.** The BD address is not in
    /// sysfs; it is only reachable over the management socket. A field that
    /// could never be filled would serialize as `null` on every session and
    /// read as "we looked and the adapter had no address" rather than "this
    /// process cannot see one" — the same absent-versus-unmeasured confusion
    /// this record exists to avoid. The first version of this struct had that
    /// field and it was `null` in every run.
    #[serde(deserialize_with = "present_option")]
    pub device_path: Option<String>,
    /// rfkill soft block. `None` means the state could not be read, never
    /// "not blocked".
    #[serde(deserialize_with = "present_option")]
    pub soft_blocked: Option<bool>,
    #[serde(deserialize_with = "present_option")]
    pub hard_blocked: Option<bool>,
}

/// What the operator said was on the other end of the wire.
///
/// [`AssertionKind::ExternalClaim`]: nothing in this process verified any of
/// it. A claim recorded as a claim is useful; the same claim recorded as an
/// observation is a lie with a provenance envelope on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedSource {
    /// e.g. `"muse-s-board-39"`. Free text on purpose — this is a claim, and
    /// constraining it to an enum would imply validation that did not happen.
    #[serde(deserialize_with = "present_option")]
    pub board_id: Option<String>,
    /// BrainFlow preset. `bridge.py` passes none, so the default preset is
    /// implied — but "implied" is not "recorded", and the operator has to say.
    #[serde(deserialize_with = "present_option")]
    pub preset: Option<String>,
    pub provenance: ProvenanceEnvelope,
}

/// One session's host and radio context, written beside the turn log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub schema_id: String,
    pub session_id: String,
    pub mode: String,
    /// The capture this session wrote, when it wrote one.
    #[serde(deserialize_with = "present_option")]
    pub recording_id: Option<String>,
    /// The EEG source URL, when one was attached.
    #[serde(deserialize_with = "present_option")]
    pub eeg_url: Option<String>,
    /// Backend the embedder actually reported, read back rather than assumed.
    #[serde(deserialize_with = "present_option")]
    pub embedder_backend_id: Option<String>,
    pub power: PowerState,
    pub hci_adapters: Vec<HciAdapter>,
    /// Envelope over `power` and `hci_adapters` — the parts this process read
    /// from the kernel.
    pub host_provenance: ProvenanceEnvelope,
    /// `None` when the operator claimed nothing.
    #[serde(deserialize_with = "present_option")]
    pub claimed_source: Option<ClaimedSource>,
}

/// Envelope for the `/sys` reads.
///
/// `method: None` is deliberate and permitted:
/// [`AssertionKind::Observed`] is not in `requires_method`, because an
/// observation is a reading rather than a computation. There is no window
/// length, no filter and no threshold to seal — the value is whatever the
/// kernel said at the moment it was asked.
pub fn host_envelope() -> ProvenanceEnvelope {
    ProvenanceEnvelope {
        schema_id: PROVENANCE_ENVELOPE_SCHEMA.to_string(),
        assertion_kind: AssertionKind::Observed,
        method: None,
        inputs: Vec::new(),
        confidence: None,
        comparison_embedding_space: None,
    }
}

/// Envelope for operator-supplied source identity.
///
/// `confidence` stays `None` even though `ExternalClaim` permits one: this
/// process has no basis for scoring how likely the operator is to be right, and
/// a number invented here would be indistinguishable from a measured one.
pub fn claim_envelope(method: MethodIdentity) -> ProvenanceEnvelope {
    ProvenanceEnvelope {
        schema_id: PROVENANCE_ENVELOPE_SCHEMA.to_string(),
        assertion_kind: AssertionKind::ExternalClaim,
        method: Some(method),
        inputs: Vec::new(),
        confidence: None,
        comparison_embedding_space: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neuralcompose_mobile_core::provenance::{evidence_mapping, validate, EvidenceMapping};

    fn method() -> MethodIdentity {
        MethodIdentity {
            method_id: "test".into(),
            software_id: "test".into(),
            software_version: "0".into(),
            git_commit: None,
            parameters_digest: "0".repeat(64),
        }
    }

    #[test]
    fn a_host_reading_is_observed_and_carries_no_method() {
        let e = host_envelope();
        assert_eq!(e.assertion_kind, AssertionKind::Observed);
        assert_eq!(e.method, None);
        assert!(validate(&e).is_empty(), "{:?}", validate(&e));
    }

    /// The operator's word is a claim, and a claim is ingestible as a claim —
    /// which is the point of having the kind at all. What must never happen is
    /// it being recorded as `Observed`.
    #[test]
    fn an_operator_claim_is_an_external_claim_not_an_observation() {
        let e = claim_envelope(method());
        assert_eq!(e.assertion_kind, AssertionKind::ExternalClaim);
        assert_ne!(e.assertion_kind, AssertionKind::Observed);
        assert!(validate(&e).is_empty(), "{:?}", validate(&e));
        assert_eq!(
            evidence_mapping(e.assertion_kind),
            EvidenceMapping::Ingestible("externalClaim")
        );
    }

    /// Absent fields must survive a round trip as `null`, not vanish and not
    /// become a default. `PowerState::default()` is all-`None`, so a record
    /// that read nothing is still a record that says it read nothing.
    #[test]
    fn an_unreadable_host_state_round_trips_as_null_not_as_a_default_value() {
        let p = PowerState::default();
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"onAc\":null"), "{json}");
        assert!(json.contains("\"scalingGovernor\":null"), "{json}");
        assert!(
            !json.contains("false"),
            "an absent flag became false: {json}"
        );
        assert!(!json.contains(":0"), "an absent number became zero: {json}");
        let back: PowerState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    /// A missing key is not the same fact as a recorded `null`, and
    /// `present_option` is what keeps them apart. Without it, serde's default
    /// would silently accept a producer that never wrote the field at all.
    #[test]
    fn a_missing_field_is_rejected_rather_than_defaulted_to_absent() {
        let full = serde_json::to_string(&PowerState::default()).unwrap();
        assert!(serde_json::from_str::<PowerState>(&full).is_ok());
        assert!(
            serde_json::from_str::<PowerState>("{}").is_err(),
            "an empty object parsed as a fully-absent reading"
        );
    }
}

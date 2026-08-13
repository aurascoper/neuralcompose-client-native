// Golden contract for the provenance envelope (ADR-004).
//
// A JSON Schema plus a fixture that validates against it is a test that passes
// by construction. Every test here is chosen for what it *breaks on*: the
// mutation table in the ADR names the specific edit each one kills. If you add
// a test, say what wrong version of the code it fails against — and if you
// cannot, it is documentation, not a check.

use neuralcompose_mobile_core::provenance::{
    comparable, evidence_mapping, validate, AssertionKind, EvidenceMapping, MethodIdentity,
    ProvenanceDefect, ProvenanceEnvelope, PROVENANCE_ENVELOPE_SCHEMA,
};
use serde_json::{json, Value};

fn read(rel: &str) -> Value {
    let p = format!(
        "{}/../../contracts/provenance/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}")))
        .unwrap()
}

/// One self-contained schema with internal `$defs` — no cross-file `$ref`, so
/// none of the base-URI registration `m7a2_contracts_golden.rs` needed.
fn validator() -> jsonschema::Validator {
    jsonschema::validator_for(&read("provenance-envelope.schema.json")).expect("schema builds")
}

fn base(name: &str) -> Value {
    match name {
        "derived" => read("fixtures/valid-derived-envelope.json"),
        "heuristic" => read("fixtures/valid-heuristic-envelope.json"),
        other => panic!("unknown base fixture: {other}"),
    }
}

/// Replace the value at `pointer`, or insert it when only the parent exists.
/// Insertion is how the misspelled-field case is expressed.
fn set_at(doc: &mut Value, pointer: &str, value: Value) {
    if let Some(slot) = doc.pointer_mut(pointer) {
        *slot = value;
        return;
    }
    let (parent, key) = pointer.rsplit_once('/').expect("pointer has a parent");
    doc.pointer_mut(parent)
        .unwrap_or_else(|| panic!("no parent at {parent:?}"))
        .as_object_mut()
        .unwrap_or_else(|| panic!("parent at {parent:?} is not an object"))
        .insert(key.to_string(), value);
}

/// The wire spelling of a defect, matching the `defect` column in
/// `invalid-cases.json`.
fn defect_label(d: &ProvenanceDefect) -> String {
    match d {
        ProvenanceDefect::SchemaIdMismatch => "SchemaIdMismatch".into(),
        ProvenanceDefect::MethodRequired => "MethodRequired".into(),
        ProvenanceDefect::DerivationInputsRequired => "DerivationInputsRequired".into(),
        ProvenanceDefect::ConfidenceNotApplicable => "ConfidenceNotApplicable".into(),
        ProvenanceDefect::ConfidenceOutOfRange => "ConfidenceOutOfRange".into(),
        ProvenanceDefect::EmptyField(f) => format!("EmptyField:{f}"),
        ProvenanceDefect::MalformedDigest(f) => format!("MalformedDigest:{f}"),
    }
}

fn method() -> MethodIdentity {
    MethodIdentity {
        method_id: "neuralcompose.test.v1".into(),
        software_id: "neuralcompose-mobile-core".into(),
        software_version: "0.1.0".into(),
        git_commit: None,
        parameters_digest: "a".repeat(64),
    }
}

fn envelope(kind: AssertionKind) -> ProvenanceEnvelope {
    ProvenanceEnvelope::new(kind, method())
}

// 1 -----------------------------------------------------------------------

/// The enum's serde spellings ARE the wire format, so they are asserted as
/// literals rather than round-tripped. A round-trip test passes happily after a
/// rename; this one does not.
#[test]
fn serde_spellings_are_the_wire() {
    let expected = [
        (AssertionKind::Observed, "observed"),
        (
            AssertionKind::DerivedDeterministically,
            "derivedDeterministically",
        ),
        (AssertionKind::HumanDecision, "humanDecision"),
        (AssertionKind::AgentInference, "agentInference"),
        (AssertionKind::ExternalClaim, "externalClaim"),
        (AssertionKind::HeuristicAnnotation, "heuristicAnnotation"),
    ];
    assert_eq!(expected.len(), AssertionKind::ALL.len(), "a kind was added");
    for (kind, wire) in expected {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(wire));
        let back: AssertionKind = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(back, kind);
    }
}

// 2 -----------------------------------------------------------------------

/// The ingestible targets must be exactly the five names neural-memory-server
/// defines — set equality in BOTH directions, so neither a sixth mapping nor a
/// dropped one survives.
///
/// This is the cross-repo seam and its ceiling is real: the list comes from a
/// committed fixture, not from the other repository, because `cargo test` here
/// cannot reach it. `scripts/check-evidence-class-drift.sh` is what re-reads
/// the source; this test is what stops the mapping drifting from the last
/// reading.
#[test]
fn mapping_targets_are_exactly_the_five_upstream_names() {
    let record = read("fixtures/evidence-class-names.json");
    let upstream: std::collections::BTreeSet<String> = record["upstream"]["evidenceClasses"]
        .as_array()
        .expect("upstream.evidenceClasses")
        .iter()
        .map(|v| v.as_str().expect("string").to_string())
        .collect();
    assert_eq!(upstream.len(), 5, "the drift record itself changed shape");

    let mapped: std::collections::BTreeSet<String> = AssertionKind::ALL
        .iter()
        .filter_map(|k| match evidence_mapping(*k) {
            EvidenceMapping::Ingestible(name) => Some(name.to_string()),
            EvidenceMapping::NeverIngestible => None,
        })
        .collect();

    assert_eq!(
        mapped, upstream,
        "the mapping and the recorded upstream class list disagree"
    );
}

/// The fixture must keep a chain back to the source it mirrors. Without the
/// commit and the file digest, "these are the five names" is an assertion with
/// no way to check it short of a full checkout.
#[test]
fn the_drift_record_pins_its_own_source() {
    let up = &read("fixtures/evidence-class-names.json")["upstream"];
    for field in ["repo", "path", "lines", "readOn"] {
        assert!(
            up[field].as_str().is_some_and(|s| !s.trim().is_empty()),
            "upstream.{field} is missing"
        );
    }
    for (field, len) in [("commit", 40), ("fileSha256", 64)] {
        let v = up[field].as_str().unwrap_or_default();
        assert!(
            v.len() == len
                && v.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "upstream.{field} is not {len} lowercase hex: {v:?}"
        );
    }
}

/// Local extensions are kept apart from mirrored names on purpose: otherwise an
/// upstream addition and one of ours look identical to the drift check, and the
/// only way to tell them apart is a hardcoded exclusion list — which is its own
/// place to drift.
///
/// So the split is asserted structurally: every `NeverIngestible` kind is
/// declared local, every local kind is `NeverIngestible`, and the two sets never
/// overlap.
#[test]
fn local_extensions_are_disjoint_from_the_mirrored_names() {
    let record = read("fixtures/evidence-class-names.json");
    let upstream: std::collections::BTreeSet<String> = record["upstream"]["evidenceClasses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let local: std::collections::BTreeSet<String> = record["localExtensions"]["assertionKinds"]
        .as_array()
        .expect("localExtensions.assertionKinds")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert!(
        upstream.is_disjoint(&local),
        "a name is claimed as both mirrored and local: {:?}",
        upstream.intersection(&local).collect::<Vec<_>>()
    );

    let never: std::collections::BTreeSet<String> = AssertionKind::ALL
        .iter()
        .filter(|k| evidence_mapping(**k) == EvidenceMapping::NeverIngestible)
        .map(|k| {
            serde_json::to_value(k)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        never, local,
        "the kinds with no evidence class and the kinds declared local must be the same set"
    );
}

/// "Four of the five mappings have no live ingestion path" is a fact about
/// another repository, and stating it only in ADR prose lets it rot. Recorded as
/// data instead: exactly one class is reachable through the agent write door
/// (`write.rs:266-273` clamps before the digest), so wiring a second without
/// updating the record fails here rather than quietly making the prose false.
#[test]
fn exactly_one_evidence_class_is_reachable_through_the_agent_door() {
    let record = read("fixtures/evidence-class-names.json");
    let upstream: std::collections::BTreeSet<String> = record["upstream"]["evidenceClasses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let writable: Vec<String> = record["agentWritableClasses"]["classes"]
        .as_array()
        .expect("agentWritableClasses.classes")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert_eq!(
        writable,
        vec!["agentInference".to_string()],
        "a second ingestion path was recorded; ADR-004's \"vocabulary alignment, \
         not a pipe\" paragraph now needs rewriting"
    );
    assert!(
        writable.iter().all(|c| upstream.contains(c)),
        "a writable class is not one of the mirrored evidence classes"
    );
    assert_eq!(
        upstream.len() - writable.len(),
        4,
        "the ADR says four mappings have no live ingestion path"
    );
}

/// The `requires_method -> true` survivor generalizes: every kind permitted to
/// omit an optional field needs a case that actually omits it. This is the
/// kind × optional-field matrix, so the gap cannot reopen one field at a time.
#[test]
fn every_kind_is_exercised_with_each_optional_field_absent() {
    /// A field's wire name and the edit that makes it absent.
    type Absence = (&'static str, fn(&mut ProvenanceEnvelope));

    let optional: [Absence; 4] = [
        ("method", |e| e.method = None),
        ("confidence", |e| e.confidence = None),
        ("comparisonEmbeddingSpace", |e| {
            e.comparison_embedding_space = None
        }),
        ("locator", |e| {
            for i in e.inputs.iter_mut() {
                i.locator = None;
            }
        }),
    ];
    let method_optional = [
        AssertionKind::Observed,
        AssertionKind::HumanDecision,
        AssertionKind::ExternalClaim,
    ];

    for kind in AssertionKind::ALL {
        for (field, clear) in &optional {
            let mut env = envelope(kind);
            if kind == AssertionKind::DerivedDeterministically {
                env.inputs = base_inputs();
            }
            clear(&mut env);
            let defects = validate(&env);
            let expected_ok = *field != "method" || method_optional.contains(&kind);
            assert_eq!(
                defects.is_empty(),
                expected_ok,
                "{kind:?} with {field} absent produced {defects:?}"
            );
        }
    }
}

// 3 -----------------------------------------------------------------------

/// Both polarities at once: mapping the heuristic kind onto an evidence class
/// fails here, and so does returning `NeverIngestible` for everything.
#[test]
fn heuristic_annotation_is_the_only_never_ingestible() {
    for kind in AssertionKind::ALL {
        let mapping = evidence_mapping(kind);
        let never = mapping == EvidenceMapping::NeverIngestible;
        assert_eq!(
            never,
            kind == AssertionKind::HeuristicAnnotation,
            "{kind:?} mapped to {mapping:?}"
        );
    }
}

// 4 -----------------------------------------------------------------------

/// The highest-value assertion in this file. `a == b` satisfies every case
/// below except `(None, None)` — and `(None, None)` is precisely the situation
/// where nothing is known about either space, which must never read as a match.
#[test]
fn comparable_refuses_two_absences() {
    let space = |s: Option<&str>| {
        let mut e = envelope(AssertionKind::AgentInference);
        e.comparison_embedding_space = s.map(str::to_string);
        e
    };
    let a = space(Some("space-a"));
    let b = space(Some("space-b"));
    let unknown = space(None);

    assert!(comparable(&a, &a), "one named space matches itself");
    assert!(!comparable(&a, &b), "different spaces are incomparable");
    assert!(!comparable(&a, &unknown));
    assert!(!comparable(&unknown, &a));
    assert!(
        !comparable(&unknown, &unknown),
        "two unknown spaces are not one shared space"
    );
}

// 5 -----------------------------------------------------------------------

/// Every field is required to be PRESENT, though several may be null. A
/// `#[serde(default)]` anywhere would turn a missing field into a confident
/// default, which is the defect this vocabulary exists to prevent.
#[test]
fn every_required_field_is_rejected_when_missing() {
    let pointers = [
        ("derived", "/schemaId"),
        ("derived", "/assertionKind"),
        ("derived", "/method"),
        ("derived", "/inputs"),
        ("derived", "/confidence"),
        ("derived", "/comparisonEmbeddingSpace"),
        ("derived", "/method/methodId"),
        ("derived", "/method/softwareId"),
        ("derived", "/method/softwareVersion"),
        ("derived", "/method/gitCommit"),
        ("derived", "/method/parametersDigest"),
        ("derived", "/inputs/0/resourceKind"),
        ("derived", "/inputs/0/sha256Hex"),
        ("derived", "/inputs/0/locator"),
    ];
    for (base_name, pointer) in pointers {
        let mut doc = base(base_name);
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        doc.pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key)
            .unwrap_or_else(|| panic!("{pointer} was not there to remove"));
        assert!(
            serde_json::from_value::<ProvenanceEnvelope>(doc).is_err(),
            "{pointer} was accepted while missing"
        );
    }
}

// 6 -----------------------------------------------------------------------

/// An unscored claim serializes as `null`, never as `0` — a zero confidence is
/// a real score meaning "certainly not", and the two must stay distinguishable
/// on the wire.
#[test]
fn absent_confidence_serializes_as_null_never_zero() {
    let env = envelope(AssertionKind::AgentInference);
    assert!(env.confidence.is_none());
    let v = serde_json::to_value(&env).unwrap();
    assert_eq!(v["confidence"], Value::Null);
    assert!(v.as_object().unwrap().contains_key("confidence"));

    // And a real zero survives as a number.
    let mut scored = envelope(AssertionKind::AgentInference);
    scored.confidence = Some(0.0);
    assert_eq!(
        serde_json::to_value(&scored).unwrap()["confidence"],
        json!(0.0)
    );
}

// 7 -----------------------------------------------------------------------

/// Each invalid case fails for exactly ONE named reason, and the schema's
/// verdict is asserted in both directions so the marker table cannot drift.
#[test]
fn invalid_cases_fail_for_exactly_one_named_reason() {
    let table = read("fixtures/invalid-cases.json");
    let cases = table["cases"].as_array().expect("cases");
    assert!(cases.len() >= 12, "the invalid corpus is too small");
    let v = validator();

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut doc = base(case["base"].as_str().unwrap());
        set_at(
            &mut doc,
            case["pointer"].as_str().unwrap(),
            case["value"].clone(),
        );

        let expected_schema_invalid = case["schemaInvalid"].as_bool().unwrap();
        assert_eq!(
            !v.is_valid(&doc),
            expected_schema_invalid,
            "{name}: schemaInvalid marker disagrees with the schema"
        );

        let expected = case["defect"].as_str().unwrap();
        match serde_json::from_value::<ProvenanceEnvelope>(doc) {
            Err(e) => assert_eq!(
                expected, "SerdeRejects",
                "{name}: expected {expected} but serde rejected it: {e}"
            ),
            Ok(typed) => {
                let got: Vec<String> = validate(&typed).iter().map(defect_label).collect();
                assert_eq!(
                    got,
                    vec![expected.to_string()],
                    "{name}: expected exactly one defect"
                );
            }
        }
    }
}

// 8 -----------------------------------------------------------------------

/// Confidence never changes what a claim IS. The mapping is invariant under
/// every score, and the only thing a score can do is be rejected as
/// inapplicable — it can never move a kind up the ladder.
#[test]
fn confidence_never_promotes() {
    for kind in AssertionKind::ALL {
        let baseline = evidence_mapping(kind);
        for c in [None, Some(0.0), Some(0.5), Some(1.0)] {
            let mut env = envelope(kind);
            env.confidence = c;
            if kind == AssertionKind::DerivedDeterministically {
                env.inputs = base_inputs();
            }
            assert_eq!(
                evidence_mapping(kind),
                baseline,
                "{kind:?} changed class under confidence {c:?}"
            );
            let defects = validate(&env);
            let only_confidence = defects
                .iter()
                .all(|d| *d == ProvenanceDefect::ConfidenceNotApplicable);
            assert!(
                only_confidence,
                "{kind:?} at {c:?} produced unrelated defects: {defects:?}"
            );
        }
    }
}

fn base_inputs() -> Vec<neuralcompose_mobile_core::provenance::ResourceRef> {
    vec![neuralcompose_mobile_core::provenance::ResourceRef {
        resource_kind: "sentence-embedding".into(),
        sha256_hex: "b".repeat(64),
        locator: None,
    }]
}

// 9 -----------------------------------------------------------------------

/// Both directions of the derivation rule. The negative half matters most: a
/// validator that demanded inputs of EVERY kind would pass a one-sided test
/// while making every heuristic annotation invalid.
#[test]
fn derivation_requires_method_and_inputs_both_ways() {
    let mut derived = envelope(AssertionKind::DerivedDeterministically);
    derived.inputs = base_inputs();
    assert_eq!(validate(&derived), [], "a complete derivation is valid");

    let mut no_inputs = derived.clone();
    no_inputs.inputs.clear();
    assert_eq!(
        validate(&no_inputs),
        [ProvenanceDefect::DerivationInputsRequired]
    );

    let mut no_method = derived.clone();
    no_method.method = None;
    assert_eq!(validate(&no_method), [ProvenanceDefect::MethodRequired]);

    // The negative half: empty inputs are fine for every other kind.
    for kind in AssertionKind::ALL {
        if kind == AssertionKind::DerivedDeterministically {
            continue;
        }
        let env = envelope(kind);
        assert!(
            env.inputs.is_empty() && validate(&env).is_empty(),
            "{kind:?} was rejected for naming no inputs"
        );
    }
}

/// The other negative half, and it was missing: three kinds are valid with NO
/// method at all. An observation ingested through an operator channel names a
/// source artifact rather than a procedure, and a human decision is a decision.
///
/// Added because `requires_method -> true` survived the mutation run: every
/// envelope the other tests build already carries a method, so nothing
/// exercised the kinds allowed to omit one. The suite looked complete and was
/// not.
#[test]
fn three_kinds_are_valid_without_any_method() {
    let optional = [
        AssertionKind::Observed,
        AssertionKind::HumanDecision,
        AssertionKind::ExternalClaim,
    ];
    for kind in AssertionKind::ALL {
        let mut env = envelope(kind);
        env.method = None;
        if kind == AssertionKind::DerivedDeterministically {
            env.inputs = base_inputs();
        }
        let defects = validate(&env);
        if optional.contains(&kind) {
            assert_eq!(defects, [], "{kind:?} must not need a method");
        } else {
            assert_eq!(
                defects,
                [ProvenanceDefect::MethodRequired],
                "{kind:?} must name the software that produced it"
            );
        }
    }
}

// 10 ----------------------------------------------------------------------

#[test]
fn valid_fixtures_validate_and_round_trip() {
    let v = validator();
    for name in ["derived", "heuristic"] {
        let doc = base(name);
        let errs: Vec<String> = v.iter_errors(&doc).map(|e| e.to_string()).collect();
        assert!(errs.is_empty(), "{name}: {errs:?}");

        let typed: ProvenanceEnvelope = serde_json::from_value(doc.clone()).expect("typed");
        assert_eq!(typed.schema_id, PROVENANCE_ENVELOPE_SCHEMA);
        assert_eq!(validate(&typed), [], "{name} has defects");
        assert_eq!(serde_json::to_value(&typed).unwrap(), doc, "{name} drifted");
    }
}

/// The heuristic fixture pins the INCLUSIVE upper bound from the passing side:
/// its confidence is exactly 1.0, so tightening `<=` to `<` turns a valid
/// fixture invalid instead of quietly surviving.
#[test]
fn the_heuristic_fixture_sits_on_the_confidence_boundary() {
    let doc = base("heuristic");
    assert_eq!(
        doc["confidence"],
        json!(1.0),
        "this fixture exists to sit on the bound; move it and the boundary is unpinned"
    );
}

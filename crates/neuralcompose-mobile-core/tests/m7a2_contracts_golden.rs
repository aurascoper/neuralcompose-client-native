// M7-A2 golden contracts: every runtime fixture validates against its JSON
// Schema and round-trips through the typed Rust structs, and every invalid
// case is rejected by Rust — with the schema's verdict asserted in BOTH
// directions so the marker table cannot drift unnoticed.

use neuralcompose_mobile_core::conformance::{
    validate_conformance_policy, BackendConformancePolicy,
};
use neuralcompose_mobile_core::runtime_target::{
    validate_model_variant, validate_runtime_pack_manifest, ModelVariant, RuntimePackManifest,
};
use serde_json::Value;

fn read(rel: &str) -> Value {
    let p = format!(
        "{}/../../contracts/runtime/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}")))
        .unwrap()
}

/// The runtime schemas `$ref` a sibling file, so resolve relative refs from
/// the contracts directory rather than the process CWD.
fn validator_for(schema_rel: &str) -> jsonschema::Validator {
    let dir = std::fs::canonicalize(format!(
        "{}/../../contracts/runtime",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("contracts dir");
    // Directory base URI (trailing slash) so sibling `$ref`s resolve against
    // the contracts directory, not the process CWD. No url crate needed.
    let base = format!("file://{}/", dir.display());
    let schema = read(schema_rel);
    jsonschema::options()
        .with_base_uri(base)
        .build(&schema)
        .unwrap_or_else(|e| panic!("{schema_rel}: {e}"))
}

fn schema_accepts(schema_rel: &str, v: &Value) -> bool {
    validator_for(schema_rel).is_valid(v)
}

fn assert_valid(schema_rel: &str, v: &Value) {
    let validator = validator_for(schema_rel);
    let errs: Vec<String> = validator.iter_errors(v).map(|e| e.to_string()).collect();
    assert!(errs.is_empty(), "{schema_rel}: {errs:?}");
}

#[test]
fn runtime_pack_fixture_validates_and_round_trips() {
    let v = read("fixtures/valid-runtime-pack-vulkan.json");
    assert_valid("runtime-pack-manifest.schema.json", &v);
    let typed: RuntimePackManifest = serde_json::from_value(v.clone()).expect("typed");
    assert!(
        validate_runtime_pack_manifest(typed.clone()).is_empty(),
        "{:?}",
        validate_runtime_pack_manifest(typed.clone())
    );
    assert_eq!(serde_json::to_value(&typed).unwrap(), v);
}

#[test]
fn model_variant_fixture_validates_and_round_trips() {
    let v = read("fixtures/valid-model-variant-cpu.json");
    assert_valid("model-variant.schema.json", &v);
    let typed: ModelVariant = serde_json::from_value(v.clone()).expect("typed");
    assert!(validate_model_variant(typed.clone()).is_empty());
    assert_eq!(serde_json::to_value(&typed).unwrap(), v);
}

#[test]
fn conformance_policy_fixture_validates_and_round_trips() {
    let v = read("fixtures/valid-conformance-policy.json");
    assert_valid("backend-conformance-policy.schema.json", &v);
    let typed: BackendConformancePolicy = serde_json::from_value(v.clone()).expect("typed");
    assert!(validate_conformance_policy(typed.clone()).is_empty());
    assert_eq!(serde_json::to_value(&typed).unwrap(), v);
}

#[test]
fn invalid_cases_fail_rust_and_match_their_schema_marker() {
    let table = read("fixtures/invalid-cases.json");
    for case in table["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let base_name = case["base"].as_str().unwrap();
        let (fixture, schema_rel) = match base_name {
            "runtime-pack" => (
                "fixtures/valid-runtime-pack-vulkan.json",
                "runtime-pack-manifest.schema.json",
            ),
            "model-variant" => (
                "fixtures/valid-model-variant-cpu.json",
                "model-variant.schema.json",
            ),
            "conformance-policy" => (
                "fixtures/valid-conformance-policy.json",
                "backend-conformance-policy.schema.json",
            ),
            other => panic!("unknown base: {other}"),
        };
        let mut doc = read(fixture);
        let pointer = case["pointer"].as_str().unwrap();
        *doc.pointer_mut(pointer)
            .unwrap_or_else(|| panic!("{name}: bad pointer {pointer}")) = case["value"].clone();

        // Both directions: the marker must match the schema's actual verdict.
        let expected_schema_invalid = case["schemaInvalid"].as_bool().unwrap();
        let schema_rejects = !schema_accepts(schema_rel, &doc);
        assert_eq!(
            schema_rejects, expected_schema_invalid,
            "{name}: schemaInvalid marker disagrees with the schema"
        );

        // Rust is authoritative for semantics in every case.
        let rust_errs: Vec<String> = match base_name {
            "runtime-pack" => match serde_json::from_value::<RuntimePackManifest>(doc) {
                Ok(t) => validate_runtime_pack_manifest(t),
                Err(e) => vec![e.to_string()],
            },
            "model-variant" => match serde_json::from_value::<ModelVariant>(doc) {
                Ok(t) => validate_model_variant(t),
                Err(e) => vec![e.to_string()],
            },
            _ => match serde_json::from_value::<BackendConformancePolicy>(doc) {
                Ok(t) => validate_conformance_policy(t),
                Err(e) => vec![e.to_string()],
            },
        };
        assert!(!rust_errs.is_empty(), "{name}: Rust must reject this case");
    }
}

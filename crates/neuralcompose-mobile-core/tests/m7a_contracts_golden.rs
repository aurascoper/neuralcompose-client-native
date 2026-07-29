// M7-A golden contracts: every fixture validates against its JSON Schema and
// round-trips through the typed Rust structs.

use neuralcompose_mobile_core::model_pack::{validate_catalog_entry, ModelPackCatalogEntry};
use neuralcompose_mobile_core::provider::ProviderDescriptor;
use serde_json::Value;

fn read(rel: &str) -> Value {
    let p = format!(
        "{}/../../contracts/model-packs/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}")))
        .unwrap()
}

fn assert_valid(schema_rel: &str, v: &Value) {
    let schema = read(schema_rel);
    let validator = jsonschema::validator_for(&schema).expect(schema_rel);
    let errs: Vec<String> = validator.iter_errors(v).map(|e| e.to_string()).collect();
    assert!(errs.is_empty(), "{schema_rel}: {errs:?}");
}

#[test]
fn generation_pack_fixture_validates_and_round_trips() {
    let v = read("fixtures/valid-generation-pack.json");
    assert_valid("model-pack-catalog-entry.schema.json", &v);
    let typed: ModelPackCatalogEntry = serde_json::from_value(v.clone()).expect("typed");
    assert!(validate_catalog_entry(typed.clone()).is_empty());
    assert_eq!(serde_json::to_value(&typed).unwrap(), v);
}

#[test]
fn embedding_pack_fixture_validates_and_round_trips() {
    let v = read("fixtures/valid-embedding-pack.json");
    assert_valid("model-pack-catalog-entry.schema.json", &v);
    let typed: ModelPackCatalogEntry = serde_json::from_value(v.clone()).expect("typed");
    assert!(validate_catalog_entry(typed.clone()).is_empty());
    assert_eq!(serde_json::to_value(&typed).unwrap(), v);
}

#[test]
fn provider_fixtures_validate_and_round_trip() {
    for f in [
        "fixtures/valid-local-provider.json",
        "fixtures/valid-cloud-provider.json",
    ] {
        let v = read(f);
        assert_valid("provider-descriptor.schema.json", &v);
        let typed: ProviderDescriptor = serde_json::from_value(v.clone()).expect("typed");
        assert_eq!(serde_json::to_value(&typed).unwrap(), v, "{f}");
    }
}

#[test]
fn invalid_cases_fail_rust_validation() {
    let table = read("fixtures/invalid-cases.json");
    let base = read("fixtures/valid-generation-pack.json");
    for case in table["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut v = base.clone();
        for (path, val) in case["mutate"].as_object().unwrap() {
            let mut cur = &mut v;
            let parts: Vec<&str> = path.split('.').collect();
            for (i, part) in parts.iter().enumerate() {
                let last = i == parts.len() - 1;
                if let Ok(idx) = part.parse::<usize>() {
                    if last {
                        cur[idx] = val.clone();
                    } else {
                        cur = &mut cur[idx];
                    }
                } else if last {
                    cur[*part] = val.clone();
                } else {
                    cur = &mut cur[*part];
                }
            }
        }
        // Dual-direction schema verdict per the table's marker.
        let schema = read("model-pack-catalog-entry.schema.json");
        let validator = jsonschema::validator_for(&schema).unwrap();
        let schema_rejects = !validator.is_valid(&v);
        let marker = case["schemaInvalid"].as_bool().unwrap();
        assert_eq!(
            schema_rejects, marker,
            "case '{name}': schema verdict must match the schemaInvalid marker"
        );
        match serde_json::from_value::<ModelPackCatalogEntry>(v) {
            Err(_) => {} // failing to even deserialize also counts as rejection
            Ok(typed) => {
                assert!(
                    !validate_catalog_entry(typed).is_empty(),
                    "case '{name}' must fail validation"
                );
            }
        }
    }
}

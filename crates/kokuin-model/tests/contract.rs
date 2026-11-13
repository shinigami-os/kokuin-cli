//! Keeps the published contract files in sync with the Rust types:
//! - `schema/boot-record.schema.json` and `schema/boot-diff.schema.json`, the JSON
//!   Schemas consumers validate against;
//! - `docs/examples/boot-record.example.json`, the example record of `docs/format.md`.
//!
//! After changing the model, regenerate the schema with:
//! `KOKUIN_UPDATE_SCHEMA=1 cargo test -p kokuin-model`

use std::{fs, path::PathBuf};

use kokuin_model::{BootRecord, SCHEMA_VERSION};

fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn committed_schemas_are_up_to_date() {
    let schemas = [
        (
            "schema/boot-record.schema.json",
            kokuin_model::json_schema(),
        ),
        (
            "schema/boot-diff.schema.json",
            kokuin_model::diff_json_schema(),
        ),
    ];
    for (file, schema) in schemas {
        let path = repo_file(file);
        let generated = serde_json::to_string_pretty(&schema).unwrap() + "\n";

        if std::env::var_os("KOKUIN_UPDATE_SCHEMA").is_some() {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, generated).unwrap();
            continue;
        }

        let committed = fs::read_to_string(&path).unwrap_or_default();
        assert!(
            committed == generated,
            "{} is out of date, regenerate it with `KOKUIN_UPDATE_SCHEMA=1 cargo test -p kokuin-model`",
            path.display()
        );
    }
}

#[test]
fn example_record_matches_the_model() {
    let path = repo_file("docs/examples/boot-record.example.json");
    let text = fs::read_to_string(&path).unwrap();
    let original: serde_json::Value = serde_json::from_str(&text).unwrap();

    let record: BootRecord = serde_json::from_value(original.clone())
        .unwrap_or_else(|err| panic!("{} does not match the model: {err}", path.display()));
    assert_eq!(record.schema_version, SCHEMA_VERSION);

    // A field the model does not know would be dropped by the round trip.
    let round_trip = serde_json::to_value(&record).unwrap();
    assert_eq!(
        round_trip,
        original,
        "{} has fields the model does not know",
        path.display()
    );
}

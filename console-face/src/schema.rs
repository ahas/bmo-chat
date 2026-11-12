//! The contract's JSON Schema of a manifest, as bmo-ui's pinned release answers it, carried as a generated file
//! (`docs/console-face.md`, 「The reading through bmo-ui's release」), and the holding of a manifest to it.

use std::sync::LazyLock;

use serde_json::Value;

/// The schema as the crate carries it, written from bmo-ui's pinned release and never by hand.
pub(crate) const MANIFEST_SCHEMA: &str = include_str!("../manifest-schema.json");

static VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
  let schema: Value = serde_json::from_str(MANIFEST_SCHEMA).expect("the carried schema is JSON");
  jsonschema::validator_for(&schema).expect("the carried schema is a JSON Schema")
});

/// Each field of the manifest the schema refuses, with why; none where it keeps the schema.
pub(crate) fn faults(manifest: &Value) -> Vec<String> {
  VALIDATOR
    .iter_errors(manifest)
    .map(|error| {
      let at = error.instance_path().to_string();
      let field = if at.is_empty() { "the manifest".to_owned() } else { at };
      format!("{field}: {error}")
    })
    .collect()
}

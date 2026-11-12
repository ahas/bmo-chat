//! The schema the crate carries is the one bmo-ui's pinned release answers (`docs/console-face.md`, 「The
//! reading through bmo-ui's release」). With `BMO_CONSOLE_FACE_WRITE_SCHEMA=1` the test writes it again from that
//! release instead.

use std::path::Path;

use bmo_console_face::contract_schema;
use serde_json::Value;

use crate::support::pinned_release;

const WRITE: &str = "BMO_CONSOLE_FACE_WRITE_SCHEMA";

#[test]
fn the_carried_schema_is_the_pinned_release_s() {
  let answered = contract_schema(&pinned_release()).unwrap_or_else(|why| panic!("{why}"));
  let carried_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("manifest-schema.json");
  if std::env::var_os(WRITE).is_some() {
    let text = serde_json::to_string_pretty(&answered).expect("the schema is written as JSON");
    std::fs::write(&carried_file, format!("{text}\n")).expect("the schema file is written");
    return;
  }
  let carried: Value =
    serde_json::from_str(&std::fs::read_to_string(&carried_file).expect("the schema file is readable"))
      .expect("the schema file is JSON");
  assert!(
    carried == answered,
    "manifest-schema.json is not the pinned release's schema; {WRITE}=1 cargo test -p bmo-console-face \
     --test main schema:: writes it again"
  );
}

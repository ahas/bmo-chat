//! The face as a service builds it (`docs/console-face.md`): the manifest it answers, read through bmo-ui's own
//! reading, the files under paths that follow their bytes, the words, and what refuses the face at start.

use axum::Router;
use axum::http::StatusCode;
use bmo_console_face::{ConsoleFace, ConsoleFaceError, read_manifest};
use serde_json::{Value, json};

use crate::support::{Built, CONSOLE, LAYER, bearer, declaration, face, get, pinned_release};

const ADMINISTRATOR: (&str, &str) = ("Owner-Role", "administrator");

async fn served_manifest(router: &Router) -> Value {
  let console = bearer(CONSOLE);
  let (status, body) = get(router, "/console-manifest", &[("Authorization", &console)]).await;
  assert_eq!(status, StatusCode::OK);
  serde_json::from_slice(&body).expect("the manifest is JSON")
}

fn path<'a>(manifest: &'a Value, field: &str) -> &'a str {
  manifest[field].as_str().unwrap_or_else(|| panic!("the manifest names {field}"))
}

#[tokio::test]
async fn the_served_manifest_reads_through_the_contract_with_the_layer_and_paths_filled() {
  let built = Built::new("export default { routes: [] };");
  let router = face(json!({ "shareReads": ["/api/shares/{token}"] }), &built).router();
  let manifest = served_manifest(&router).await;

  let read = read_manifest(&manifest, &pinned_release()).unwrap_or_else(|why| panic!("{why}"));
  assert_eq!(read["layer"], LAYER);
  assert_eq!(
    read["layerUses"],
    json!({ "bmo-ui/app/domain/island": ["island"] }),
    "what the bundle uses of the layer"
  );
  assert_eq!(read["relays"], json!([]), "the reading fills the defaults");
  for field in ["bundle", "stylesheet", "words"] {
    assert!(
      path(&manifest, field).starts_with("/console/"),
      "{field} stands under the files' prefix"
    );
  }
}

#[tokio::test]
async fn a_manifest_breaking_a_rule_across_fields_is_refused_by_the_reading() {
  // A title outside the bundle's namespace keeps the schema and breaks the reading's own rule.
  let built = Built::new("export default { routes: [] };");
  let manifest = face(json!({ "title": "other.title" }), &built).manifest().clone();
  let refused = read_manifest(&manifest, &pinned_release()).expect_err("the reading refuses it");
  assert!(refused.to_string().contains("title"), "{refused}");
}

#[tokio::test]
async fn the_module_path_follows_its_bytes_and_every_file_answers_where_the_manifest_names_it() {
  let first = Built::new("export default { routes: [] };");
  let again = Built::new("export default { routes: [] };");
  let changed = Built::new("export default { routes: [{ path: '/sample' }] };");
  let first_module = path(face(json!({}), &first).manifest(), "bundle").to_owned();
  assert_eq!(path(face(json!({}), &again).manifest(), "bundle"), first_module);
  assert_ne!(path(face(json!({}), &changed).manifest(), "bundle"), first_module);

  let router = face(json!({}), &first).router();
  let manifest = served_manifest(&router).await;
  let console = bearer(CONSOLE);
  let reader = [("Authorization", console.as_str()), ADMINISTRATOR];
  for (file, holds) in [
    (path(&manifest, "bundle"), "export default"),
    (path(&manifest, "stylesheet"), ".sample"),
    (path(&manifest, "words"), "Sample"),
    ("/console/chunk-a1b2c3.js", "export const part"),
    ("/console/assets/mark-d4e5f6.svg", "<svg"),
  ] {
    let (status, body) = get(&router, file, &reader).await;
    assert_eq!(status, StatusCode::OK, "{file}");
    assert!(String::from_utf8_lossy(&body).contains(holds), "{file}");
  }
  let (status, _) = get(&router, "/console/layer.json", &reader).await;
  assert_eq!(status, StatusCode::NOT_FOUND, "the layer is the manifest's, not a file");
}

#[test]
fn a_word_is_read_by_its_locale_and_dotted_key() {
  let built = Built::new("export default { routes: [] };");
  let face = face(json!({}), &built);
  assert_eq!(face.word("en", "sample.title"), Some("Sample"));
  assert_eq!(face.word("en", "sample.missing"), None);
  assert_eq!(face.word("fr", "sample.title"), None);
}

#[test]
fn a_declaration_naming_a_field_the_face_fills_is_refused() {
  let built = Built::new("export default { routes: [] };");
  for field in ["layer", "bundle", "stylesheet", "words"] {
    let refused = ConsoleFace::new(&declaration(json!({ field: "/x" })), &built.bundle(), CONSOLE);
    assert!(
      matches!(refused, Err(ConsoleFaceError::FilledField(named)) if named == field),
      "{field}"
    );
  }
}

#[test]
fn a_declaration_the_contract_refuses_fails_the_start_naming_the_field() {
  let built = Built::new("export default { routes: [] };");
  let Err(ConsoleFaceError::Contract(faults)) =
    ConsoleFace::new(&declaration(json!({ "role": "owner", "icon": null })), &built.bundle(), CONSOLE)
  else {
    panic!("the face is refused");
  };
  let faults = faults.join("\n");
  assert!(faults.contains("/role"), "{faults}");
  assert!(faults.contains("icon"), "{faults}");
}

#[test]
fn a_bundle_without_its_layer_or_with_words_that_are_no_object_fails_the_start() {
  let without_layer = Built::new("export default { routes: [] };");
  without_layer.remove("layer.json");
  let refused = ConsoleFace::new(&declaration(json!({})), &without_layer.bundle(), CONSOLE);
  assert!(matches!(refused, Err(ConsoleFaceError::Unreadable { .. })));

  let listed_words = Built::new("export default { routes: [] };");
  listed_words.write("words.json", "[]");
  let refused = ConsoleFace::new(&declaration(json!({})), &listed_words.bundle(), CONSOLE);
  assert!(matches!(refused, Err(ConsoleFaceError::Malformed { .. })));
}

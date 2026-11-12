//! A base library depends on nothing of the bmo family, not even the other base library.

use toml::{Table, Value};

const SECTIONS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

fn manifest() -> Table {
  let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
  let text = std::fs::read_to_string(path).expect("the crate's manifest is readable");
  text.parse().expect("the crate's manifest is TOML")
}

/// Every dependency the manifest names, by the key it is written under and by the package it renames, in
/// the plain sections and in each target's own.
fn dependency_names(manifest: &Table) -> Vec<String> {
  let mut tables: Vec<&Table> =
    SECTIONS.iter().filter_map(|section| manifest.get(*section)?.as_table()).collect();
  if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
    for target in targets.values().filter_map(Value::as_table) {
      tables.extend(SECTIONS.iter().filter_map(|section| target.get(*section)?.as_table()));
    }
  }
  let mut names = Vec::new();
  for table in tables {
    for (key, spec) in table {
      names.push(key.clone());
      if let Some(package) = spec.get("package").and_then(Value::as_str) {
        names.push(package.to_owned());
      }
    }
  }
  names
}

fn of_the_family(names: Vec<String>) -> Vec<String> {
  names.into_iter().filter(|name| name.starts_with("bmo-")).collect()
}

#[test]
fn no_dependency_is_of_the_bmo_family() {
  let family = of_the_family(dependency_names(&manifest()));
  assert!(
    family.is_empty(),
    "the manifest depends on {family:?}; a base library depends on nothing bmo-*"
  );
}

#[test]
fn a_family_dependency_would_be_found() {
  let planted: Table = r#"
    [dependencies]
    serde = "1"
    [dev-dependencies]
    chat = { package = "bmo-chat-client", path = "../client" }
    [target.'cfg(unix)'.build-dependencies]
    bmo-chat = { path = "../provider" }
  "#
  .parse()
  .expect("the planted manifest is TOML");
  assert_eq!(of_the_family(dependency_names(&planted)), ["bmo-chat-client", "bmo-chat"]);
}

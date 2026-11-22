//! What the face's tests share: a bundle build's folder written into a scratch folder, the console's
//! credential, a request to a router, and bmo-ui's release this repository pins.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use bmo_console_face::{BundleFolder, ConsoleFace};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

/// The console's credential every face of the tests is built with.
pub const CONSOLE: &str = "console-credential";

/// The layer every bundle of the tests names.
pub const LAYER: &str = "0.1.0-rev.71";

/// A bundle build's folder in a scratch folder of its own, removed when it is let go of.
pub struct Built {
  pub folder: PathBuf,
}

impl Built {
  /// A folder as the bundle build writes one, its module saying `module`: the module, a stylesheet, a chunk the
  /// module loads, an asset under a folder of its own, the words and the layer.
  pub fn new(module: &str) -> Self {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let folder = std::env::temp_dir().join(format!(
      "bmo-console-face-test-{}-{}",
      std::process::id(),
      COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(folder.join("assets")).expect("the scratch folder is made");
    let built = Self { folder };
    built.write("bundle.js", module);
    built.write("bundle.css", ".sample { color: inherit; }");
    built.write("chunk-a1b2c3.js", "export const part = 1;");
    built.write("assets/mark-d4e5f6.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    built.write("words.json", &json!({ "en": { "sample": { "title": "Sample" } } }).to_string());
    built.write("layer.json", &json!({ "layer": LAYER }).to_string());
    built
  }

  pub fn write(&self, file: &str, text: &str) {
    std::fs::write(self.folder.join(file), text).expect("a file of the bundle is written");
  }

  pub fn remove(&self, file: &str) {
    std::fs::remove_file(self.folder.join(file)).expect("a file of the bundle is removed");
  }

  pub fn bundle(&self) -> BundleFolder {
    BundleFolder::new(&self.folder, "bundle.js", "words.json").with_stylesheet("bundle.css")
  }
}

impl Drop for Built {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.folder);
  }
}

/// A declaration that keeps the contract, with `changes` laid over it; a null removes a field.
pub fn declaration(changes: Value) -> String {
  let mut declared = json!({
    "name": "sample",
    "title": "sample.title",
    "icon": "i-lucide-box",
    "screenPath": "/sample",
    "serviceName": "sample",
    "apiPrefix": "/api",
    "role": "administrator",
  });
  for (field, value) in changes.as_object().expect("the changes are an object") {
    if value.is_null() {
      declared.as_object_mut().expect("the declaration is an object").remove(field);
    } else {
      declared[field] = value.clone();
    }
  }
  declared.to_string()
}

/// The face of a declaration over a folder, which the test expects to build.
pub fn face(changes: Value, built: &Built) -> ConsoleFace {
  ConsoleFace::new(&declaration(changes), &built.bundle(), CONSOLE).expect("the face is built")
}

/// A `GET` of a path with the headers given, answered by the router: the status and the body's bytes.
pub async fn get(router: &Router, path: &str, headers: &[(&str, &str)]) -> (StatusCode, Vec<u8>) {
  let mut request = Request::get(path);
  for (name, value) in headers {
    request = request.header(*name, *value);
  }
  let answer = router
    .clone()
    .oneshot(request.body(Body::empty()).expect("the request is built"))
    .await
    .expect("answered");
  let status = answer.status();
  let body = to_bytes(answer.into_body(), usize::MAX).await.expect("the body is read");
  (status, body.to_vec())
}

/// The `Authorization` header carrying a token.
pub fn bearer(token: &str) -> String {
  format!("Bearer {token}")
}

/// bmo-ui's release this repository pins, from the bmo-ui checkout beside this one, held to the pinned sha256
/// (`docs/console-face.md`, 「The release it reads」).
pub fn pinned_release() -> PathBuf {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
  let pins: toml::Table = std::fs::read_to_string(root.join("shared-versions.toml"))
    .expect("the shared versions are readable")
    .parse()
    .expect("the shared versions are TOML");
  let pin = &pins["packages"]["bmo-ui"];
  let file = pin["file"].as_str().expect("the pin names its file");
  let release = root.join("../ui/releases").join(file);
  let bytes =
    std::fs::read(&release).unwrap_or_else(|why| panic!("{} is not readable: {why}", release.display()));
  assert_eq!(
    format!("{:x}", Sha256::digest(&bytes)),
    pin["sha256"].as_str().expect("the pin names its sha256"),
    "{} is not the file the pin names",
    release.display()
  );
  release
}

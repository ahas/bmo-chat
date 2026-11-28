//! The face itself: the manifest assembled from the service's declaration and its bundle folder, held to the
//! contract's schema when it is built, its routes and the guard a service lays over its screens' own routes
//! (`docs/console-face.md`).

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Request, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::Value;

use crate::bundle::{BundleFolder, Served};
use crate::error::{ConsoleFaceError, Refusal};
use crate::guard::Guard;
use crate::schema;

/// The route a service answers its manifest at, as the contract names it.
pub const CONSOLE_MANIFEST_ROUTE: &str = "/console-manifest";

/// The path every file of the bundle is answered under.
pub const CONSOLE_FILES_PREFIX: &str = "/console/";

/// The fields of the manifest the face fills from the bundle folder, which a declaration never names.
const FILLED: [&str; 5] = ["layer", "layerUses", "bundle", "stylesheet", "words"];

/// A file whose path follows its bytes is kept for good.
const KEPT_FOR_GOOD: &str = "private, max-age=31536000, immutable";
/// The manifest is asked again each time.
const ASKED_AGAIN: &str = "no-cache";

/// A service's console face: its manifest, its bundle's files and the guard around them.
#[derive(Clone)]
pub struct ConsoleFace {
  inner: Arc<Inner>,
}

struct Inner {
  manifest: Value,
  words_name: String,
  words: Value,
  files: BTreeMap<String, Served>,
  guard: Guard,
}

impl ConsoleFace {
  /// The face of the service's declaration, a JSON object of every manifest field the service decides, over
  /// the bundle build's folder, guarded by the console's credential. A declaration naming a field the face
  /// fills, a file that cannot be read and a manifest the contract's schema refuses are each refused here,
  /// before anything is answered.
  pub fn new(declaration: &str, bundle: &BundleFolder, credential: &str) -> Result<Self, ConsoleFaceError> {
    let declared: Value = serde_json::from_str(declaration).map_err(|why| ConsoleFaceError::Malformed {
      what: "the declaration".to_owned(),
      why: format!("is not JSON: {why}"),
    })?;
    let Value::Object(mut manifest) = declared else {
      return Err(ConsoleFaceError::Malformed {
        what: "the declaration".to_owned(),
        why: "is not one JSON object".to_owned(),
      });
    };
    if let Some(field) = FILLED.iter().find(|field| manifest.contains_key(**field)) {
      return Err(ConsoleFaceError::FilledField((*field).to_owned()));
    }

    let read = bundle.read()?;
    manifest.insert("layer".to_owned(), Value::String(read.layer));
    if let Some(uses) = read.uses {
      manifest.insert("layerUses".to_owned(), uses);
    }
    manifest.insert("bundle".to_owned(), Value::String(format!("{CONSOLE_FILES_PREFIX}{}", read.module)));
    if let Some(stylesheet) = &read.stylesheet {
      manifest
        .insert("stylesheet".to_owned(), Value::String(format!("{CONSOLE_FILES_PREFIX}{stylesheet}")));
    }
    manifest
      .insert("words".to_owned(), Value::String(format!("{CONSOLE_FILES_PREFIX}{}", read.words_name)));
    let manifest = Value::Object(manifest);

    let faults = schema::faults(&manifest);
    if !faults.is_empty() {
      return Err(ConsoleFaceError::Contract(faults));
    }
    let guard = Guard::new(credential, &manifest);
    Ok(Self {
      inner: Arc::new(Inner {
        manifest,
        words_name: read.words_name,
        words: read.words,
        files: read.files,
        guard,
      }),
    })
  }

  /// The manifest as the face answers it.
  pub fn manifest(&self) -> &Value {
    &self.inner.manifest
  }

  /// A word of the bundle's words in a locale, by its dotted key, where the words hold it as text.
  pub fn word(&self, locale: &str, key: &str) -> Option<&str> {
    let mut at = self.inner.words.get(locale)?;
    for part in key.split('.') {
      at = at.get(part)?;
    }
    at.as_str()
  }

  /// The face's routes: the manifest and every file of the bundle, each behind its guard. A service merges
  /// them beside its own.
  pub fn router<S: Clone + Send + Sync + 'static>(&self) -> Router<S> {
    Router::new()
      .route(CONSOLE_MANIFEST_ROUTE, get(manifest))
      .route(&format!("{CONSOLE_FILES_PREFIX}{{*file}}"), get(file))
      .with_state(self.inner.clone())
  }

  /// The service's own routes that its screens call, with the screens' guard laid over them: a call carrying the
  /// console's credential is held to the manifest's role and owner key, and any other call passes on to the
  /// service's own guard untouched.
  pub fn guard_screens<S: Clone + Send + Sync + 'static>(&self, routes: Router<S>) -> Router<S> {
    routes.route_layer(middleware::from_fn_with_state(self.inner.clone(), screens))
  }
}

async fn manifest(State(face): State<Arc<Inner>>, headers: HeaderMap) -> Response {
  if let Err(refusal) = face.guard.console(&headers) {
    return refusal.into_response();
  }
  let mut answer = axum::Json(face.manifest.clone()).into_response();
  answer.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static(ASKED_AGAIN));
  answer
}

async fn file(State(face): State<Arc<Inner>>, Path(name): Path<String>, headers: HeaderMap) -> Response {
  if let Err(refusal) = face.guard.console(&headers) {
    return refusal.into_response();
  }
  if name != face.words_name
    && let Err(refusal) = face.guard.file(&headers)
  {
    return refusal.into_response();
  }
  let Some(served) = face.files.get(&name) else {
    return Refusal::NotFound.into_response();
  };
  let mut answer = served.bytes.clone().into_response();
  let headers = answer.headers_mut();
  if let Ok(content_type) = HeaderValue::from_str(&served.content_type) {
    headers.insert(CONTENT_TYPE, content_type);
  }
  headers.insert(CACHE_CONTROL, HeaderValue::from_static(KEPT_FOR_GOOD));
  answer
}

async fn screens(State(face): State<Arc<Inner>>, request: Request, next: Next) -> Response {
  if face.guard.carries_console_credential(request.headers())
    && !face.guard.shared(request.method(), request.uri().path())
    && let Err(refusal) = face.guard.reader(request.headers())
  {
    return refusal.into_response();
  }
  next.run(request).await
}

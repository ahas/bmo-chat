//! The guard around the face's routes and over a service's screens (`docs/console-face.md`, 「What it answers」
//! and 「The screens' guard」): the console's credential alone, the manifest's role and owner key on the bundle's
//! files, and a share bundle's files answered with neither.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{any, get as route};
use serde_json::{Value, json};
use tower::ServiceExt;

use bmo_console_face::{
  CONSOLE_MANIFEST_ROUTE, OWNER_KEY_HEADER, OWNER_ROLE_HEADER, contract_terms, share_reads,
};

use crate::support::{Built, CONSOLE, bearer, face, get, pinned_release};

const SERVICE: &str = "service-token";

async fn manifest_paths(router: &Router) -> Value {
  let console = bearer(CONSOLE);
  let (_, body) = get(router, "/console-manifest", &[("Authorization", &console)]).await;
  serde_json::from_slice(&body).expect("the manifest is JSON")
}

fn code(body: &[u8]) -> String {
  let refusal: Value = serde_json::from_slice(body).expect("the refusal is JSON");
  assert!(refusal["error"].is_string(), "the refusal names why");
  refusal["code"].as_str().expect("the refusal has a code").to_owned()
}

#[tokio::test]
async fn every_route_of_the_face_takes_the_console_credential_alone() {
  let built = Built::new("export default { routes: [] };");
  let router = face(json!({}), &built).router();
  let manifest = manifest_paths(&router).await;
  let service = bearer(SERVICE);
  for path in [
    "/console-manifest",
    manifest["words"].as_str().unwrap(),
    manifest["bundle"].as_str().unwrap(),
  ] {
    for headers in [vec![("Authorization", service.as_str())], vec![]] {
      let mut with_role = headers.clone();
      with_role.push(("Owner-Role", "administrator"));
      let (status, body) = get(&router, path, &with_role).await;
      assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
      assert_eq!(code(&body), "unauthorized");
    }
  }
}

#[tokio::test]
async fn the_bundle_files_ask_the_manifest_role_and_owner_key_and_the_words_ask_neither() {
  let built = Built::new("export default { routes: [] };");
  let router = face(json!({ "ownerKey": true }), &built).router();
  let manifest = manifest_paths(&router).await;
  let console = bearer(CONSOLE);
  let files = [
    manifest["bundle"].as_str().unwrap(),
    manifest["stylesheet"].as_str().unwrap(),
    "/console/chunk-a1b2c3.js",
  ];
  for file in files {
    for (role, key) in [(None, Some("7")), (Some("member"), Some("7")), (Some("administrator"), None)] {
      let mut headers = vec![("Authorization", console.as_str())];
      headers.extend(role.map(|role| ("Owner-Role", role)));
      headers.extend(key.map(|key| ("Owner-Key", key)));
      let (status, body) = get(&router, file, &headers).await;
      assert_eq!(status, StatusCode::FORBIDDEN, "{file} with {role:?} and {key:?}");
      assert_eq!(code(&body), "forbidden");
    }
    let reader = [("Authorization", console.as_str()), ("Owner-Role", "administrator"), ("Owner-Key", "7")];
    assert_eq!(get(&router, file, &reader).await.0, StatusCode::OK, "{file}");
  }
  let words = manifest["words"].as_str().unwrap();
  assert_eq!(get(&router, words, &[("Authorization", &console)]).await.0, StatusCode::OK);

  let reader = [("Authorization", console.as_str()), ("Owner-Role", "administrator"), ("Owner-Key", "7")];
  let (status, body) = get(&router, "/console/absent.js", &reader).await;
  assert_eq!(status, StatusCode::NOT_FOUND);
  assert_eq!(code(&body), "not-found");
}

#[tokio::test]
async fn a_member_screen_opens_to_an_administrator_too() {
  let built = Built::new("export default { routes: [] };");
  let router = face(json!({ "role": "member" }), &built).router();
  let module = manifest_paths(&router).await["bundle"].as_str().unwrap().to_owned();
  let console = bearer(CONSOLE);
  for role in ["member", "administrator"] {
    let (status, _) = get(&router, &module, &[("Authorization", &console), ("Owner-Role", role)]).await;
    assert_eq!(status, StatusCode::OK, "{role}");
  }
}

#[tokio::test]
async fn a_share_bundle_answers_its_files_with_no_role() {
  let built = Built::new("export default { routes: [] };");
  let router = face(json!({ "shareReads": ["/api/shares/{token}"] }), &built).router();
  let module = manifest_paths(&router).await["bundle"].as_str().unwrap().to_owned();
  let console = bearer(CONSOLE);
  assert_eq!(get(&router, &module, &[("Authorization", &console)]).await.0, StatusCode::OK);
  let service = bearer(SERVICE);
  assert_eq!(get(&router, &module, &[("Authorization", &service)]).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_screens_guard_holds_a_console_call_to_the_role_and_leaves_other_calls_to_the_service() {
  let built = Built::new("export default { routes: [] };");
  let face = face(json!({}), &built);
  let router: Router = face.guard_screens(Router::new().route("/api/notes", route(|| async { "notes" })));
  let console = bearer(CONSOLE);

  let (status, body) =
    get(&router, "/api/notes", &[("Authorization", &console), ("Owner-Role", "member")]).await;
  assert_eq!(status, StatusCode::FORBIDDEN);
  assert_eq!(code(&body), "forbidden");
  let reader = [("Authorization", console.as_str()), ("Owner-Role", "administrator")];
  assert_eq!(get(&router, "/api/notes", &reader).await.0, StatusCode::OK);
  let service = bearer(SERVICE);
  assert_eq!(get(&router, "/api/notes", &[("Authorization", &service)]).await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_screens_guard_lets_a_share_read_pass_on_to_the_token() {
  let built = Built::new("export default { routes: [] };");
  let face = face(
    json!({ "shareReads": ["/api/shares/{token}/files/{fileId}"], "shareHead": "/api/shares/{token}/head" }),
    &built,
  );
  let shared = Router::new()
    .route("/api/shares/{token}/files/{file}", route(|| async { "file" }).post(|| async { "kept" }))
    .route("/api/shares/{token}/head", route(|| async { "head" }));
  let router: Router = face.guard_screens(shared);
  let console = bearer(CONSOLE);
  let no_role = [("Authorization", console.as_str())];

  assert_eq!(get(&router, "/api/shares/abc/files/7", &no_role).await.0, StatusCode::OK);
  assert_eq!(get(&router, "/api/shares/abc/head", &no_role).await.0, StatusCode::OK);
  assert_eq!(get(&router, "/api/shares/%2E%2e/files/7", &no_role).await.0, StatusCode::FORBIDDEN);
  let posted = Request::post("/api/shares/abc/files/7")
    .header("Authorization", &console)
    .body(Body::empty())
    .expect("the request is built");
  let answer = router.clone().oneshot(posted).await.expect("answered");
  assert_eq!(answer.status(), StatusCode::FORBIDDEN, "only a read is a share read");
}

#[tokio::test]
async fn the_screens_guard_matches_a_share_read_as_bmo_ui_s_own_share_read_does() {
  let built = Built::new("export default { routes: [] };");
  let face = face(json!({ "shareReads": ["/api/shares/{token}/files/{fileId}"] }), &built);
  let router: Router = face.guard_screens(Router::new().route("/{*rest}", any(|| async { "answered" })));
  let asked = [
    ("GET", "/api/shares/abc/files/7"),
    ("get", "/api/shares/abc/files/7"),
    ("HEAD", "/api/shares/abc/files/7"),
    ("POST", "/api/shares/abc/files/7"),
    ("GET", "/api/shares/%2E%2e/files/7"),
    ("GET", "/api/shares/%2e/files/7"),
    ("GET", "/api/shares/a%2Eb/files/7"),
    ("GET", "/api/shares/%E0%A4%A/files/7"),
    ("GET", "/api/shares//files/7"),
    ("GET", "/api/shares/abc/files"),
    ("GET", "/api/shares/abc/files/7/more"),
    ("GET", "/api/other/abc/files/7"),
  ];
  let expected =
    share_reads(face.manifest(), &asked, &pinned_release()).unwrap_or_else(|why| panic!("{why}"));
  let console = bearer(CONSOLE);
  for ((method, path), shared) in asked.iter().zip(expected) {
    let request = Request::builder()
      .method(*method)
      .uri(*path)
      .header("Authorization", &console)
      .body(Body::empty());
    let answer = router.clone().oneshot(request.expect("the request is built")).await.expect("answered");
    let passed = answer.status() == StatusCode::OK;
    assert_eq!(passed, shared, "{method} {path}: the guard and bmo-ui's shareRead disagree");
  }
}

#[tokio::test]
async fn the_route_the_headers_and_the_order_of_the_roles_are_bmo_ui_s_own() {
  let terms = contract_terms(&pinned_release()).unwrap_or_else(|why| panic!("{why}"));
  assert_eq!(terms["route"], CONSOLE_MANIFEST_ROUTE);
  assert_eq!(terms["ownerKeyHeader"], OWNER_KEY_HEADER);
  assert_eq!(terms["ownerRoleHeader"], OWNER_ROLE_HEADER);

  let built = Built::new("export default { routes: [] };");
  let console = bearer(CONSOLE);
  let roles = terms["roles"].as_array().expect("the roles are a list");
  assert!(!roles.is_empty());
  for case in roles {
    let (held, needed) = (case["held"].as_str().unwrap(), case["needed"].as_str().unwrap());
    let router: Router = face(json!({ "role": needed }), &built)
      .guard_screens(Router::new().route("/api/notes", route(|| async { "notes" })));
    let (status, _) =
      get(&router, "/api/notes", &[("Authorization", &console), (OWNER_ROLE_HEADER, held)]).await;
    assert_eq!(status == StatusCode::OK, case["allows"] == true, "{held} for {needed}");
  }
}

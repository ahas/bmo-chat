//! The client against a stand-in chain: a kind declared under the unit's name, a resolution read in its three
//! answers, and a write carrying the caller's key and the declaring unit.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, put};
use bmo_chain::{Chain, Resolution, Subject, flag, path_of, units_of};
use serde_json::{Value, json};

// What the stand-in heard: each call's path, its Owner-Key and Declared-By headers, and its body.
type Heard = Arc<Mutex<Vec<(String, Option<String>, Option<String>, Value)>>>;

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
  headers.get(name).and_then(|value| value.to_str().ok()).map(str::to_owned)
}

async fn stand() -> (Chain, Heard) {
  let heard: Heard = Arc::default();
  let app = axum::Router::new()
    .route(
      "/chain/kinds/{key}",
      put(
        |State(heard): State<Heard>, Path(key): Path<String>, Json(body): Json<Value>| async move {
          heard.lock().unwrap().push((format!("kind {key}"), None, None, body));
          StatusCode::OK
        },
      ),
    )
    .route(
      "/chain/kinds/{key}/resolve",
      get(|Path(key): Path<String>, Query(query): Query<Vec<(String, String)>>| async move {
        match key.as_str() {
          "unknown" => (StatusCode::NOT_FOUND, Json(json!({ "code": "no-such-kind" }))),
          _ if query.iter().any(|(_, value)| value == "404") => {
            (StatusCode::NOT_FOUND, Json(json!({ "code": "not-found" })))
          }
          _ => (
            StatusCode::OK,
            Json(json!({
              "value": true,
              "layers": [{ "link": "platform" }, { "link": "unit:3" }, { "link": "unit:7" }],
              "path": ["platform", { "link": "unit:3" }, "person:9"],
            })),
          ),
        }
      }),
    )
    .route(
      "/chain/kinds/{key}/links/{link}/value",
      put(
        |State(heard): State<Heard>,
         Path((key, link)): Path<(String, String)>,
         headers: HeaderMap,
         Json(body): Json<Value>| async move {
          heard.lock().unwrap().push((
            format!("value {key} {link}"),
            header(&headers, "Owner-Key"),
            header(&headers, "Declared-By"),
            body,
          ));
          (StatusCode::FORBIDDEN, Json(json!({ "code": "not-a-writer" })))
        },
      ),
    )
    .with_state(heard.clone());
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());

  tokio::spawn(async move { axum::serve(listener, app).await });

  (Chain::new(&address, "token", "unit-a", Duration::from_secs(5)), heard)
}

#[tokio::test]
async fn a_kind_is_declared_under_the_unit_s_name() {
  let (chain, heard) = stand().await;

  chain.declare(&[json!({ "key": "app.x.offered", "shape": "flag" })]).await.unwrap();

  let heard = heard.lock().unwrap();

  assert_eq!(heard[0].0, "kind app.x.offered");
  assert_eq!(heard[0].3, json!({ "key": "app.x.offered", "shape": "flag", "declaredBy": "unit-a" }));
}

#[tokio::test]
async fn a_resolution_reads_as_resolved_undeclared_or_nothing() {
  let (chain, _) = stand().await;

  let Resolution::Resolved(answer) = chain.resolve("app.x.offered", Subject::Person(9)).await.unwrap()
  else {
    panic!("a resolved kind");
  };

  assert!(flag(Some(&answer), false));
  assert_eq!(units_of(&answer), [3, 7]);
  assert_eq!(path_of(&answer), ["platform", "unit:3", "person:9"]);
  assert_eq!(chain.resolve("unknown", Subject::Unit(3)).await.unwrap(), Resolution::Undeclared);
  assert_eq!(chain.resolve("app.x.offered", Subject::Person(404)).await.unwrap(), Resolution::Nothing);
  assert!(!flag(None, false), "the unit's fallback stands where the chain says nothing");
}

#[tokio::test]
async fn a_write_carries_the_caller_s_key_and_the_declaring_unit_and_answers_the_refusal_whole() {
  let (chain, heard) = stand().await;

  let answered = chain.write_value("app.x.offered", "unit:3", 9, json!(false)).await.unwrap();

  assert!(!answered.ok());
  assert_eq!((answered.status, &answered.body["code"]), (403, &json!("not-a-writer")));

  let heard = heard.lock().unwrap();

  assert_eq!(
    (&heard[0].0, heard[0].1.as_deref(), heard[0].2.as_deref(), &heard[0].3),
    (
      &"value app.x.offered unit:3".to_owned(),
      Some("9"),
      Some("unit-a"),
      &json!({ "value": false })
    )
  );
}

#[tokio::test]
async fn a_chain_naming_no_service_says_so() {
  assert!(!Chain::new("", "", "unit-a", Duration::from_secs(1)).named());
}

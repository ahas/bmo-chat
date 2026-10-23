//! The server for the provider routes, driven in process over a stand-in provider: every route, its token,
//! the error shape and the gate on readiness, as `docs/protocol.md` states them.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::Request;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bmo_chat::{
  ChatAvatar, ChatAvatarKind, ChatCapability, ChatDialect, ChatDialectLimits, ChatFailure, ChatMarkup,
  ChatMediaRequest, ChatMediaResult, ChatModeStanding, ChatNumberRanges, ChatNumberSpace, ChatNumbering,
  ChatPicture, ChatProvider, ChatReach, ChatReadMode, ChatReceiver, ChatRefusal, ChatSend, ChatSendRequest,
  ChatSendResult, ChatSent, ChatVoice, MessageFile, ProviderRoom, RETRY_AFTER_SECONDS, RoomKind,
  provider_router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "the-provider-token";
const ROOM: i64 = -100;
const OWNER: i64 = 42;
/// A room the stand-in knows but does not own.
const ELSEWHERE: i64 = 9;
/// A room the stand-in knows nothing of.
const UNKNOWN: i64 = 13;

/// A dialect whose every answer shows which of its members gave it.
struct Loud;

impl ChatDialect for Loud {
  fn name(&self) -> &str {
    "loud"
  }
  fn limits(&self) -> ChatDialectLimits {
    ChatDialectLimits::default()
  }
  fn writing(&self) -> &str {
    "Everything in this room is shouted."
  }
  fn foldable(&self) -> bool {
    true
  }
  fn convert(&self, markdown: &str) -> ChatMarkup {
    ChatMarkup {
      markup: markdown.to_uppercase(),
      plain: markdown.to_owned(),
      formatted: true,
    }
  }
  fn parts(&self, markdown: &str) -> Vec<String> {
    markdown.split("\n\n").map(str::to_owned).collect()
  }
  fn fault(&self, markup: &str) -> Option<String> {
    (markup.contains('<') && !markup.contains('>')).then(|| "A tag is left open.".to_owned())
  }
  /// Nothing short is worth folding.
  fn fold(&self, markdown: &str) -> Option<String> {
    (markdown.len() > 10).then(|| format!("[folded] {markdown}"))
  }
  fn visible(&self, markup: &str) -> String {
    markup.to_lowercase()
  }
}

/// The stand-in provider. It says it is not ready while it is not configured, fails every question while
/// told to, and keeps what it was asked so a test can read it back.
struct Desk {
  can: BTreeSet<ChatCapability>,
  configured: Mutex<Option<String>>,
  failing: AtomicBool,
  asked: Mutex<Vec<String>>,
}

impl Desk {
  fn new(can: &[ChatCapability]) -> Arc<Self> {
    Arc::new(Self {
      can: can.iter().copied().collect(),
      configured: Mutex::new(None),
      failing: AtomicBool::new(false),
      asked: Mutex::new(Vec::new()),
    })
  }

  fn unconfigured(&self, reason: &str) {
    *self.configured.lock().unwrap() = Some(reason.to_owned());
  }

  fn fails(&self) {
    self.failing.store(true, Ordering::SeqCst);
  }

  fn asked(&self) -> Vec<String> {
    self.asked.lock().unwrap().clone()
  }

  fn ask(&self, what: String) -> Result<(), ChatFailure> {
    self.asked.lock().unwrap().push(what);
    if self.failing.load(Ordering::SeqCst) {
      return Err("The platform connection dropped.".into());
    }
    Ok(())
  }
}

#[async_trait]
impl ChatProvider for Desk {
  fn key(&self) -> &str {
    "stand-in"
  }
  fn label(&self) -> &str {
    "Platform"
  }
  fn version(&self) -> &str {
    "2.3.4"
  }
  fn configured(&self) -> Option<String> {
    self.configured.lock().unwrap().clone()
  }
  async fn start(&self, _receiver: Arc<dyn ChatReceiver>) -> Result<(), ChatFailure> {
    Ok(())
  }
  async fn stop(&self) {}
  fn modes(&self) -> Vec<ChatReadMode> {
    vec![ChatReadMode {
      voice: ChatVoice::BotAccount,
      reach: ChatReach::AllMessages,
      standing: ChatModeStanding::Active,
      reason: None,
      can_reply_to: true,
    }]
  }
  fn can(&self) -> BTreeSet<ChatCapability> {
    self.can.clone()
  }
  fn dialect(&self) -> &dyn ChatDialect {
    &Loud
  }
  fn numbering(&self) -> ChatNumbering {
    ChatNumbering {
      rooms: ChatNumberSpace::Native,
      people: ChatNumberSpace::Native,
      messages: ChatNumberSpace::Native,
      ranges: ChatNumberRanges::default(),
    }
  }
  async fn room(&self, room_id: i64) -> Result<Option<ProviderRoom>, ChatFailure> {
    self.ask(format!("room {room_id}"))?;
    Ok(match room_id {
      ROOM => Some(ProviderRoom {
        id: ROOM,
        owns: true,
        identity: Some(7),
        kind: RoomKind::Owner,
        private_to: Some(OWNER),
        title: Some("A Room".into()),
        modes: None,
      }),
      ELSEWHERE => Some(ProviderRoom {
        id: ELSEWHERE,
        owns: false,
        identity: None,
        kind: RoomKind::Group,
        private_to: None,
        title: None,
        modes: Some(vec![ChatVoice::HumanReader]),
      }),
      _ => None,
    })
  }
  async fn owner_room(&self) -> Result<Option<i64>, ChatFailure> {
    self.ask("owner".to_owned())?;
    Ok(Some(ROOM))
  }
  async fn account_rooms(&self) -> Result<Vec<ProviderRoom>, ChatFailure> {
    self.ask("rooms".to_owned())?;
    let room = |id, title: &str| ProviderRoom {
      id,
      owns: true,
      identity: Some(7),
      kind: RoomKind::Group,
      private_to: None,
      title: Some(title.into()),
      modes: None,
    };
    Ok(vec![room(5, "Five"), room(ROOM, "A Room"), room(-7, "Seven"), room(ROOM, "A Room")])
  }
  async fn send(&self, request: ChatSendRequest) -> Result<ChatSendResult, ChatFailure> {
    let what = match &request.send {
      ChatSend::Document { bytes, .. } => format!("send a document of {} bytes", bytes.len()),
      send => format!("send {:?} to {}", send.kind(), request.room_id),
    };
    self.ask(what)?;
    if let ChatSend::Delete { message_id } = request.send {
      return Ok(ChatSendResult::Refused(ChatRefusal::Gone {
        label: self.label().to_owned(),
        detail: format!("Message {message_id} is no longer there."),
      }));
    }
    Ok(ChatSendResult::Sent(ChatSent {
      message_id: Some(40),
      missed: vec![],
    }))
  }
  async fn avatars(&self) -> Result<Vec<ChatAvatar>, ChatFailure> {
    self.ask("avatars".into())?;
    Ok(kept_avatars())
  }
  async fn avatar(&self, kind: ChatAvatarKind, id: i64) -> Result<Option<ChatPicture>, ChatFailure> {
    self.ask(format!("avatar {kind} {id}"))?;
    Ok((kind == ChatAvatarKind::Room && id == ROOM).then(|| ChatPicture {
      mime: "image/jpeg".into(),
      bytes: vec![0xff, 0xd8, 0xff],
    }))
  }
  async fn fetch_media(&self, request: ChatMediaRequest) -> Result<ChatMediaResult, ChatFailure> {
    let ChatMediaRequest {
      room_id,
      message_id,
      sent_at,
    } = request;
    self.ask(format!("files {room_id} {message_id} {sent_at:?}"))?;
    if message_id != 5 {
      return Ok(ChatMediaResult::Unavailable {
        reason: "The platform no longer has it.".into(),
      });
    }
    let file = |id: &str, name: &str| MessageFile {
      id: id.into(),
      name: name.into(),
      mime: "image/png".into(),
      bytes: vec![137, 80],
      kind: None,
    };
    Ok(ChatMediaResult::Fetched(vec![file("file-1", "a.png"), file("file-2", "b.png")]))
  }
}

/// The pictures the stand-in keeps, out of order, two of them taken at the same moment.
fn kept_avatars() -> Vec<ChatAvatar> {
  let avatar = |kind, id, taken_at| ChatAvatar { kind, id, taken_at };
  vec![
    avatar(ChatAvatarKind::Person, OWNER, 3_000),
    avatar(ChatAvatarKind::Room, ROOM, 1_000),
    avatar(ChatAvatarKind::Person, 7, 2_000),
    avatar(ChatAvatarKind::Room, ELSEWHERE, 2_000),
  ]
}

fn router(desk: &Arc<Desk>) -> Router {
  provider_router(desk.clone(), TOKEN)
}

/// An answer: its status, its `Retry-After` where it gave one, and its JSON body.
struct Answer {
  status: u16,
  retry_after: Option<String>,
  body: Value,
}

/// One request with the given authorization header.
async fn answer_as(
  router: &Router,
  authorization: Option<&str>,
  method: &str,
  path: &str,
  body: Option<Value>,
) -> Answer {
  let mut request = Request::builder().method(method).uri(path);
  if let Some(authorization) = authorization {
    request = request.header("authorization", authorization);
  }
  let body = match body {
    Some(body) => {
      request = request.header("content-type", "application/json");
      Body::from(serde_json::to_vec(&body).unwrap())
    }
    None => Body::empty(),
  };
  let answer = router.clone().oneshot(request.body(body).unwrap()).await.expect("the router answers");
  let status = answer.status().as_u16();
  let retry_after = answer.headers().get("retry-after").map(|value| value.to_str().unwrap().to_owned());
  let bytes = to_bytes(answer.into_body(), usize::MAX).await.expect("the answer has a body");
  let body = serde_json::from_slice(&bytes).expect("every answer is JSON");
  Answer {
    status,
    retry_after,
    body,
  }
}

/// One request with the token, answered with its status and its JSON body.
async fn call(router: &Router, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
  let Answer { status, body, .. } =
    answer_as(router, Some(&format!("Bearer {TOKEN}")), method, path, body).await;
  (status, body)
}

fn text() -> Option<Value> {
  Some(json!({ "kind": "text", "roomId": ROOM, "fields": { "text": "hello" } }))
}

/// The path of a message's files.
fn files(message_id: i64) -> String {
  format!("/rooms/{ROOM}/messages/{message_id}/files")
}

/// Every provider route but the avatar routes, with a body it takes.
fn routes() -> Vec<(&'static str, String, Option<Value>)> {
  vec![
    ("GET", "/declaration".into(), None),
    ("GET", "/health".into(), None),
    ("POST", "/sends".into(), text()),
    ("GET", "/rooms".into(), None),
    ("GET", format!("/rooms/{ROOM}"), None),
    ("GET", "/owner".into(), None),
    ("GET", files(5), None),
    ("POST", "/dialect/readings".into(), Some(json!({ "text": "hello" }))),
    ("POST", "/dialect/checks".into(), Some(json!({ "markup": "hello" }))),
  ]
}

/// Whether a route answers whatever state the provider is in.
fn ungated(path: &str) -> bool {
  matches!(path, "/declaration" | "/health" | "/dialect/readings" | "/dialect/checks")
}

#[tokio::test]
async fn every_route_and_every_unknown_path_needs_the_token() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  let unauthorized = json!({ "code": "unauthorized", "error": "The bearer token is missing or wrong." });
  let mut routes = routes();
  routes.extend([
    ("GET", "/avatars".into(), None),
    ("GET", format!("/rooms/{ROOM}/avatar"), None),
    ("GET", "/people/7/avatar".into(), None),
    ("GET", "/nowhere".into(), None),
  ]);
  for (method, path, body) in routes {
    for authorization in [None, Some("Bearer another-token"), Some("Basic dGhlLXByb3ZpZGVyLXRva2Vu")] {
      let answer = answer_as(&router, authorization, method, &path, body.clone()).await;
      assert_eq!(
        (answer.status, answer.body),
        (401, unauthorized.clone()),
        "{method} {path} with {authorization:?}"
      );
    }
  }
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn an_unknown_path_or_method_or_a_key_that_is_not_a_number_is_no_route() {
  let desk = Desk::new(&[ChatCapability::Avatars]);
  let router = router(&desk);
  for (method, path) in [
    ("GET", "/nowhere"),
    ("GET", "/sends"),
    ("POST", "/declaration"),
    ("GET", "/rooms/seven"),
    ("GET", "/rooms/-100/messages/five/files"),
    ("GET", "/people/someone/avatar"),
  ] {
    assert_eq!(
      call(&router, method, path, None).await,
      (
        404,
        json!({ "code": "no-route", "error": format!("There is no route at {method} {path}.") })
      ),
      "{method} {path}"
    );
  }
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn declaration_and_health_while_ready() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  let declared = serde_json::to_value(desk.declaration()).unwrap();
  assert_eq!(call(&router, "GET", "/declaration", None).await, (200, declared));
  assert_eq!(
    call(&router, "GET", "/health", None).await,
    (200, json!({ "ready": true, "reason": null, "version": "2.3.4" }))
  );
}

#[tokio::test]
async fn while_not_ready_the_gated_routes_answer_not_ready_with_when_to_ask_again() {
  let desk = Desk::new(&[ChatCapability::Text, ChatCapability::FetchMedia]);
  desk.unconfigured("No token is configured.");
  let router = router(&desk);
  for (method, path, body) in routes() {
    let answer = answer_as(&router, Some(&format!("Bearer {TOKEN}")), method, &path, body).await;
    match path.as_str() {
      "/declaration" => {
        assert_eq!(answer.status, 200);
        assert_eq!(answer.body["configured"], "No token is configured.");
      }
      "/health" => assert_eq!(
        (answer.status, answer.body),
        (200, json!({ "ready": false, "reason": "No token is configured.", "version": "2.3.4" }))
      ),
      path if ungated(path) => assert_eq!(answer.status, 200, "{path} answers in any state"),
      _ => {
        assert_eq!(
          (answer.status, answer.body),
          (
            503,
            json!({ "code": "not-ready", "error": "Platform is not ready yet: No token is configured." })
          ),
          "{method} {path}"
        );
        assert_eq!(answer.retry_after, Some(RETRY_AFTER_SECONDS.to_string()), "{method} {path}");
      }
    }
  }
  assert_eq!(desk.asked(), Vec::<String>::new(), "a provider that is not ready is never asked");
}

#[tokio::test]
async fn a_body_or_query_not_of_the_route_shape_is_a_bad_request_naming_what_is_wrong() {
  let desk = Desk::new(&[ChatCapability::Text, ChatCapability::FetchMedia]);
  let router = router(&desk);
  let (status, answer) =
    call(&router, "POST", "/dialect/readings", Some(json!({ "markdown": "hi" }))).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")));
  assert!(answer["error"].as_str().unwrap().contains("missing field `text`"), "{answer}");
  let shout = json!({ "kind": "shout", "roomId": ROOM, "fields": {} });
  let (status, answer) = call(&router, "POST", "/sends", Some(shout)).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")), "{answer}");
  let (status, answer) = call(&router, "POST", "/sends", None).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")), "{answer}");
  let (status, answer) = call(&router, "GET", &format!("{}?sentAt=soon", files(5)), None).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")), "{answer}");
  assert!(answer["error"].as_str().unwrap().starts_with("The query is not the shape"), "{answer}");
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn a_declared_send_reaches_the_provider_and_is_answered_created() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  assert_eq!(
    call(&router, "POST", "/sends", text()).await,
    (201, json!({ "messageId": 40, "missed": [] }))
  );
  assert_eq!(desk.asked(), ["send Text to -100"]);
}

#[tokio::test]
async fn a_send_carrying_a_large_file_is_taken_whole() {
  let desk = Desk::new(&[ChatCapability::Document]);
  let router = router(&desk);
  let bytes = vec![7u8; 5 * 1024 * 1024];
  let send = json!({
    "kind": "document", "roomId": ROOM,
    "fields": { "bytes": STANDARD.encode(&bytes), "fileName": "large.bin" },
  });
  assert_eq!(
    call(&router, "POST", "/sends", Some(send)).await,
    (201, json!({ "messageId": 40, "missed": [] }))
  );
  assert_eq!(desk.asked(), [format!("send a document of {} bytes", bytes.len())]);
}

#[tokio::test]
async fn an_undeclared_send_is_refused_before_the_provider_is_asked() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  let pin = json!({ "kind": "pin", "roomId": ROOM, "fields": { "messageId": 5, "on": true } });
  assert_eq!(
    call(&router, "POST", "/sends", Some(pin)).await,
    (
      422,
      json!({ "code": "unsupported-capability", "error": desk.unsupported(ChatCapability::Pin).to_string() })
    )
  );
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn a_refusal_the_provider_answers_crosses_with_its_code_and_status() {
  let desk = Desk::new(&[ChatCapability::Delete]);
  let delete = json!({ "kind": "delete", "roomId": ROOM, "fields": { "messageId": 5 } });
  assert_eq!(
    call(&router(&desk), "POST", "/sends", Some(delete)).await,
    (410, json!({ "code": "gone", "error": "Platform: Message 5 is no longer there." }))
  );
}

#[tokio::test]
async fn a_provider_that_fails_answers_failed() {
  let desk = Desk::new(&[ChatCapability::Text, ChatCapability::FetchMedia, ChatCapability::RoomList]);
  desk.fails();
  let router = router(&desk);
  let failed = json!({ "code": "failed", "error": "The platform connection dropped." });
  let gated: Vec<_> = routes().into_iter().filter(|(_, path, _)| !ungated(path)).collect();
  for (method, path, body) in &gated {
    assert_eq!(
      call(&router, method, path, body.clone()).await,
      (500, failed.clone()),
      "{method} {path}"
    );
  }
  assert_eq!(desk.asked().len(), gated.len(), "{:?}", desk.asked());
}

#[tokio::test]
async fn files_are_refused_where_undeclared_and_fetched_as_one_page_where_declared() {
  let undeclared = Desk::new(&[ChatCapability::Text]);
  assert_eq!(
    call(&router(&undeclared), "GET", &files(5), None).await,
    (
      422,
      json!({
        "code": "unsupported-capability", "error": "Platform cannot fetch a picture or a file later.",
      })
    )
  );
  assert_eq!(undeclared.asked(), Vec::<String>::new());
  let declared = Desk::new(&[ChatCapability::FetchMedia]);
  let router = router(&declared);
  let file = |id: &str, name: &str| json!({ "id": id, "name": name, "mime": "image/png", "bytes": "iVA=" });
  assert_eq!(
    call(&router, "GET", &format!("{}?sentAt=1760000000000", files(5)), None).await,
    (
      200,
      json!({ "items": [file("file-1", "a.png"), file("file-2", "b.png")], "more": false })
    )
  );
  assert_eq!(
    call(&router, "GET", &files(6), None).await,
    (404, json!({ "code": "unavailable", "error": "The platform no longer has it." }))
  );
  assert_eq!(declared.asked(), ["files -100 5 Some(1760000000000)", "files -100 6 None"]);
}

#[tokio::test]
async fn the_rooms_of_the_account_are_refused_where_undeclared_and_paged_by_number_where_declared() {
  let undeclared = Desk::new(&[ChatCapability::Text]);
  assert_eq!(
    call(&router(&undeclared), "GET", "/rooms", None).await,
    (
      422,
      json!({ "code": "unsupported-capability", "error": "Platform cannot list the rooms of its account." })
    )
  );
  assert_eq!(undeclared.asked(), Vec::<String>::new());
  let declared = Desk::new(&[ChatCapability::RoomList]);
  let router = router(&declared);
  let listed = async |query: &str| {
    let (status, answer) = call(&router, "GET", &format!("/rooms{query}"), None).await;
    assert_eq!(status, 200, "{query}: {answer}");
    let ids: Vec<i64> = answer["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|room| room["id"].as_i64().unwrap())
      .collect();
    (ids, answer["more"].as_bool().unwrap())
  };
  assert_eq!(listed("").await, (vec![ROOM, -7, 5], false));
  assert_eq!(listed("?after=-100").await, (vec![-7, 5], false));
  assert_eq!(listed("?after=-100&limit=1").await, (vec![-7], true));
  assert_eq!(listed("?limit=3").await, (vec![ROOM, -7, 5], false));
  assert_eq!(listed("?after=5").await, (Vec::new(), false));
  let (status, answer) = call(&router, "GET", "/rooms", None).await;
  assert_eq!(
    answer["items"][0],
    json!({
      "id": ROOM, "owns": true, "identity": 7, "kind": "group", "privateTo": null, "title": "A Room",
      "modes": null,
    }),
    "{status}"
  );
  let (status, answer) = call(&router, "GET", "/rooms?after=first", None).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")), "{answer}");
  assert_eq!(declared.asked().len(), 6);
}

#[tokio::test]
async fn the_dialect_routes_answer_from_the_dialect() {
  let router = router(&Desk::new(&[]));
  assert_eq!(
    call(&router, "POST", "/dialect/readings", Some(json!({ "text": "first part\n\nsecond" }))).await,
    (
      200,
      json!({
        "markup": "FIRST PART\n\nSECOND", "plain": "first part\n\nsecond", "formatted": true,
        "parts": ["first part", "second"], "fold": "[folded] first part\n\nsecond",
      })
    )
  );
  assert_eq!(
    call(&router, "POST", "/dialect/readings", Some(json!({ "text": "short" }))).await,
    (
      200,
      json!({ "markup": "SHORT", "plain": "short", "formatted": true, "parts": ["short"], "fold": null })
    )
  );
  assert_eq!(
    call(&router, "POST", "/dialect/checks", Some(json!({ "markup": "<B Open" }))).await,
    (200, json!({ "fault": "A tag is left open.", "visible": "<b open" }))
  );
  assert_eq!(
    call(&router, "POST", "/dialect/checks", Some(json!({ "markup": "<B>Shut</B>" }))).await,
    (200, json!({ "fault": null, "visible": "<b>shut</b>" }))
  );
}

#[tokio::test]
async fn a_room_and_the_owner_are_answered_from_the_provider_and_an_unknown_room_is_not_found() {
  let desk = Desk::new(&[]);
  let router = router(&desk);
  assert_eq!(
    call(&router, "GET", &format!("/rooms/{ROOM}"), None).await,
    (
      200,
      json!({
        "id": ROOM, "owns": true, "identity": 7, "kind": "owner", "privateTo": OWNER, "title": "A Room",
        "modes": null,
      })
    )
  );
  assert_eq!(
    call(&router, "GET", &format!("/rooms/{ELSEWHERE}"), None).await,
    (
      200,
      json!({
        "id": ELSEWHERE, "owns": false, "identity": null, "kind": "group", "privateTo": null, "title": null,
        "modes": ["human-reader"],
      })
    )
  );
  assert_eq!(
    call(&router, "GET", &format!("/rooms/{UNKNOWN}"), None).await,
    (404, json!({ "code": "not-found", "error": "Nothing is known of room 13." }))
  );
  assert_eq!(call(&router, "GET", "/owner", None).await, (200, json!({ "roomId": ROOM })));
  assert_eq!(desk.asked(), ["room -100", "room 9", "room 13", "owner"]);
}

/// One request with the token and no body, answered with its status, its type and its bytes.
async fn fetch(router: &Router, path: &str) -> (u16, Option<String>, Vec<u8>) {
  let request = Request::builder()
    .method("GET")
    .uri(path)
    .header("authorization", format!("Bearer {TOKEN}"))
    .body(Body::empty())
    .unwrap();
  let answer = router.clone().oneshot(request).await.expect("the router answers");
  let status = answer.status().as_u16();
  let mime = answer.headers().get("content-type").map(|value| value.to_str().unwrap().to_owned());
  let bytes = to_bytes(answer.into_body(), usize::MAX).await.expect("the answer has a body");
  (status, mime, bytes.to_vec())
}

#[tokio::test]
async fn a_provider_that_does_not_declare_avatars_keeps_none_and_is_not_asked() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  assert_eq!(
    call(&router, "GET", "/avatars", None).await,
    (200, json!({ "items": [], "more": false }))
  );
  assert_eq!(
    call(&router, "GET", &format!("/rooms/{ROOM}/avatar"), None).await,
    (
      404,
      json!({ "code": "not-found", "error": "No profile picture is kept for room -100." })
    )
  );
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn the_kept_pictures_are_paged_oldest_first_from_the_cursor_and_never_parted_at_one_moment() {
  let desk = Desk::new(&[ChatCapability::Avatars]);
  let router = router(&desk);
  let listed = async |query: &str| {
    let (status, answer) = call(&router, "GET", &format!("/avatars{query}"), None).await;
    assert_eq!(status, 200, "{query}: {answer}");
    let subjects: Vec<String> = answer["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|avatar| format!("{} {} {}", avatar["kind"].as_str().unwrap(), avatar["id"], avatar["takenAt"]))
      .collect();
    (subjects, answer["more"].as_bool().unwrap())
  };
  let every = ["room -100 1000", "room 9 2000", "person 7 2000", "person 42 3000"];
  assert_eq!(listed("").await, (every.map(String::from).to_vec(), false));
  assert_eq!(listed("?after=1000").await, (every[1..].iter().map(|s| s.to_string()).collect(), false));
  assert_eq!(
    listed("?after=1000&limit=1").await,
    (every[1..3].iter().map(|s| s.to_string()).collect(), true)
  );
  assert_eq!(
    listed("?after=1000&limit=3").await,
    (every[1..].iter().map(|s| s.to_string()).collect(), false)
  );
  assert_eq!(listed("?limit=1").await, (vec![every[0].to_string()], true));
  assert_eq!(listed("?limit=0").await, (Vec::new(), true));
  assert_eq!(listed("?after=3000").await, (Vec::new(), false));
  let (status, answer) = call(&router, "GET", "/avatars?after=yesterday", None).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")), "{answer}");
  assert_eq!(desk.asked().len(), 7);
}

#[tokio::test]
async fn a_kept_picture_is_answered_as_its_bytes_and_a_missing_one_as_not_found() {
  let desk = Desk::new(&[ChatCapability::Avatars]);
  desk.unconfigured("No token is configured.");
  let router = router(&desk);
  assert_eq!(
    fetch(&router, &format!("/rooms/{ROOM}/avatar")).await,
    (200, Some("image/jpeg".into()), vec![0xff, 0xd8, 0xff]),
    "a kept picture is served whatever state the provider is in"
  );
  assert_eq!(
    call(&router, "GET", "/people/7/avatar", None).await,
    (404, json!({ "code": "not-found", "error": "No profile picture is kept for person 7." }))
  );
  assert_eq!(desk.asked(), ["avatar room -100", "avatar person 7"]);
  desk.fails();
  let failed = json!({ "code": "failed", "error": "The platform connection dropped." });
  assert_eq!(
    call(&router, "GET", &format!("/rooms/{ROOM}/avatar"), None).await,
    (500, failed.clone())
  );
  assert_eq!(call(&router, "GET", "/avatars", None).await, (500, failed));
}

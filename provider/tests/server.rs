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
  ChatPicture, ChatProvider, ChatReach, ChatReadMode, ChatReceiver, ChatSend, ChatSendRequest,
  ChatSendResult, ChatVoice, MessageFile, RoomKind, provider_router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "the-provider-token";
const ROOM: i64 = -100;
const OWNER: i64 = 42;

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
  async fn owns_room(&self, chat_id: i64) -> Result<bool, ChatFailure> {
    self.ask(format!("owns {chat_id}"))?;
    Ok(chat_id == ROOM)
  }
  async fn identity_of(&self, chat_id: i64) -> Result<Option<i64>, ChatFailure> {
    self.ask(format!("identity {chat_id}"))?;
    Ok((chat_id == ROOM).then_some(7))
  }
  async fn is_owner_room(&self, chat_id: i64) -> Result<RoomKind, ChatFailure> {
    self.ask(format!("is-owner {chat_id}"))?;
    Ok(if chat_id == ROOM { RoomKind::Owner } else { RoomKind::Group })
  }
  async fn private_room_of(&self, chat_id: i64) -> Result<Option<i64>, ChatFailure> {
    self.ask(format!("private-of {chat_id}"))?;
    Ok((chat_id == ROOM).then_some(OWNER))
  }
  async fn owner_room(&self) -> Result<Option<i64>, ChatFailure> {
    self.ask("owner-room".to_owned())?;
    Ok(Some(ROOM))
  }
  async fn room_title(&self, chat_id: i64) -> Result<Option<String>, ChatFailure> {
    self.ask(format!("title {chat_id}"))?;
    Ok((chat_id == ROOM).then(|| "A Room".to_owned()))
  }
  async fn send(&self, request: ChatSendRequest) -> Result<ChatSendResult, ChatFailure> {
    let what = match &request.send {
      ChatSend::Document { bytes, .. } => format!("send a document of {} bytes", bytes.len()),
      send => format!("send {:?} to {}", send.verb(), request.chat_id),
    };
    self.ask(what)?;
    Ok(ChatSendResult::Sent {
      message_id: Some(40),
      missed: vec![],
    })
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
    self.ask(format!("media {} {}", request.chat_id, request.message_id))?;
    Ok(ChatMediaResult::Fetched(MessageFile {
      id: "file-1".into(),
      name: "a.png".into(),
      mime: "image/png".into(),
      bytes: vec![137, 80],
      kind: None,
    }))
  }
}

/// The pictures the stand-in keeps, out of order, two of them taken at the same moment.
fn kept_avatars() -> Vec<ChatAvatar> {
  let avatar = |kind, id, taken_at| ChatAvatar { kind, id, taken_at };
  vec![
    avatar(ChatAvatarKind::Person, OWNER, 3_000),
    avatar(ChatAvatarKind::Room, ROOM, 1_000),
    avatar(ChatAvatarKind::Person, 7, 2_000),
    avatar(ChatAvatarKind::Room, 9, 2_000),
  ]
}

fn router(desk: &Arc<Desk>) -> Router {
  provider_router(desk.clone(), TOKEN)
}

/// One request with the given authorization header, answered with its status and its JSON body.
async fn call_as(
  router: &Router,
  authorization: Option<&str>,
  method: &str,
  path: &str,
  body: Option<Value>,
) -> (u16, Value) {
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
  let bytes = to_bytes(answer.into_body(), usize::MAX).await.expect("the answer has a body");
  let body = serde_json::from_slice(&bytes).expect("every answer is JSON");
  (status, body)
}

async fn call(router: &Router, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
  call_as(router, Some(&format!("Bearer {TOKEN}")), method, path, body).await
}

fn room() -> Option<Value> {
  Some(json!({ "chatId": ROOM }))
}

fn text() -> Option<Value> {
  Some(json!({ "verb": "text", "chatId": ROOM, "text": "hello" }))
}

/// Every provider route, with a body it takes.
fn routes() -> Vec<(&'static str, &'static str, Option<Value>)> {
  vec![
    ("GET", "/declaration", None),
    ("GET", "/health", None),
    ("POST", "/send", text()),
    ("POST", "/media", Some(json!({ "chatId": ROOM, "messageId": 5 }))),
    ("POST", "/dialect/read", Some(json!({ "text": "hello" }))),
    ("POST", "/dialect/check", Some(json!({ "markup": "hello" }))),
    ("POST", "/room/owns", room()),
    ("POST", "/room/identity", room()),
    ("POST", "/room/is-owner", room()),
    ("POST", "/room/private-of", room()),
    ("POST", "/room/title", room()),
    ("GET", "/owner-room", None),
  ]
}

#[tokio::test]
async fn every_route_and_every_unknown_path_needs_the_token() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  let unauthorized = json!({ "code": "unauthorized", "error": "The bearer token is missing or wrong." });
  let mut routes = routes();
  routes.extend([
    ("GET", "/avatars", None),
    ("GET", "/avatars/room/-100", None),
    ("GET", "/nowhere", None),
  ]);
  for (method, path, body) in routes {
    for authorization in [None, Some("Bearer another-token"), Some("Basic dGhlLXByb3ZpZGVyLXRva2Vu")] {
      let answer = call_as(&router, authorization, method, path, body.clone()).await;
      assert_eq!(answer, (401, unauthorized.clone()), "{method} {path} with {authorization:?}");
    }
  }
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn an_unknown_path_or_method_is_no_route() {
  let router = router(&Desk::new(&[]));
  assert_eq!(
    call(&router, "GET", "/nowhere", None).await,
    (404, json!({ "code": "no-route", "error": "There is no route at GET /nowhere." }))
  );
  assert_eq!(
    call(&router, "GET", "/send", None).await,
    (404, json!({ "code": "no-route", "error": "There is no route at GET /send." }))
  );
  assert_eq!(
    call(&router, "POST", "/declaration", None).await,
    (404, json!({ "code": "no-route", "error": "There is no route at POST /declaration." }))
  );
}

#[tokio::test]
async fn declaration_and_health_while_ready() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  let declared = serde_json::to_value(desk.declaration()).unwrap();
  assert_eq!(call(&router, "GET", "/declaration", None).await, (200, declared));
  assert_eq!(
    call(&router, "GET", "/health", None).await,
    (200, json!({ "ready": true, "reason": null }))
  );
}

#[tokio::test]
async fn while_not_ready_only_the_declaration_health_and_dialect_routes_answer() {
  let desk = Desk::new(&[ChatCapability::Text, ChatCapability::FetchMedia]);
  desk.unconfigured("No token is configured.");
  let router = router(&desk);
  for (method, path, body) in routes() {
    let (status, answer) = call(&router, method, path, body).await;
    match path {
      "/declaration" => {
        assert_eq!(status, 200);
        assert_eq!(answer["configured"], "No token is configured.");
      }
      "/health" => {
        assert_eq!((status, answer), (200, json!({ "ready": false, "reason": "No token is configured." })))
      }
      "/dialect/read" | "/dialect/check" => assert_eq!(status, 200, "{path} answers in any state"),
      _ => assert_eq!(
        (status, answer),
        (
          409,
          json!({ "code": "not-ready", "error": "Platform is not ready yet: No token is configured." })
        ),
        "{method} {path}"
      ),
    }
  }
  assert_eq!(desk.asked(), Vec::<String>::new(), "a provider that is not ready is never asked");
}

#[tokio::test]
async fn a_body_not_of_the_route_shape_is_a_bad_request_naming_what_is_missing() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  let (status, answer) = call(&router, "POST", "/room/owns", Some(json!({}))).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")));
  assert!(answer["error"].as_str().unwrap().contains("missing field `chatId`"), "{answer}");
  let (status, answer) = call(&router, "POST", "/dialect/read", Some(json!({ "markdown": "hi" }))).await;
  assert_eq!(status, 400);
  assert!(answer["error"].as_str().unwrap().contains("missing field `text`"), "{answer}");
  let (status, answer) =
    call(&router, "POST", "/send", Some(json!({ "verb": "shout", "chatId": ROOM }))).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")));
  let (status, answer) = call(&router, "POST", "/room/title", None).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")));
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn a_declared_send_reaches_the_provider() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  assert_eq!(
    call(&router, "POST", "/send", text()).await,
    (200, json!({ "sent": true, "messageId": 40 }))
  );
  assert_eq!(desk.asked(), ["send Text to -100"]);
}

#[tokio::test]
async fn a_send_carrying_a_large_file_is_taken_whole() {
  let desk = Desk::new(&[ChatCapability::Document]);
  let router = router(&desk);
  let bytes = vec![7u8; 5 * 1024 * 1024];
  let send = json!({
    "verb": "document", "chatId": ROOM, "bytes": STANDARD.encode(&bytes), "fileName": "large.bin",
  });
  assert_eq!(
    call(&router, "POST", "/send", Some(send)).await,
    (200, json!({ "sent": true, "messageId": 40 }))
  );
  assert_eq!(desk.asked(), [format!("send a document of {} bytes", bytes.len())]);
}

#[tokio::test]
async fn an_undeclared_send_is_refused_before_the_provider_is_asked() {
  let desk = Desk::new(&[ChatCapability::Text]);
  let router = router(&desk);
  let pin = json!({ "verb": "pin", "chatId": ROOM, "messageId": 5, "on": true });
  assert_eq!(
    call(&router, "POST", "/send", Some(pin)).await,
    (
      200,
      json!({
        "sent": false, "gone": false,
        "refusal": {
          "code": "unsupported-capability", "label": "Platform", "capability": "pin",
          "message": desk.unsupported(ChatCapability::Pin).to_string(),
        },
      })
    )
  );
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn a_provider_that_fails_answers_failed() {
  let desk = Desk::new(&[ChatCapability::Text, ChatCapability::FetchMedia]);
  desk.fails();
  let router = router(&desk);
  let failed = json!({ "code": "failed", "error": "The platform connection dropped." });
  for (method, path, body) in routes() {
    if matches!(path, "/declaration" | "/health" | "/dialect/read" | "/dialect/check") {
      continue;
    }
    assert_eq!(call(&router, method, path, body).await, (500, failed.clone()), "{method} {path}");
  }
  assert_eq!(desk.asked().len(), 8, "{:?}", desk.asked());
}

#[tokio::test]
async fn media_is_refused_where_undeclared_and_fetched_where_declared() {
  let request = Some(json!({ "chatId": ROOM, "messageId": 5, "sentAt": 1_760_000_000_000_i64 }));
  let undeclared = Desk::new(&[ChatCapability::Text]);
  assert_eq!(
    call(&router(&undeclared), "POST", "/media", request.clone()).await,
    (
      200,
      json!({
        "outcome": "refused",
        "refusal": {
          "code": "unsupported-capability", "label": "Platform", "capability": "fetchMedia",
          "message": "Platform cannot fetch a picture or a file later.",
        },
      })
    )
  );
  assert_eq!(undeclared.asked(), Vec::<String>::new());
  let declared = Desk::new(&[ChatCapability::FetchMedia]);
  assert_eq!(
    call(&router(&declared), "POST", "/media", request).await,
    (
      200,
      json!({ "outcome": "fetched", "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=" })
    )
  );
  assert_eq!(declared.asked(), ["media -100 5"]);
}

#[tokio::test]
async fn the_dialect_routes_answer_from_the_dialect() {
  let router = router(&Desk::new(&[]));
  assert_eq!(
    call(&router, "POST", "/dialect/read", Some(json!({ "text": "first part\n\nsecond" }))).await,
    (
      200,
      json!({
        "markup": "FIRST PART\n\nSECOND", "plain": "first part\n\nsecond", "formatted": true,
        "parts": ["first part", "second"], "fold": "[folded] first part\n\nsecond",
      })
    )
  );
  assert_eq!(
    call(&router, "POST", "/dialect/read", Some(json!({ "text": "short" }))).await,
    (
      200,
      json!({ "markup": "SHORT", "plain": "short", "formatted": true, "parts": ["short"], "fold": null })
    )
  );
  assert_eq!(
    call(&router, "POST", "/dialect/check", Some(json!({ "markup": "<B Open" }))).await,
    (200, json!({ "fault": "A tag is left open.", "visible": "<b open" }))
  );
  assert_eq!(
    call(&router, "POST", "/dialect/check", Some(json!({ "markup": "<B>Shut</B>" }))).await,
    (200, json!({ "fault": null, "visible": "<b>shut</b>" }))
  );
}

#[tokio::test]
async fn every_room_route_answers_from_the_provider() {
  let desk = Desk::new(&[]);
  let router = router(&desk);
  let elsewhere = Some(json!({ "chatId": 9 }));
  let asked = [
    ("/room/owns", json!({ "owns": true }), json!({ "owns": false })),
    ("/room/identity", json!({ "identity": 7 }), json!({ "identity": null })),
    ("/room/is-owner", json!({ "kind": "owner" }), json!({ "kind": "group" })),
    ("/room/private-of", json!({ "person": OWNER }), json!({ "person": null })),
    ("/room/title", json!({ "title": "A Room" }), json!({ "title": null })),
  ];
  for (path, here, there) in asked {
    assert_eq!(call(&router, "POST", path, room()).await, (200, here), "{path}");
    assert_eq!(call(&router, "POST", path, elsewhere.clone()).await, (200, there), "{path}");
  }
  assert_eq!(call(&router, "GET", "/owner-room", None).await, (200, json!({ "chatId": ROOM })));
  assert_eq!(
    desk.asked(),
    [
      "owns -100",
      "owns 9",
      "identity -100",
      "identity 9",
      "is-owner -100",
      "is-owner 9",
      "private-of -100",
      "private-of 9",
      "title -100",
      "title 9",
      "owner-room",
    ]
  );
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
  assert_eq!(call(&router, "GET", "/avatars", None).await, (200, json!({ "avatars": [] })));
  assert_eq!(
    call(&router, "GET", "/avatars/room/-100", None).await,
    (
      404,
      json!({ "code": "nothing-held", "error": "No profile picture is kept for room -100." })
    )
  );
  assert_eq!(desk.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn the_kept_pictures_are_listed_oldest_first_from_the_cursor_and_never_parted_at_one_moment() {
  let desk = Desk::new(&[ChatCapability::Avatars]);
  let router = router(&desk);
  let listed = async |query: &str| {
    let (status, answer) = call(&router, "GET", &format!("/avatars{query}"), None).await;
    assert_eq!(status, 200, "{query}: {answer}");
    let subjects: Vec<String> = answer["avatars"]
      .as_array()
      .unwrap()
      .iter()
      .map(|avatar| format!("{} {} {}", avatar["kind"].as_str().unwrap(), avatar["id"], avatar["takenAt"]))
      .collect();
    subjects
  };
  assert_eq!(listed("").await, ["room -100 1000", "room 9 2000", "person 7 2000", "person 42 3000"]);
  assert_eq!(listed("?after=1000").await, ["room 9 2000", "person 7 2000", "person 42 3000"]);
  assert_eq!(listed("?after=1000&limit=1").await, ["room 9 2000", "person 7 2000"]);
  assert_eq!(listed("?limit=1").await, ["room -100 1000"]);
  assert_eq!(listed("?limit=0").await, Vec::<String>::new());
  assert_eq!(listed("?after=3000").await, Vec::<String>::new());
  let (status, answer) = call(&router, "GET", "/avatars?after=yesterday", None).await;
  assert_eq!((status, &answer["code"]), (400, &json!("bad-request")), "{answer}");
  assert_eq!(desk.asked().len(), 6);
}

#[tokio::test]
async fn a_kept_picture_is_answered_as_its_bytes_and_a_missing_one_as_nothing_held() {
  let desk = Desk::new(&[ChatCapability::Avatars]);
  desk.unconfigured("No token is configured.");
  let router = router(&desk);
  assert_eq!(
    fetch(&router, "/avatars/room/-100").await,
    (200, Some("image/jpeg".into()), vec![0xff, 0xd8, 0xff]),
    "a kept picture is served whatever state the provider is in"
  );
  assert_eq!(
    call(&router, "GET", "/avatars/person/7", None).await,
    (
      404,
      json!({ "code": "nothing-held", "error": "No profile picture is kept for person 7." })
    )
  );
  for path in ["/avatars/somebody/7", "/avatars/room/seven"] {
    assert_eq!(
      call(&router, "GET", path, None).await,
      (404, json!({ "code": "no-route", "error": format!("There is no route at GET {path}.") })),
      "{path}"
    );
  }
  assert_eq!(desk.asked(), ["avatar room -100", "avatar person 7"]);
  desk.fails();
  let failed = json!({ "code": "failed", "error": "The platform connection dropped." });
  assert_eq!(call(&router, "GET", "/avatars/room/-100", None).await, (500, failed.clone()));
  assert_eq!(call(&router, "GET", "/avatars", None).await, (500, failed));
}

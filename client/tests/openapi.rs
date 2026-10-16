//! The OpenAPI document: the committed `docs/openapi.json` is what the code generates, it lists exactly the
//! routes the client calls and the receiver serves, and every body this crate writes is valid against the
//! schema the document gives its route.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::{Method, Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use bmo_chat_client::{
  AlbumItem, ChatCapability, ChatDeclaration, ChatDialectDeclaration, ChatDialectLimits, ChatGathered,
  ChatHandover, ChatHealth, ChatMarkup, ChatMediaRequest, ChatMediaResult, ChatMessage, ChatModeStanding,
  ChatNumberSpace, ChatNumbering, ChatProviderClient, ChatReach, ChatReadMode, ChatReceiver, ChatRefusal,
  ChatSend, ChatSendRequest, ChatSendResult, ChatSpeaker, ChatVerb, ChatVoice, DialectCheck,
  DialectChecked, DialectRead, DialectReading, ErrorCode, FileItem, GatheredOutcome, InboundChatContext,
  MissedPicture, MissedPictureReason, OwnerRoom, RoomFacts, RoomIdentity, RoomKind, RoomKindAnswer,
  RoomOwns, RoomPerson, RoomQuery, RoomTitle, WireError, openapi, openapi_json, receiver_router,
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower::ServiceExt;

const COMMITTED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/openapi.json");
const TOKEN: &str = "the-token";

#[test]
fn the_committed_document_is_what_the_code_generates() {
  let committed = std::fs::read_to_string(COMMITTED).expect("docs/openapi.json is readable");
  let generated = openapi_json();
  if committed == generated {
    return;
  }
  let differs = committed
    .lines()
    .zip(generated.lines())
    .position(|(committed, generated)| committed != generated)
    .unwrap_or_else(|| committed.lines().count().min(generated.lines().count()));
  panic!(
    "docs/openapi.json is not what the code generates; write it again with \
     `cargo run -p bmo-chat-client --example openapi > docs/openapi.json`. They first differ at line {}: \
     committed {:?}, generated {:?}.",
    differs + 1,
    committed.lines().nth(differs),
    generated.lines().nth(differs),
  );
}

/// Every route of the document with the given tag, as its method in capitals and its path.
fn routes_tagged(tag: &str) -> BTreeSet<(String, String)> {
  let document = openapi();
  let mut routes = BTreeSet::new();
  for (path, item) in document["paths"].as_object().unwrap() {
    for (method, operation) in item.as_object().unwrap() {
      if operation["tags"] == json!([tag]) {
        routes.insert((method.to_uppercase(), path.clone()));
      }
    }
  }
  routes
}

/// The requests a stand-in provider was sent, each by its method and path.
type Heard = Arc<Mutex<Vec<(String, String)>>>;

/// Keeps every request a stand-in provider is sent, and answers each `no-route`.
async fn heard(State(heard): State<Heard>, method: Method, uri: Uri) -> Response {
  heard.lock().unwrap().push((method.to_string(), uri.path().to_owned()));
  let error = WireError::new(ErrorCode::NoRoute, "Nothing is served here.");
  (StatusCode::NOT_FOUND, axum::Json(error)).into_response()
}

#[tokio::test]
async fn the_provider_routes_are_exactly_those_the_client_calls() {
  let calls = Arc::new(Mutex::new(Vec::new()));
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let router = Router::new().fallback(heard).with_state(calls.clone());
  let (stop, stopped) = oneshot::channel::<()>();
  let served = tokio::spawn(async move {
    axum::serve(listener, router)
      .with_graceful_shutdown(async {
        stopped.await.ok();
      })
      .await
      .expect("the stand-in runs");
  });

  let client = ChatProviderClient::new(format!("http://{address}"), TOKEN);
  let send = ChatSendRequest::new(-100, ChatSend::Typing { on: true });
  let media = ChatMediaRequest {
    chat_id: -100,
    message_id: 5,
    sent_at: None,
  };
  let failures = [
    client.declaration().await.err(),
    client.health().await.err(),
    client.send(&send).await.err(),
    client.media(&media).await.err(),
    client.dialect_read("*a*").await.err(),
    client.dialect_check("<b>a</b>").await.err(),
    client.room_owns(-100).await.err(),
    client.room_identity(-100).await.err(),
    client.room_kind(-100).await.err(),
    client.room_private_of(-100).await.err(),
    client.room_title(-100).await.err(),
    client.owner_room().await.err(),
  ];
  stop.send(()).ok();
  served.await.expect("the stand-in stops cleanly");

  assert!(failures.iter().all(Option::is_some), "the stand-in serves nothing: {failures:?}");
  let called: BTreeSet<(String, String)> = calls.lock().unwrap().iter().cloned().collect();
  assert_eq!(called, routes_tagged("provider"));
}

struct Takes;

#[async_trait]
impl ChatReceiver for Takes {
  async fn message(&self, _: ChatHandover) -> Result<(), WireError> {
    Ok(())
  }
  async fn attachment(&self, _: ChatHandover) -> Result<(), WireError> {
    Ok(())
  }
  async fn edited(&self, _: ChatHandover) -> Result<(), WireError> {
    Ok(())
  }
  async fn gathered(&self, _: ChatGathered) -> Result<(), WireError> {
    Ok(())
  }
}

#[tokio::test]
async fn every_receiver_route_of_the_document_is_served() {
  let router = receiver_router(Arc::new(Takes), TOKEN);
  let routes = routes_tagged("receiver");
  assert_eq!(routes.len(), 4);
  for (method, path) in routes {
    let request = Request::builder()
      .method(method.as_str())
      .uri(&path)
      .header("authorization", format!("Bearer {TOKEN}"))
      .header("content-type", "application/json")
      .body(Body::from("{}"))
      .unwrap();
    let answer = router.clone().oneshot(request).await.unwrap();
    let status = answer.status().as_u16();
    let body: Value =
      serde_json::from_slice(&to_bytes(answer.into_body(), usize::MAX).await.unwrap()).unwrap();
    // An empty object is no route's body, so a served route reads it and refuses it as a bad request.
    assert_eq!((status, &body["code"]), (400, &json!("bad-request")), "{method} {path}");
  }
}

/// Which part of a route a body is.
#[derive(Clone, Copy, Debug)]
enum Part {
  Request,
  Answer,
}

/// A validator for one part of one route, with the document's schemas for its references.
fn validator(document: &Value, method: &str, path: &str, part: Part) -> jsonschema::Validator {
  let operation = &document["paths"][path][method];
  let schema = match part {
    Part::Request => &operation["requestBody"]["content"]["application/json"]["schema"],
    Part::Answer => &operation["responses"]["200"]["content"]["application/json"]["schema"],
  };
  assert!(schema.is_object(), "the document gives {method} {path} a {part:?} schema");
  jsonschema::validator_for(&json!({
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "allOf": [schema],
    "components": document["components"],
  }))
  .unwrap_or_else(|error| panic!("the {part:?} schema of {method} {path} compiles: {error}"))
}

/// The bodies this crate writes, each with the route and part it travels as.
struct Bodies {
  document: Value,
  checked: BTreeMap<(String, String), usize>,
}

impl Bodies {
  fn new() -> Self {
    Self {
      document: openapi(),
      checked: BTreeMap::new(),
    }
  }

  fn valid(&mut self, method: &str, path: &str, part: Part, body: &impl Serialize) {
    let body = serde_json::to_value(body).unwrap();
    let validator = validator(&self.document, method, path, part);
    let errors: Vec<String> = validator.iter_errors(&body).map(|error| error.to_string()).collect();
    assert!(errors.is_empty(), "{method} {path} {part:?} {body}: {errors:?}");
    *self.checked.entry((method.to_owned(), path.to_owned())).or_default() += 1;
  }

  fn invalid(&self, method: &str, path: &str, part: Part, body: Value) {
    let validator = validator(&self.document, method, path, part);
    assert!(!validator.is_valid(&body), "{method} {path} {part:?} takes {body}");
  }
}

fn file() -> FileItem {
  FileItem {
    name: "a.png".into(),
    mime: "image/png".into(),
    bytes: vec![137, 80, 78, 71],
  }
}

/// One send of every verb, with every optional field set on some of them.
fn every_send() -> Vec<ChatSendRequest> {
  let sends = [
    ChatSend::Text {
      text: "Hello.".into(),
      reply_to: Some(4),
      silent: true,
      fold: true,
    },
    ChatSend::Photo {
      image_url: "https://example.org/a.png".into(),
      caption: Some("A picture.".into()),
      reply_to: None,
    },
    ChatSend::Album {
      items: vec![
        AlbumItem::Address {
          address: "https://example.org/a.png".into(),
        },
        AlbumItem::File(file()),
      ],
      caption: None,
      reply_to: Some(4),
    },
    ChatSend::Document {
      bytes: vec![1, 2, 3],
      file_name: "a.txt".into(),
      reply_to: None,
    },
    ChatSend::Audio {
      audio: vec![1, 2, 3],
      file_name: "a.ogg".into(),
      reply_to: Some(4),
    },
    ChatSend::Voice {
      audio: vec![1, 2, 3],
      said: "Hello.".into(),
      caption: Some("A line.".into()),
      transcript: Some("Hello.".into()),
      reply_to: None,
    },
    ChatSend::Sticker {
      emoji: "👍".into()
    },
    ChatSend::Reaction {
      message_id: 4,
      emoji: String::new(),
    },
    ChatSend::Pin {
      message_id: 4,
      on: true,
      silent: true,
    },
    ChatSend::Typing { on: false },
    ChatSend::Edit {
      message_id: 4,
      text: "Hello again.".into(),
    },
    ChatSend::Delete { message_id: 4 },
  ];
  let verbs: BTreeSet<ChatVerb> = sends.iter().map(ChatSend::verb).collect();
  assert_eq!(verbs, ChatVerb::ALL.iter().copied().collect());
  sends
    .into_iter()
    .enumerate()
    .map(|(at, send)| {
      let mut request = ChatSendRequest::new(-100, send);
      if at % 2 == 0 {
        request.voice = Some(ChatVoice::HumanReader);
        request.speaker = Some(ChatSpeaker {
          agent: "helper".into(),
        });
        request.tag = Some("echo-1".into());
      }
      request
    })
    .collect()
}

fn every_refusal() -> Vec<ChatRefusal> {
  let label = || "The Platform".to_owned();
  let missed = [
    MissedPictureReason::NotAnAddress,
    MissedPictureReason::OutsideDataRoot,
    MissedPictureReason::MissingFile,
    MissedPictureReason::Unfetched,
    MissedPictureReason::Platform {
      detail: "The picture is too large.".into(),
    },
  ]
  .into_iter()
  .map(|reason| MissedPicture {
    address: "/nowhere.png".into(),
    reason,
  })
  .collect();
  vec![
    ChatRefusal::UnsupportedCapability {
      label: label(),
      capability: ChatCapability::FetchMedia,
    },
    ChatRefusal::NoPictureSent {
      label: label(),
      missed,
    },
    ChatRefusal::SpeakerNotAllowed {
      label: label(),
      agent: "helper".into(),
    },
    ChatRefusal::SpeakerUnnumbered {
      label: label(),
      agent: "helper".into(),
    },
    ChatRefusal::NotReady { label: label() },
    ChatRefusal::Platform {
      label: label(),
      detail: "The room is closed.".into(),
    },
  ]
}

fn declaration() -> ChatDeclaration {
  ChatDeclaration {
    key: "stand-in".into(),
    label: "The Platform".into(),
    configured: None,
    modes: vec![
      ChatReadMode {
        voice: ChatVoice::BotAccount,
        reach: ChatReach::AllMessages,
        standing: ChatModeStanding::Active,
        reason: None,
        can_reply_to: true,
      },
      ChatReadMode {
        voice: ChatVoice::HumanReader,
        reach: ChatReach::MentionsOnly,
        standing: ChatModeStanding::Inactive,
        reason: Some("No reader account is configured.".into()),
        can_reply_to: false,
      },
    ],
    can: ChatCapability::ALL.iter().copied().collect(),
    dialect: ChatDialectDeclaration {
      name: "plain".into(),
      limits: ChatDialectLimits {
        text: Some(4096),
        file_bytes: Some(1 << 20),
        ..ChatDialectLimits::default()
      },
      writing: "Marks are sent as they are.".into(),
      foldable: false,
    },
    shows: Some("Profile pictures.".into()),
    numbering: ChatNumbering {
      rooms: ChatNumberSpace::Native,
      people: ChatNumberSpace::Assigned,
      messages: ChatNumberSpace::None,
    },
    speakers: true,
  }
}

fn message() -> ChatMessage {
  ChatMessage {
    chat_id: -100,
    message_id: 5,
    sender_id: None,
    sender_name: "A Person".into(),
    sent_at: 1_760_000_000,
    text: "said".into(),
    reply_to_message_id: Some(4),
    via: "stand-in".into(),
    album: None,
    mode: ChatVoice::HumanAvatar,
  }
}

#[test]
fn every_body_the_crate_writes_is_valid_against_the_schema_of_its_route() {
  use Part::{Answer, Request};
  let mut bodies = Bodies::new();

  bodies.valid("get", "/declaration", Answer, &declaration());
  bodies.valid("get", "/health", Answer, &ChatHealth::ready());
  bodies.valid("get", "/health", Answer, &ChatHealth::not_ready("The connection is not open yet."));

  for send in every_send() {
    bodies.valid("post", "/send", Request, &send);
  }
  bodies.valid(
    "post",
    "/send",
    Answer,
    &ChatSendResult::Sent {
      message_id: Some(9),
      missed: vec![],
    },
  );
  bodies.valid(
    "post",
    "/send",
    Answer,
    &ChatSendResult::Sent {
      message_id: None,
      missed: vec![MissedPicture {
        address: "/nowhere.png".into(),
        reason: MissedPictureReason::MissingFile,
      }],
    },
  );
  for (at, refusal) in every_refusal().into_iter().enumerate() {
    let refused = ChatSendResult::Refused {
      refusal: refusal.clone(),
      gone: at % 2 == 0,
    };
    bodies.valid("post", "/send", Answer, &refused);
    bodies.valid("post", "/media", Answer, &ChatMediaResult::Refused { refusal });
  }

  for sent_at in [None, Some(1_760_000_000)] {
    let request = ChatMediaRequest {
      chat_id: -100,
      message_id: 5,
      sent_at,
    };
    bodies.valid("post", "/media", Request, &request);
  }
  bodies.valid("post", "/media", Answer, &ChatMediaResult::Fetched(file()));
  bodies.valid(
    "post",
    "/media",
    Answer,
    &ChatMediaResult::Unavailable {
      reason: "The message is too old.".into(),
    },
  );

  bodies.valid("post", "/dialect/read", Request, &DialectRead { text: "*a*".into() });
  for fold in [None, Some("a".to_owned())] {
    let reading = DialectReading {
      markup: ChatMarkup {
        markup: "<i>a</i>".into(),
        plain: "a".into(),
        formatted: true,
      },
      parts: vec!["*a*".into()],
      fold,
    };
    bodies.valid("post", "/dialect/read", Answer, &reading);
  }
  bodies.valid(
    "post",
    "/dialect/check",
    Request,
    &DialectCheck {
      markup: "<i>a</i>".into(),
    },
  );
  for fault in [None, Some("An unclosed mark.".to_owned())] {
    let checked = DialectChecked {
      fault,
      visible: "a".into(),
    };
    bodies.valid("post", "/dialect/check", Answer, &checked);
  }

  let query = RoomQuery { chat_id: -100 };
  for path in ["/room/owns", "/room/identity", "/room/is-owner", "/room/private-of", "/room/title"] {
    bodies.valid("post", path, Request, &query);
  }
  bodies.valid("post", "/room/owns", Answer, &RoomOwns { owns: true });
  for identity in [None, Some(7)] {
    bodies.valid("post", "/room/identity", Answer, &RoomIdentity { identity });
    bodies.valid("post", "/room/private-of", Answer, &RoomPerson { person: identity });
    bodies.valid("get", "/owner-room", Answer, &OwnerRoom { chat_id: identity });
  }
  for kind in [RoomKind::Owner, RoomKind::Group] {
    bodies.valid("post", "/room/is-owner", Answer, &RoomKindAnswer { kind });
  }
  for title in [None, Some("A room".to_owned())] {
    bodies.valid("post", "/room/title", Answer, &RoomTitle { title });
  }

  let bare = ChatHandover {
    message: message(),
    context: None,
    room: None,
    files: vec![],
  };
  let full = ChatHandover {
    message: message(),
    context: Some(InboundChatContext {
      room_title: Some("A room".into()),
      sender_username: Some("a_person".into()),
      reply_to_sender_id: Some(42),
      spoken_text: Some("said aloud".into()),
      metadata: Some(json!({ "anything": [1, "two"] })),
      carries_file: true,
      gathering: true,
      tag: Some("echo-1".into()),
    }),
    room: Some(RoomFacts {
      kind: Some(RoomKind::Group),
      identity: Some(7),
      private_to: Some(42),
    }),
    files: vec![file()],
  };
  for path in ["/message", "/attachment", "/edited"] {
    bodies.valid("post", path, Request, &bare);
    bodies.valid("post", path, Request, &full);
  }
  let gathered = |outcome| ChatGathered {
    chat_id: -100,
    message_id: 5,
    via: "stand-in".into(),
    outcome,
  };
  bodies.valid(
    "post",
    "/gathered",
    Request,
    &gathered(GatheredOutcome::Kept {
      files: vec![file()],
    }),
  );
  bodies.valid(
    "post",
    "/gathered",
    Request,
    &gathered(GatheredOutcome::NotKept {
      reason: "The file was too large to keep.".into(),
    }),
  );
  for path in ["/message", "/attachment", "/edited", "/gathered"] {
    bodies.valid("post", path, Answer, &json!({}));
  }

  let routes: BTreeSet<(String, String)> = bodies.checked.keys().cloned().collect();
  let documented: BTreeSet<(String, String)> = routes_tagged("provider")
    .into_iter()
    .chain(routes_tagged("receiver"))
    .map(|(method, path)| (method.to_lowercase(), path))
    .collect();
  assert_eq!(routes, documented, "a body was checked for every route");
}

#[test]
fn a_body_not_of_its_route_shape_is_not_valid() {
  use Part::{Answer, Request};
  let bodies = Bodies::new();
  bodies.invalid("post", "/send", Request, json!({ "verb": "text", "text": "Hello." }));
  bodies.invalid("post", "/send", Request, json!({ "chatId": -100, "verb": "shout", "text": "Hello." }));
  bodies.invalid("post", "/send", Request, json!({ "chatId": -100, "verb": "document", "fileName": "a" }));
  bodies.invalid("post", "/send", Answer, json!({ "sent": false, "gone": false }));
  bodies.invalid(
    "post",
    "/send",
    Answer,
    json!({ "sent": false, "gone": false, "refusal": { "code": "not-ready", "label": "The Platform" } }),
  );
  bodies.invalid(
    "post",
    "/media",
    Answer,
    json!({ "outcome": "fetched", "name": "a.png", "mime": "image/png" }),
  );
  bodies.invalid("get", "/declaration", Answer, json!({ "key": "stand-in" }));
  bodies.invalid("post", "/room/is-owner", Answer, json!({ "kind": "channel" }));
  bodies.invalid(
    "post",
    "/gathered",
    Request,
    json!({ "chatId": -100, "messageId": 5, "via": "stand-in" }),
  );
  bodies.invalid("post", "/message", Answer, json!({ "taken": true }));
}

/// Every reference the value makes, wherever it stands in it.
fn references(value: &Value, found: &mut BTreeSet<String>) {
  match value {
    Value::Object(object) => {
      if let Some(Value::String(reference)) = object.get("$ref") {
        found.insert(reference.clone());
      }
      object.values().for_each(|value| references(value, found));
    }
    Value::Array(values) => values.iter().for_each(|value| references(value, found)),
    _ => {}
  }
}

#[test]
fn every_reference_of_the_document_points_at_what_it_holds() {
  let document = openapi();
  let mut found = BTreeSet::new();
  references(&document, &mut found);
  assert!(!found.is_empty());
  for reference in found {
    let pointer = reference.strip_prefix('#').expect("every reference is within the document");
    assert!(document.pointer(pointer).is_some(), "{reference} points at nothing");
  }
}

#[test]
fn every_schema_of_the_document_is_a_json_schema() {
  let document = openapi();
  let schemas = document["components"]["schemas"].as_object().unwrap();
  for (name, schema) in schemas {
    let mut schema = schema.clone();
    schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    jsonschema::meta::validate(&schema).unwrap_or_else(|error| panic!("{name}: {error}"));
  }
}

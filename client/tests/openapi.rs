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
  AlbumItem, AvatarList, AvatarsQuery, AvatarsUpdated, ChatAvatar, ChatAvatarKind, ChatCapability,
  ChatDeclaration, ChatDialectDeclaration, ChatDialectLimits, ChatFileKind, ChatGathered, ChatHandover,
  ChatHealth, ChatMarkup, ChatMediaRequest, ChatMessage, ChatModeStanding, ChatNumberRange,
  ChatNumberRanges, ChatNumberSpace, ChatNumbering, ChatProviderClient, ChatReach, ChatReadMode,
  ChatReceiver, ChatSend, ChatSendRequest, ChatSent, ChatSpeaker, ChatVerb, ChatVoice, DialectCheck,
  DialectChecked, DialectRead, DialectReading, ErrorCode, FileItem, GatheredOutcome, HistoryKept,
  HistoryRoom, InboundChatContext, Listened, ListenedQuery, MessageFile, MissedPicture,
  MissedPictureReason, OwnerRoom, RoomFacts, RoomIdentity, RoomKind, RoomKindAnswer, RoomOwns, RoomPerson,
  RoomQuery, RoomTitle, RoomsOf, RoomsOfQuery, WireError, openapi, openapi_json, receiver_router,
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
  let listed = AvatarsQuery {
    after: Some(1_760_000_000_000),
    limit: Some(10),
  };
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
    client.avatars(&listed).await.err(),
    client.avatar(ChatAvatarKind::Room, -100).await.err(),
  ];
  stop.send(()).ok();
  served.await.expect("the stand-in stops cleanly");

  assert!(failures.iter().all(Option::is_some), "the stand-in serves nothing: {failures:?}");
  let documented = routes_tagged("provider");
  let called: BTreeSet<(String, String)> = calls
    .lock()
    .unwrap()
    .iter()
    .map(|(method, path)| (method.clone(), template_of(path, &documented)))
    .collect();
  assert_eq!(called, documented);
}

/// The document's path a called path matches, where a `{name}` segment matches any one segment; the called
/// path itself where none does.
fn template_of(path: &str, documented: &BTreeSet<(String, String)>) -> String {
  let segments: Vec<&str> = path.split('/').collect();
  documented
    .iter()
    .map(|(_, template)| template)
    .find(|template| {
      let parts: Vec<&str> = template.split('/').collect();
      parts.len() == segments.len()
        && parts
          .iter()
          .zip(&segments)
          .all(|(part, segment)| part.starts_with('{') || part == segment)
    })
    .cloned()
    .unwrap_or_else(|| path.to_owned())
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
  async fn history(&self, _: ChatHandover) -> Result<bool, WireError> {
    Ok(true)
  }
  async fn avatars_updated(&self, _: AvatarsUpdated) -> Result<(), WireError> {
    Ok(())
  }
  async fn listened(&self, _: ListenedQuery) -> Result<bool, WireError> {
    Ok(true)
  }
  async fn rooms_of(&self, _: RoomsOfQuery) -> Result<Vec<HistoryRoom>, WireError> {
    Ok(Vec::new())
  }
}

#[tokio::test]
async fn every_receiver_route_of_the_document_is_served() {
  let router = receiver_router(Arc::new(Takes), TOKEN);
  let routes = routes_tagged("receiver");
  assert_eq!(routes.len(), 8);
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

  /// A route whose answer is bytes of a type the document names, not JSON.
  fn bytes(&mut self, method: &str, path: &str, mime: &str) {
    let content = &self.document["paths"][path][method]["responses"]["200"]["content"];
    assert!(content[mime].is_object(), "{method} {path} answers {mime}: {content}");
    assert!(content.get("application/json").is_none(), "{method} {path} answers no JSON");
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

fn message_file() -> MessageFile {
  MessageFile {
    id: "file-1".into(),
    name: "a.png".into(),
    mime: "image/png".into(),
    bytes: vec![137, 80, 78, 71],
    kind: Some(ChatFileKind::Photo),
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

fn every_missed_picture() -> Vec<MissedPicture> {
  [
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
  .collect()
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
      ranges: ChatNumberRanges {
        rooms: None,
        people: Some(ChatNumberRange {
          first: 1_000_001,
          last: 2_000_000,
        }),
      },
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
    sent_at: 1_760_000_000_000,
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
    &ChatSent {
      message_id: Some(9),
      missed: vec![],
    },
  );
  bodies.valid(
    "post",
    "/send",
    Answer,
    &ChatSent {
      message_id: None,
      missed: every_missed_picture(),
    },
  );

  for sent_at in [None, Some(1_760_000_000_000)] {
    let request = ChatMediaRequest {
      chat_id: -100,
      message_id: 5,
      sent_at,
    };
    bodies.valid("post", "/media", Request, &request);
  }
  bodies.valid("post", "/media", Answer, &message_file());

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
    files: vec![message_file()],
  };
  for path in ["/message", "/attachment", "/edited", "/history"] {
    bodies.valid("post", path, Request, &bare);
    bodies.valid("post", path, Request, &full);
  }
  for kept in [true, false] {
    bodies.valid("post", "/history", Answer, &HistoryKept { kept });
  }
  let updated = AvatarsUpdated {
    via: "stand-in".into(),
    rooms: vec![-100],
    people: vec![],
  };
  bodies.valid("post", "/avatars/updated", Request, &updated);
  let listened = ListenedQuery {
    chat_id: -100,
    via: "stand-in".into(),
  };
  bodies.valid("post", "/rooms/listened", Request, &listened);
  bodies.valid("post", "/rooms/listened", Answer, &Listened { listened: false });
  for (after, limit) in [(None, None), (Some(-100), Some(50))] {
    let query = RoomsOfQuery {
      via: "stand-in".into(),
      after,
      limit,
    };
    bodies.valid("post", "/rooms/of", Request, &query);
  }
  let rooms = RoomsOf {
    rooms: vec![
      HistoryRoom {
        chat_id: -100,
        oldest_message_id: Some(5),
        oldest_sent_at: Some(1_760_000_000_000),
      },
      HistoryRoom {
        chat_id: 7,
        oldest_message_id: None,
        oldest_sent_at: None,
      },
    ],
  };
  bodies.valid("post", "/rooms/of", Answer, &rooms);
  let avatars = AvatarList {
    avatars: vec![ChatAvatar {
      kind: ChatAvatarKind::Person,
      id: 42,
      taken_at: 1_760_000_000_000,
    }],
  };
  bodies.valid("get", "/avatars", Answer, &avatars);
  bodies.bytes("get", "/avatars/{kind}/{id}", "image/*");
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
      files: vec![message_file()],
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
  for path in ["/message", "/attachment", "/edited", "/gathered", "/avatars/updated"] {
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
  bodies.invalid("post", "/send", Answer, json!({ "missed": [] }));
  bodies.invalid(
    "post",
    "/send",
    Answer,
    json!({ "messageId": 9, "missed": [{ "address": "/nowhere.png", "reason": { "code": "lost" } }] }),
  );
  bodies.invalid(
    "post",
    "/media",
    Answer,
    json!({ "id": "file-1", "name": "a.png", "mime": "image/png" }),
  );
  bodies.invalid(
    "post",
    "/media",
    Answer,
    json!({ "name": "a.png", "mime": "image/png", "bytes": "iVA=" }),
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
  bodies.invalid("post", "/history", Answer, json!({}));
  bodies.invalid("post", "/rooms/of", Answer, json!({ "rooms": [{ "chatId": -100 }] }));
  bodies.invalid("post", "/rooms/listened", Request, json!({ "chatId": -100 }));
  bodies.invalid(
    "get",
    "/avatars",
    Answer,
    json!({ "avatars": [{ "kind": "group", "id": 1, "takenAt": 1 }] }),
  );
}

#[test]
fn the_avatar_routes_state_their_parameters_and_what_they_hold() {
  let document = openapi();
  let list = &document["paths"]["/avatars"]["get"];
  let names = |operation: &Value| -> Vec<(String, String, bool)> {
    operation["parameters"]
      .as_array()
      .unwrap()
      .iter()
      .map(|parameter| {
        let name = parameter["name"].as_str().unwrap().to_owned();
        let place = parameter["in"].as_str().unwrap().to_owned();
        (name, place, parameter["required"].as_bool().unwrap())
      })
      .collect()
  };
  let owned = |name: &str, place: &str, required| (name.to_owned(), place.to_owned(), required);
  assert_eq!(names(list), [owned("after", "query", false), owned("limit", "query", false)]);
  assert!(list["responses"]["400"].is_object(), "a query not of its shape is a bad request");
  let picture = &document["paths"]["/avatars/{kind}/{id}"]["get"];
  assert_eq!(names(picture), [owned("kind", "path", true), owned("id", "path", true)]);
  assert_eq!(picture["responses"]["404"], json!({ "$ref": "#/components/responses/NothingHeld" }));
  assert!(list["responses"]["404"].is_null(), "a list holds nothing missing");
  for operation in [list, picture] {
    assert!(
      operation["responses"]["409"].is_null(),
      "what a provider keeps is answered in any state"
    );
    assert!(
      operation["responses"]["500"].is_object(),
      "a provider can fail while reading what it keeps"
    );
  }
}

/// The codes an answer of the document names: the one its reference stands for, or those its description
/// lists.
fn codes_of(document: &Value, answer: &Value) -> Vec<String> {
  let answer = match answer["$ref"].as_str() {
    Some(reference) => document.pointer(reference.trim_start_matches('#')).expect("the reference resolves"),
    None => answer,
  };
  assert_eq!(
    answer["content"]["application/json"]["schema"],
    json!({ "$ref": "#/components/schemas/WireError" })
  );
  let description = answer["description"].as_str().unwrap();
  description
    .split('`')
    .skip(1)
    .step_by(2)
    .filter(|code| !code.contains(' '))
    .map(str::to_owned)
    .collect()
}

/// A route's path, with each status it refuses at and the codes it names there.
type Refusing = (&'static str, &'static [(&'static str, &'static [&'static str])]);

#[test]
fn every_refusal_is_answered_at_its_status_in_the_error_shape() {
  let document = openapi();
  let refusals: &[Refusing] = &[
    (
      "/send",
      &[
        ("403", &["speaker-not-allowed"]),
        ("409", &["not-ready"]),
        ("410", &["gone"]),
        ("422", &["unsupported-capability", "no-picture-sent", "speaker-unnumbered", "platform"]),
      ],
    ),
    (
      "/media",
      &[
        ("404", &["unavailable"]),
        ("409", &["not-ready"]),
        ("410", &["gone"]),
        ("422", &["unsupported-capability", "platform"]),
      ],
    ),
  ];
  for (path, statuses) in refusals {
    let responses = &document["paths"][path]["post"]["responses"];
    for (status, codes) in *statuses {
      let named = codes_of(&document, &responses[status]);
      for code in *codes {
        assert!(named.iter().any(|named| named == code), "{path} {status} names `{code}`: {named:?}");
      }
    }
  }
  let refusal_codes = [
    "speaker-not-allowed",
    "gone",
    "unsupported-capability",
    "no-picture-sent",
    "unavailable",
  ];
  for path in ["/declaration", "/health", "/dialect/read", "/room/owns"] {
    let method = if path.starts_with("/room") || path.starts_with("/dialect") { "post" } else { "get" };
    let responses = document["paths"][path][method]["responses"].as_object().unwrap();
    for answer in responses.iter().filter(|(status, _)| *status != "200").map(|(_, answer)| answer) {
      let named = codes_of(&document, answer);
      assert!(
        named.iter().all(|code| !refusal_codes.contains(&code.as_str())),
        "{path} refuses nothing: {named:?}"
      );
    }
  }
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

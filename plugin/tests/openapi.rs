//! The OpenAPI documents: each committed document is what the code generates, the provider document lists
//! exactly the routes the client calls and the receiving-side document exactly those the router serves,
//! every body the client crate writes is valid against the schema of its route, and the tools stated under
//! `x-tools` are the thirteen the MCP server lists, each on a route of the document.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::State;
use axum::http::{Method, Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use bmo_chat_client::{
  AccountRoomsQuery, AlbumItem, AvatarChange, AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatContact,
  ChatContactKind, ChatDeclaration, ChatDialectDeclaration, ChatDialectLimits, ChatHealth, ChatMarkup,
  ChatMediaRequest, ChatMember, ChatMembers, ChatModeStanding, ChatNumberRanges, ChatNumberSpace,
  ChatNumbering, ChatProviderClient, ChatReach, ChatReadMode, ChatReceiver, ChatSend, ChatSendRequest,
  ChatSent, ChatStart, ChatStartRequest, ChatSticker, ChatVoice, ContactsQuery, DialectCheck,
  DialectChecked, DialectRead, DialectReading, ErrorCode, FileItem, GatheredPatch, HandedMessage,
  HeldMessage, InboundChatContext, MessageFile, MessageOrigin, MissedPicture, MissedPictureReason, Page,
  ProviderOwner, ProviderRoom, ReceiverRoom, RoomFacts, RoomKind, RoomQuery, RoomsQuery, StickerItem,
  WireError, receiver_router,
};
use bmo_chat_plugin::{Side, openapi, openapi_json, tools};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tower::ServiceExt;

const TOKEN: &str = "the-token";

/// The thirteen tools the protocol's harness plugin serves, in the order the protocol lists their routes.
const TOOLS: [&str; 13] = [
  "chat_declaration",
  "chat_health",
  "chat_send",
  "chat_rooms",
  "chat_start",
  "chat_contacts",
  "chat_room",
  "chat_members",
  "chat_owner",
  "chat_message_files",
  "chat_dialect_reading",
  "chat_dialect_check",
  "chat_avatars",
];

#[test]
fn each_committed_document_is_what_the_code_generates() {
  for (side, name) in [(Side::Provider, "provider"), (Side::Receiver, "receiver")] {
    let path = format!("{}/../docs/openapi/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    let generated = openapi_json(side);
    if committed == generated {
      continue;
    }
    let differs = committed
      .lines()
      .zip(generated.lines())
      .position(|(committed, generated)| committed != generated)
      .unwrap_or_else(|| committed.lines().count().min(generated.lines().count()));
    panic!(
      "docs/openapi/{name}.json is not what the code generates; write it again with \
       `cargo run -p bmo-chat-plugin --example openapi -- {name} > docs/openapi/{name}.json`. They first \
       differ at line {}: committed {:?}, generated {:?}.",
      differs + 1,
      committed.lines().nth(differs),
      generated.lines().nth(differs),
    );
  }
}

/// Every route of a side's document, as its method in capitals and its path.
fn routes(side: Side) -> BTreeSet<(String, String)> {
  let document = openapi(side);
  let mut routes = BTreeSet::new();
  for (path, item) in document["paths"].as_object().unwrap() {
    for method in item.as_object().unwrap().keys() {
      routes.insert((method.to_uppercase(), path.clone()));
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
    room_id: -100,
    message_id: 5,
    sent_at: None,
  };
  let failures = [
    client.declaration().await.err(),
    client.health().await.err(),
    client.send(&send).await.err(),
    client.account_rooms(&AccountRoomsQuery::default()).await.err(),
    client
      .start_chat(&ChatStartRequest::new(ChatStart::OneToOne { contact_id: 42 }))
      .await
      .err(),
    client.contacts(&ContactsQuery::default()).await.err(),
    client.room(-100).await.err(),
    client.members(-100).await.err(),
    client.owner().await.err(),
    client.message_files(&media).await.err(),
    client.dialect_reading("*a*").await.err(),
    client.dialect_check("<b>a</b>").await.err(),
    client.avatars(&AvatarsQuery::default()).await.err(),
    client.avatar(ChatAvatarKind::Room, -100).await.err(),
    client.avatar(ChatAvatarKind::Person, 42).await.err(),
  ];
  stop.send(()).ok();
  served.await.expect("the stand-in stops cleanly");

  assert!(failures.iter().all(Option::is_some), "the stand-in serves nothing: {failures:?}");
  let documented = routes(Side::Provider);
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

/// A receiving side that takes everything and holds nothing.
struct Takes;

#[async_trait]
impl ChatReceiver for Takes {
  async fn hand_over(&self, _: i64, _: i64, message: HandedMessage) -> Result<HeldMessage, WireError> {
    Ok(HeldMessage {
      created: true,
      message,
    })
  }
  async fn gathered(&self, _: i64, _: i64, _: GatheredPatch) -> Result<HandedMessage, WireError> {
    Err(WireError::new(ErrorCode::NotFound, "Nothing is held."))
  }
  async fn avatars_changed(&self, _: AvatarChange) -> Result<(), WireError> {
    Ok(())
  }
  async fn reactions_changed(&self, _: bmo_chat_client::ReactionChange) -> Result<(), WireError> {
    Ok(())
  }
  async fn deleted(&self, _: bmo_chat_client::Deletion) -> Result<(), WireError> {
    Ok(())
  }
  async fn room(&self, _: i64, _: RoomQuery) -> Result<Option<ReceiverRoom>, WireError> {
    Ok(None)
  }
  async fn rooms(&self, _: RoomsQuery) -> Result<Vec<ReceiverRoom>, WireError> {
    Ok(Vec::new())
  }
}

#[tokio::test]
async fn every_receiving_side_route_of_the_document_is_served() {
  let router = receiver_router(Arc::new(Takes), TOKEN);
  let routes = routes(Side::Receiver);
  assert_eq!(routes.len(), 9);
  for (method, path) in routes {
    let path = path.replace("{roomId}", "-100").replace("{messageId}", "5");
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
    // An empty object is no route's body and no `via` is given, so a served route reads them and refuses
    // them as a bad request.
    assert_eq!((status, &body["code"]), (400, &json!("bad-request")), "{method} {path}");
  }
}

/// Which part of a route a body is.
#[derive(Clone, Copy, Debug)]
enum Part {
  Request,
  Answer(u16),
}

/// A validator for one part of one route, with the document's schemas for its references.
fn validator(document: &Value, method: &str, path: &str, part: Part) -> jsonschema::Validator {
  let operation = &document["paths"][path][method];
  let schema = match part {
    Part::Request => &operation["requestBody"]["content"]["application/json"]["schema"],
    Part::Answer(status) => {
      &operation["responses"][status.to_string()]["content"]["application/json"]["schema"]
    }
  };
  assert!(schema.is_object(), "the document gives {method} {path} a {part:?} schema");
  jsonschema::validator_for(&json!({
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "allOf": [schema],
    "components": document["components"],
  }))
  .unwrap_or_else(|error| panic!("the {part:?} schema of {method} {path} compiles: {error}"))
}

/// The bodies the client crate writes, each with the route and part it travels as, on one side's document.
struct Bodies {
  document: Value,
  checked: BTreeMap<(String, String), usize>,
}

impl Bodies {
  fn new(side: Side) -> Self {
    Self {
      document: openapi(side),
      checked: BTreeMap::new(),
    }
  }

  fn valid(&mut self, method: &str, path: &str, part: Part, body: &impl Serialize) {
    let body = serde_json::to_value(body).unwrap();
    let validator = validator(&self.document, method, path, part);
    let errors: Vec<String> = validator.iter_errors(&body).map(|error| error.to_string()).collect();
    assert!(errors.is_empty(), "{method} {path} {part:?} {body}: {errors:?}");
    *self.checked.entry((method.to_uppercase(), path.to_owned())).or_default() += 1;
  }

  /// A route whose answer is bytes of a type the document names, not JSON.
  fn bytes(&mut self, method: &str, path: &str, mime: &str) {
    let content = &self.document["paths"][path][method]["responses"]["200"]["content"];
    assert!(content[mime].is_object(), "{method} {path} answers {mime}: {content}");
    assert!(content.get("application/json").is_none(), "{method} {path} answers no JSON");
    *self.checked.entry((method.to_uppercase(), path.to_owned())).or_default() += 1;
  }

  fn invalid(&self, method: &str, path: &str, part: Part, body: Value) {
    let validator = validator(&self.document, method, path, part);
    assert!(!validator.is_valid(&body), "{method} {path} {part:?} takes {body}");
  }

  /// Every route of the document had a body checked.
  fn every_route(self, side: Side) {
    let checked: BTreeSet<(String, String)> = self.checked.keys().cloned().collect();
    assert_eq!(checked, routes(side), "a body was checked for every route");
  }
}

fn message_file() -> MessageFile {
  MessageFile {
    id: "file-1".into(),
    name: "a.png".into(),
    mime: "image/png".into(),
    bytes: vec![137, 80].into(),
    kind: None,
  }
}

/// A send of every kind, and one with every field a send may carry.
fn every_send() -> Vec<ChatSendRequest> {
  let send = |send| ChatSendRequest::new(-100, send);
  vec![
    ChatSendRequest {
      mode: Some(ChatVoice::HumanAvatar),
      speaker: Some("helper".into()),
      tag: Some("echo-1".into()),
      prefix: Some("[helper] ".into()),
      reply_to_message_id: Some(4),
      ..send(ChatSend::Text {
        text: "Hello.".into(),
        silent: true,
        fold: true,
      })
    },
    send(ChatSend::Photo {
      image_url: "https://example.com/a.png".into(),
      caption: Some("A picture".into()),
    }),
    send(ChatSend::Album {
      items: vec![
        AlbumItem::Address {
          address: "/data/a.png".into(),
        },
        AlbumItem::File(FileItem {
          name: "b.png".into(),
          mime: "image/png".into(),
          bytes: vec![1, 2].into(),
        }),
      ],
      caption: None,
    }),
    send(ChatSend::Document {
      bytes: b"a document".to_vec().into(),
      file_name: "a.txt".into(),
    }),
    send(ChatSend::Audio {
      audio: vec![0xff, 0xfb].into(),
      file_name: "a.mp3".into(),
    }),
    send(ChatSend::Voice {
      audio: vec![1].into(),
      said: "Hello".into(),
      caption: None,
      transcript: Some("Hello!".into()),
    }),
    send(ChatSend::Sticker(ChatSticker::Emoji {
      emoji: "\u{1f44d}".into(),
    })),
    send(ChatSend::Sticker(ChatSticker::Item {
      item: StickerItem {
        id: "4412.emot_002.webp".into(),
        pack: Some("4412".into()),
        title: Some("Waving cat".into()),
        image_url: Some("https://example.com/4412/emot_002.webp".into()),
        thumbnail_url: Some("https://example.com/4412/emot_002_thumb.webp".into()),
      },
    })),
    send(ChatSend::Reaction {
      message_id: 4,
      emoji: String::new(),
    }),
    send(ChatSend::Pin {
      message_id: 4,
      on: true,
      silent: false,
    }),
    send(ChatSend::Typing { on: false }),
    send(ChatSend::Edit {
      message_id: 4,
      text: "Again.".into(),
    }),
    send(ChatSend::Delete { message_id: 4 }),
  ]
}

fn declaration() -> ChatDeclaration {
  ChatDeclaration {
    key: "stand-in".into(),
    label: "Stand-in".into(),
    configured: None,
    modes: vec![ChatReadMode {
      voice: ChatVoice::OwnAccount,
      reach: ChatReach::MentionsOnly,
      standing: ChatModeStanding::Inactive,
      reason: Some("No account is signed in.".into()),
      can_reply_to: false,
    }],
    can: bmo_chat_client::ChatCapability::ALL.iter().copied().collect(),
    dialect: ChatDialectDeclaration {
      name: "plain".into(),
      limits: ChatDialectLimits {
        text: Some(4096),
        ..ChatDialectLimits::default()
      },
      writing: "This room shows no formatting.".into(),
      foldable: false,
    },
    shows: None,
    numbering: ChatNumbering {
      rooms: ChatNumberSpace::Native,
      people: ChatNumberSpace::Native,
      messages: ChatNumberSpace::Native,
      ranges: ChatNumberRanges::default(),
    },
    speakers: true,
  }
}

fn handed() -> HandedMessage {
  HandedMessage {
    origin: MessageOrigin::Live,
    sender_id: None,
    sender_name: "A person".into(),
    sent_at: 1_760_000_000_000,
    text: "Hello".into(),
    reply_to_message_id: None,
    via: "stand-in".into(),
    album: None,
    mode: ChatVoice::BotAccount,
    context: InboundChatContext::default(),
    room: RoomFacts::default(),
    files: vec![],
    gathering: false,
    gather_failure: None,
  }
}

#[test]
fn every_body_the_client_crate_writes_is_valid_against_the_schema_of_its_provider_route() {
  use Part::{Answer, Request};
  let mut bodies = Bodies::new(Side::Provider);

  bodies.valid("get", "/declaration", Answer(200), &declaration());
  bodies.valid("get", "/health", Answer(200), &ChatHealth::ready("1.0.0"));
  bodies.valid("get", "/health", Answer(200), &ChatHealth::not_ready("1.0.0", "Not open yet."));
  let members = ChatMembers {
    members: vec![
      ChatMember {
        person: Some(42),
        name: "A person".into(),
        status_message: Some("Away".into()),
      },
      ChatMember {
        person: None,
        name: "A guest".into(),
        status_message: None,
      },
    ],
  };
  bodies.valid("get", "/rooms/{roomId}/members", Answer(200), &members);
  for send in every_send() {
    bodies.valid("post", "/sends", Request, &send);
  }
  let missed = MissedPicture {
    address: "/nowhere.png".into(),
    reason: MissedPictureReason::Platform {
      detail: "Too large.".into(),
    },
  };
  for missed in [vec![], vec![missed]] {
    bodies.valid(
      "post",
      "/sends",
      Answer(201),
      &ChatSent {
        message_id: Some(9),
        missed,
      },
    );
  }
  for (kind, private_to, title, modes) in [
    (RoomKind::Owner, Some(42), None, None),
    (RoomKind::Group, None, Some("A room"), Some(vec![ChatVoice::HumanReader])),
  ] {
    let room = ProviderRoom {
      id: -100,
      owns: true,
      identity: Some(7),
      kind,
      private_to,
      title: title.map(str::to_owned),
      modes,
    };
    bodies.valid("get", "/rooms/{roomId}", Answer(200), &room);
    let listed = Page {
      items: vec![room],
      more: true,
    };
    bodies.valid("get", "/rooms", Answer(200), &listed);
    bodies.valid("post", "/rooms", Answer(201), &listed.items[0]);
  }
  for start in [
    ChatStartRequest::new(ChatStart::OneToOne { contact_id: 42 }),
    ChatStartRequest {
      start: ChatStart::Group {
        title: "A room".into(),
        contact_ids: vec![42, 77],
      },
      account_id: Some(77),
    },
  ] {
    bodies.valid("post", "/rooms", Request, &start);
  }
  for (kind, room_id) in [(ChatContactKind::Person, Some(7)), (ChatContactKind::Bot, None)] {
    let contacts = Page {
      items: vec![ChatContact {
        id: 42,
        name: "A person".into(),
        kind,
        room_id,
      }],
      more: true,
    };
    bodies.valid("get", "/contacts", Answer(200), &contacts);
  }
  for room_id in [None, Some(7)] {
    bodies.valid("get", "/owner", Answer(200), &ProviderOwner { room_id });
  }
  let files = Page {
    items: vec![message_file()],
    more: false,
  };
  bodies.valid("get", "/rooms/{roomId}/messages/{messageId}/files", Answer(200), &files);
  bodies.valid("post", "/dialect/readings", Request, &DialectRead { text: "*a*".into() });
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
    bodies.valid("post", "/dialect/readings", Answer(200), &reading);
  }
  bodies.valid(
    "post",
    "/dialect/checks",
    Request,
    &DialectCheck {
      markup: "<i>a</i>".into(),
    },
  );
  for fault in [None, Some("An unclosed mark.".to_owned())] {
    bodies.valid(
      "post",
      "/dialect/checks",
      Answer(200),
      &DialectChecked {
        fault,
        visible: "a".into(),
      },
    );
  }
  let avatars = Page {
    items: vec![ChatAvatar {
      kind: ChatAvatarKind::Person,
      id: 42,
      taken_at: 1_760_000_000_000,
    }],
    more: true,
  };
  bodies.valid("get", "/avatars", Answer(200), &avatars);
  bodies.bytes("get", "/rooms/{roomId}/avatar", "image/*");
  bodies.bytes("get", "/people/{personId}/avatar", "image/*");
  bodies.every_route(Side::Provider);
}

#[test]
fn every_body_the_client_crate_writes_is_valid_against_the_schema_of_its_receiving_side_route() {
  use Part::{Answer, Request};
  let mut bodies = Bodies::new(Side::Receiver);

  let room = |id, oldest: Option<i64>| ReceiverRoom {
    id,
    listened: oldest.is_some(),
    read: oldest.is_some(),
    oldest_message_id: oldest,
    oldest_sent_at: oldest.map(|_| 1_760_000_000_000),
  };
  let page = Page {
    items: vec![room(-100, Some(5)), room(7, None)],
    more: false,
  };
  bodies.valid("get", "/rooms", Answer(200), &page);
  bodies.valid("get", "/rooms/{roomId}", Answer(200), &room(-100, None));
  let full = HandedMessage {
    origin: MessageOrigin::Edit,
    sender_id: Some(42),
    reply_to_message_id: Some(4),
    album: Some(3),
    context: InboundChatContext {
      room_title: Some("A room".into()),
      sender_username: Some("a_person".into()),
      reply_to_sender_id: Some(42),
      spoken_text: Some("said aloud".into()),
      metadata: Some(json!({ "anything": [1, "two"] })),
      carries_file: true,
      tag: Some("echo-1".into()),
    },
    room: RoomFacts {
      kind: Some(RoomKind::Group),
      identity: Some(7),
      private_to: Some(42),
    },
    files: vec![message_file()],
    gather_failure: Some("Too large to keep.".into()),
    ..handed()
  };
  for message in [handed(), full] {
    bodies.valid("put", "/rooms/{roomId}/messages/{messageId}", Request, &message);
    bodies.valid("put", "/rooms/{roomId}/messages/{messageId}", Answer(200), &message);
    bodies.valid("put", "/rooms/{roomId}/messages/{messageId}", Answer(201), &message);
    bodies.valid("patch", "/rooms/{roomId}/messages/{messageId}", Answer(200), &message);
  }
  for patch in [GatheredPatch::kept(vec![message_file()]), GatheredPatch::not_kept("Too large to keep.")] {
    bodies.valid("patch", "/rooms/{roomId}/messages/{messageId}", Request, &patch);
  }
  let change = AvatarChange {
    via: "stand-in".into(),
    room_ids: vec![-100],
    person_ids: vec![],
  };
  bodies.valid("post", "/avatar-changes", Request, &change);
  bodies.valid("post", "/avatar-changes", Answer(201), &change);
  let reactions = bmo_chat_client::ReactionChange {
    via: "stand-in".into(),
    room_id: -100,
    message_id: 5,
    stamp: Some(bmo_chat_client::MessageStamp {
      sender_id: 42,
      sent_at: 1_700_000_000_000,
    }),
    reactions: vec![bmo_chat_client::PersonReactions {
      person_id: Some(42),
      emojis: vec!["\u{1F44D}".into()],
    }],
    whole: false,
  };
  bodies.valid("post", "/reaction-changes", Request, &reactions);
  bodies.valid("post", "/reaction-changes", Answer(201), &reactions);
  let deletion = bmo_chat_client::Deletion {
    via: "stand-in".into(),
    room_id: -100,
    message_ids: vec![5],
    stamps: vec![],
  };
  bodies.valid("post", "/deletions", Request, &deletion);
  bodies.valid("post", "/deletions", Answer(201), &deletion);
  for lasts_ms in [Some(5_000), None] {
    let mark = bmo_chat_client::TypingMark {
      via: "stand-in".into(),
      room_id: -100,
      person_id: 42,
      on: true,
      lasts_ms,
    };
    bodies.valid("post", "/typing-marks", Request, &mark);
    bodies.valid("post", "/typing-marks", Answer(201), &mark);
  }
  let presence = bmo_chat_client::PresenceChange {
    via: "stand-in".into(),
    people: vec![
      bmo_chat_client::PersonPresence {
        person_id: 42,
        presence: bmo_chat_client::ChatPresence {
          state: bmo_chat_client::ChatPresenceState::Away,
          last_seen_at: Some(1_700_000_000_000),
        },
      },
      bmo_chat_client::PersonPresence {
        person_id: 43,
        presence: bmo_chat_client::ChatPresence {
          state: bmo_chat_client::ChatPresenceState::Online,
          last_seen_at: None,
        },
      },
    ],
    whole: true,
  };
  bodies.valid("post", "/presence-changes", Request, &presence);
  bodies.valid("post", "/presence-changes", Answer(201), &presence);
  bodies.every_route(Side::Receiver);
}

#[test]
fn a_body_not_of_its_route_shape_is_not_valid() {
  use Part::{Answer, Request};
  let provider = Bodies::new(Side::Provider);
  provider.invalid("post", "/sends", Request, json!({ "kind": "text", "fields": { "text": "Hello." } }));
  provider.invalid("post", "/sends", Request, json!({ "roomId": -100, "kind": "shout", "fields": {} }));
  provider.invalid("post", "/sends", Request, json!({ "roomId": -100, "kind": "text", "text": "Hello." }));
  provider.invalid(
    "post",
    "/sends",
    Request,
    json!({ "roomId": -100, "kind": "document", "fields": { "fileName": "a" } }),
  );
  provider.invalid("post", "/sends", Answer(201), json!({ "missed": [] }));
  provider.invalid(
    "post",
    "/sends",
    Answer(201),
    json!({ "messageId": 9, "missed": [{ "address": "/nowhere.png", "reason": { "code": "lost" } }] }),
  );
  let files = "/rooms/{roomId}/messages/{messageId}/files";
  provider.invalid(
    "get",
    files,
    Answer(200),
    json!({ "items": [{ "name": "a.png", "mime": "image/png", "bytes": "iVA=" }], "more": false }),
  );
  provider.invalid("get", files, Answer(200), json!({ "items": [] }));
  provider.invalid("post", "/rooms", Request, json!({ "contactId": "someone" }));
  provider.invalid("post", "/rooms", Request, json!({ "contactId": 42, "title": "A room" }));
  provider.invalid("post", "/rooms", Request, json!({ "title": "A room" }));
  provider.invalid("post", "/rooms", Request, json!({ "contactId": 42, "accountId": "someone" }));
  provider.invalid("post", "/rooms", Request, json!({ "contactId": 42, "account": 77 }));
  provider.invalid(
    "get",
    "/contacts",
    Answer(200),
    json!({ "items": [{ "id": 42, "name": "A person", "kind": "group", "roomId": null }], "more": false }),
  );
  provider.invalid("get", "/declaration", Answer(200), json!({ "key": "stand-in" }));
  provider.invalid("get", "/health", Answer(200), json!({ "ready": true, "reason": null }));
  provider.invalid(
    "get",
    "/rooms/{roomId}",
    Answer(200),
    json!({ "id": -100, "owns": true, "kind": "channel" }),
  );
  provider.invalid(
    "get",
    "/avatars",
    Answer(200),
    json!({ "items": [{ "kind": "group", "id": 1, "takenAt": 1 }], "more": false }),
  );
  let receiver = Bodies::new(Side::Receiver);
  let message = "/rooms/{roomId}/messages/{messageId}";
  let mut wrong = serde_json::to_value(handed()).unwrap();
  wrong["origin"] = json!("forwarded");
  receiver.invalid("put", message, Request, wrong);
  let mut missing = serde_json::to_value(handed()).unwrap();
  missing.as_object_mut().unwrap().remove("via");
  receiver.invalid("put", message, Request, missing);
  receiver.invalid("patch", message, Request, json!({ "gatherFailure": "Too large." }));
  receiver.invalid("get", "/rooms", Answer(200), json!({ "items": [{ "id": -100 }], "more": false }));
  receiver.invalid("post", "/avatar-changes", Request, json!({ "roomIds": [], "personIds": [] }));
}

#[test]
fn the_queries_and_keys_state_their_parameters() {
  let names = |document: &Value, path: &str| -> Vec<(String, String, bool)> {
    let operation = &document["paths"][path]["get"];
    let mut names: Vec<_> = operation["parameters"]
      .as_array()
      .unwrap()
      .iter()
      .map(|parameter| {
        let name = parameter["name"].as_str().unwrap().to_owned();
        let place = parameter["in"].as_str().unwrap().to_owned();
        (name, place, parameter["required"].as_bool().unwrap())
      })
      .collect();
    names.sort();
    names
  };
  let owned = |name: &str, place: &str, required| (name.to_owned(), place.to_owned(), required);
  let provider = openapi(Side::Provider);
  assert_eq!(
    names(&provider, "/avatars"),
    [owned("after", "query", false), owned("limit", "query", false)]
  );
  assert_eq!(
    names(&provider, "/rooms"),
    [owned("after", "query", false), owned("limit", "query", false)]
  );
  assert_eq!(
    names(&provider, "/contacts"),
    [
      owned("accountId", "query", false),
      owned("after", "query", false),
      owned("limit", "query", false)
    ]
  );
  assert_eq!(
    names(&provider, "/rooms/{roomId}/messages/{messageId}/files"),
    [
      owned("messageId", "path", true),
      owned("roomId", "path", true),
      owned("sentAt", "query", false)
    ]
  );
  assert_eq!(names(&provider, "/people/{personId}/avatar"), [owned("personId", "path", true)]);
  let receiver = openapi(Side::Receiver);
  assert_eq!(
    names(&receiver, "/rooms"),
    [
      owned("after", "query", false),
      owned("limit", "query", false),
      owned("via", "query", true)
    ]
  );
  assert_eq!(
    names(&receiver, "/rooms/{roomId}"),
    [owned("roomId", "path", true), owned("via", "query", true)]
  );
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

/// A route's method and path, with each status it refuses at and the codes it names there.
type Refusing = (&'static str, &'static str, &'static [(&'static str, &'static [&'static str])]);

#[test]
fn every_refusal_and_the_wait_are_answered_at_their_status_in_the_error_shape() {
  let document = openapi(Side::Provider);
  let refusals: &[Refusing] = &[
    (
      "post",
      "/sends",
      &[
        ("403", &["speaker-not-allowed"]),
        ("410", &["gone"]),
        ("422", &["unsupported-capability", "no-picture-sent", "speaker-unnumbered", "platform"]),
        ("503", &["not-ready"]),
      ],
    ),
    (
      "post",
      "/rooms",
      &[
        ("404", &["not-found"]),
        ("422", &["unsupported-capability", "platform"]),
        ("503", &["not-ready"]),
      ],
    ),
    ("get", "/contacts", &[("422", &["unsupported-capability"]), ("503", &["not-ready"])]),
    (
      "get",
      "/rooms/{roomId}/messages/{messageId}/files",
      &[
        ("404", &["unavailable"]),
        ("410", &["gone"]),
        ("422", &["unsupported-capability", "platform"]),
        ("503", &["not-ready"]),
      ],
    ),
  ];
  for (method, path, statuses) in refusals {
    let responses = &document["paths"][path][method]["responses"];
    for (status, codes) in *statuses {
      let named = codes_of(&document, &responses[status]);
      for code in *codes {
        assert!(named.iter().any(|named| named == code), "{path} {status} names `{code}`: {named:?}");
      }
    }
    let codes: Vec<&str> = document["paths"][path][method]["x-codes"]
      .as_array()
      .unwrap()
      .iter()
      .filter_map(Value::as_str)
      .collect();
    for (_, named) in *statuses {
      assert!(
        named.iter().all(|code| codes.contains(code)),
        "{path} lists its codes in x-codes: {codes:?}"
      );
    }
  }
  let waiting = document.pointer("/components/responses/NotReady").unwrap();
  assert!(waiting["headers"]["Retry-After"].is_object(), "not-ready says when to ask again");
  for (method, path) in [
    ("get", "/declaration"),
    ("get", "/health"),
    ("post", "/dialect/readings"),
    ("get", "/avatars"),
  ] {
    let responses = document["paths"][path][method]["responses"].as_object().unwrap();
    assert!(responses.get("503").is_none(), "{path} answers in any state");
    for answer in responses
      .iter()
      .filter(|(status, _)| !status.starts_with('2'))
      .map(|(_, answer)| answer)
    {
      let named = codes_of(&document, answer);
      assert!(
        named
          .iter()
          .all(|code| !["speaker-not-allowed", "gone", "unsupported-capability", "unavailable"]
            .contains(&code.as_str())),
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
fn every_reference_points_at_what_it_holds_and_every_schema_is_a_json_schema() {
  for side in [Side::Provider, Side::Receiver] {
    let document = openapi(side);
    let mut found = BTreeSet::new();
    references(&document, &mut found);
    assert!(!found.is_empty());
    for reference in found {
      let pointer = reference.strip_prefix('#').expect("every reference is within the document");
      assert!(document.pointer(pointer).is_some(), "{side:?}: {reference} points at nothing");
    }
    for (name, schema) in document["components"]["schemas"].as_object().unwrap() {
      let mut schema = schema.clone();
      schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
      jsonschema::meta::validate(&schema).unwrap_or_else(|error| panic!("{side:?} {name}: {error}"));
    }
  }
}

#[test]
fn the_documents_state_the_plugin_and_the_provider_document_its_thirteen_tools_each_on_a_route() {
  for side in [Side::Provider, Side::Receiver] {
    let document = openapi(side);
    assert_eq!(document["openapi"], "3.1.0");
    assert_eq!(document["info"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(document["x-layer"], "upstream");
    assert_eq!(document["x-stage"], 1);
    assert_eq!(document["x-calls"], json!([]));
    assert_eq!(document["x-harness"]["plugin"], "bmo-chat");
    assert_eq!(document["x-harness"]["mcp"], "bmo-chat mcp --provider <url>");
  }
  assert_eq!(openapi(Side::Receiver)["x-tools"], json!([]));
  let document = openapi(Side::Provider);
  let stated: Vec<&str> = document["x-tools"]
    .as_array()
    .unwrap()
    .iter()
    .map(|tool| tool["name"].as_str().unwrap())
    .collect();
  assert_eq!(stated, TOOLS);
  let listed: Vec<&str> = tools().iter().map(|tool| tool.name).collect();
  assert_eq!(listed, TOOLS, "the document states the tools the MCP server lists");
  let documented = routes(Side::Provider);
  for tool in document["x-tools"].as_array().unwrap() {
    let (method, path) = tool["route"].as_str().unwrap().split_once(' ').unwrap();
    assert!(
      documented.contains(&(method.to_owned(), path.to_owned())),
      "{} calls a route of the document",
      tool["name"]
    );
    assert_eq!(tool["input"]["type"], "object", "{}", tool["name"]);
    assert!(tool["output"]["$ref"].is_string(), "{}", tool["name"]);
  }
}

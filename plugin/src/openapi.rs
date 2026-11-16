//! The protocol's OpenAPI documents, one for each side: every route `docs/protocol.md` lists, with its
//! parameters, its request body, its answers and the error codes it can give, for a service written in any
//! language. The body schemas come from the types bmo-chat-client sends and reads, each described as its
//! sender writes it, and the provider document carries the harness tools from the table the MCP server lists.

use std::collections::BTreeMap;

use bmo_chat_client::{
  AccountRoomsQuery, AvatarChange, AvatarsQuery, ChatAvatar, ChatContact, ChatDeclaration, ChatHealth,
  ChatMembers, ChatSendRequest, ChatSent, ChatStartRequest, ContactsQuery, Deletion, DialectCheck,
  DialectChecked, DialectRead, DialectReading, ErrorCode, FilesQuery, GatheredPatch, HandedMessage,
  MessageFile, Page, PresenceChange, ProviderOwner, ProviderRoom, ReactionChange, ReceiverRoom, RoomQuery,
  RoomsQuery, TypingMark, WireError,
};
use schemars::generate::{SchemaGenerator, SchemaSettings};
use schemars::{JsonSchema, Schema};
use serde_json::{Map, Value, json};
use strum::VariantArray;

use crate::tools::tools;

/// Where the schemas the routes refer to stand in a document.
const SCHEMAS: &str = "/components/schemas";

/// The plugin a harness takes the protocol in, and the command that starts its MCP server.
const PLUGIN: &str = "bmo-chat";
const MCP: &str = "bmo-chat mcp --provider <url>";

/// Which side serves a document's routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
  /// The routes a provider serves and the receiving side calls.
  Provider,
  /// The routes the receiving side serves and a provider calls.
  Receiver,
}

impl Side {
  /// The side by the name its document is kept under: `provider` or `receiver`.
  pub fn named(name: &str) -> Option<Self> {
    match name {
      "provider" => Some(Self::Provider),
      "receiver" => Some(Self::Receiver),
      _ => None,
    }
  }
}

/// The OpenAPI document of one side.
pub fn openapi(side: Side) -> Value {
  let mut generator = SchemaSettings::draft2020_12()
    .for_serialize()
    .with(|settings| {
      settings.definitions_path = SCHEMAS.into();
      settings.meta_schema = None;
    })
    .into_generator();
  let error = generator.subschema_for::<WireError>();
  let (routes, title, description, server, entry) = match side {
    Side::Provider => (
      provider_routes(&mut generator),
      "bmo-chat provider routes",
      PROVIDER,
      "a chat provider",
      "A harness building or testing a chat provider reads its declaration with chat_declaration, sends \
       through it with chat_send and checks its dialect with chat_dialect_check, with no receiving side \
       running.",
    ),
    Side::Receiver => (
      receiver_routes(&mut generator),
      "bmo-chat receiving-side routes",
      RECEIVER,
      "the receiving side",
      "The receiving side's routes a provider is tested against; the same plugin as the provider routes.",
    ),
  };
  let mut tags: Vec<&str> = Vec::new();
  let mut paths = Map::new();
  for route in routes {
    if !tags.contains(&route.tag) {
      tags.push(route.tag);
    }
    let (path, method) = (route.path, route.method);
    paths.entry(path).or_insert_with(|| json!({}))[method] = route.operation(&error);
  }
  let x_tools: Vec<Value> = match side {
    Side::Provider => tools()
      .iter()
      .map(|tool| {
        json!({
          "name": tool.name,
          "description": tool.description,
          "input": tool.input(&mut generator),
          "output": tool.output(&mut generator),
          "route": tool.route,
        })
      })
      .collect(),
    Side::Receiver => Vec::new(),
  };
  let responses: Map<String, Value> = ErrorCode::VARIANTS
    .iter()
    .map(|&code| (response_name(code), answered(&error, &[code])))
    .collect();
  json!({
    "openapi": "3.1.0",
    "info": {
      "title": title,
      "version": env!("CARGO_PKG_VERSION"),
      "description": description,
    },
    "servers": [{ "url": "/", "description": server }],
    "security": [{ "bearer": [] }],
    "x-layer": "upstream",
    "x-stage": 1,
    "x-calls": [],
    "x-harness": { "plugin": PLUGIN, "mcp": MCP, "entry": entry },
    "tags": tags.iter().map(|tag| json!({ "name": tag })).collect::<Vec<_>>(),
    "paths": paths,
    "components": {
      "schemas": generator.take_definitions(true),
      "responses": responses,
      "securitySchemes": {
        "bearer": {
          "type": "http",
          "scheme": "bearer",
          "description": "The token of the service called, from the caller's own configuration.",
        },
      },
    },
    "x-tools": x_tools,
  })
}

/// The document as the repository keeps it: two-space indented JSON with a closing newline.
pub fn openapi_json(side: Side) -> String {
  let mut text = serde_json::to_string_pretty(&openapi(side)).expect("a JSON value always renders");
  text.push('\n');
  text
}

const PROVIDER: &str = "The routes a chat provider serves and the receiving side calls, as the chat protocol \
  defines them: the declaration, health, sends, a room, the account owner's room, a message's files fetched \
  again, the dialect's readings and checks, and kept profile pictures. Paths are relative to the base address \
  the caller is configured with, and a service may mount them under a prefix of its own. Bodies are JSON, \
  except a profile picture, which is answered as its own bytes; bytes inside JSON are base64. Times are \
  integers of epoch milliseconds. Every failure, a refusal among them, is answered in the error shape with its \
  status, so a 2xx answer is always the route's success. The sends, a room, the owner and a message's files \
  answer 503 `not-ready` while the provider is not ready. A caller offers a request again only when the \
  connection was refused, so a request that reached a listener is never made twice. The harness plugin's \
  tools, each a thin call of one route, are stated under `x-tools`.";

const RECEIVER: &str = "The routes the receiving side serves and a provider calls, as the chat protocol \
  defines them, so no receiving side grows a variant: a message handed over (live, edited or read back), the \
  files a gathering message promised, the rooms a provider may read back and whether one is listened to, \
  word that kept pictures, a message's reactions or its deletion changed, and who is typing or reachable \
  now. Paths are relative to the base address the provider is configured with, and a service may mount \
  them under a prefix of its own. Bodies are JSON; bytes inside them are base64. Times are integers of \
  epoch milliseconds. Every failure is answered in the error shape with its status.";

/// An answer in the error shape with one of these codes, all answered with one status.
fn answered(error: &Schema, codes: &[ErrorCode]) -> Value {
  let meaning = |code: ErrorCode| format!("`{}`: {}", code_name(code), code.meaning());
  let description = match codes {
    [code] => meaning(*code),
    _ => codes.iter().map(|&code| format!("- {}", meaning(code))).collect::<Vec<_>>().join("\n"),
  };
  let mut answer = json!({
    "description": description,
    "content": { "application/json": { "schema": error } },
  });
  if codes.contains(&ErrorCode::NotReady) {
    answer["headers"] = json!({
      "Retry-After": {
        "description": "When to ask again, in seconds.",
        "schema": { "type": "integer", "minimum": 0 },
      },
    });
  }
  answer
}

/// The code as it crosses the wire.
fn code_name(code: ErrorCode) -> String {
  match serde_json::to_value(code) {
    Ok(Value::String(name)) => name,
    _ => unreachable!("an error code is a unit variant, so it renders as a string"),
  }
}

/// The name the document's answer of one code stands under: the code's own name in Rust.
fn response_name(code: ErrorCode) -> String {
  format!("{code:?}")
}

/// One route as a document states it.
struct Route {
  method: &'static str,
  path: &'static str,
  id: &'static str,
  tag: &'static str,
  summary: &'static str,
  /// The path and query parameters, as OpenAPI parameter objects.
  parameters: Vec<Value>,
  body: Option<Schema>,
  /// Whether the body may also come as multipart, its files carried as parts.
  parts: bool,
  /// The success answers, each by its status: the content by media type.
  answers: Vec<(u16, Value)>,
  /// Every code the route can answer, the request's own faults among them.
  codes: Vec<ErrorCode>,
}

impl Route {
  /// The route's operation. A status that one code answers refers to that code's answer; a status that
  /// several share describes them together.
  fn operation(self, error: &Schema) -> Value {
    let mut statuses: BTreeMap<u16, Vec<ErrorCode>> = BTreeMap::new();
    for &code in &self.codes {
      statuses.entry(code.status()).or_default().push(code);
    }
    let mut responses = Map::new();
    for (status, content) in self.answers {
      let description = if status == 201 { "Created." } else { "Answered." };
      responses.insert(status.to_string(), json!({ "description": description, "content": content }));
    }
    for (status, codes) in statuses {
      let answer = match codes.as_slice() {
        [code] => json!({ "$ref": format!("#/components/responses/{}", response_name(*code)) }),
        _ => answered(error, &codes),
      };
      responses.insert(status.to_string(), answer);
    }
    let mut operation = json!({
      "operationId": self.id,
      "tags": [self.tag],
      "summary": self.summary,
      "responses": responses,
      "x-codes": self.codes.iter().map(|&code| code_name(code)).collect::<Vec<_>>(),
    });
    if !self.parameters.is_empty() {
      operation["parameters"] = self.parameters.into();
    }
    if let Some(body) = self.body {
      let mut content = json!({ "application/json": { "schema": body.clone() } });
      if self.parts {
        content["multipart/form-data"] = parts_body(body);
      }
      operation["requestBody"] = json!({ "required": true, "content": content });
    }
    operation
  }
}

/// A body as a multipart form: the JSON body as the part `body`, and each file it names by `part` as the part
/// of that name, its bytes as they are.
fn parts_body(body: Schema) -> Value {
  json!({
    "schema": {
      "type": "object",
      "properties": { "body": body },
      "required": ["body"],
      "additionalProperties": { "type": "string", "contentMediaType": "application/octet-stream" },
    },
    "encoding": { "body": { "contentType": "application/json" } },
  })
}

/// A JSON answer of this type.
fn json_answer<Answer: JsonSchema>(generator: &mut SchemaGenerator) -> Value {
  json!({ "application/json": { "schema": generator.subschema_for::<Answer>() } })
}

/// An answer of bytes, typed by `Content-Type`.
fn bytes_answer() -> Value {
  json!({ "image/*": { "schema": { "description": "The picture's bytes." } } })
}

/// One path parameter: a member's number.
fn key(generator: &mut SchemaGenerator, name: &str, description: &str) -> Value {
  json!({
    "name": name,
    "in": "path",
    "required": true,
    "description": description,
    "schema": generator.subschema_for::<i64>(),
  })
}

/// The query parameters of a query type, each property one parameter, required as the type requires it.
fn query<Query: JsonSchema>(generator: &mut SchemaGenerator) -> Vec<Value> {
  let schema = Query::json_schema(generator).to_value();
  let required: Vec<&str> = schema["required"]
    .as_array()
    .map(|names| names.iter().filter_map(Value::as_str).collect())
    .unwrap_or_default();
  let Some(properties) = schema["properties"].as_object() else {
    return Vec::new();
  };
  properties
    .iter()
    .map(|(name, property)| {
      let mut parameter = json!({
        "name": name,
        "in": "query",
        "required": required.contains(&name.as_str()),
        "schema": property,
      });
      if let Some(description) = property.get("description") {
        parameter["description"] = description.clone();
      }
      parameter
    })
    .collect()
}

/// The codes a route answers: its request's own faults, the token, and the route's own.
fn codes(asks: bool, own: &[ErrorCode]) -> Vec<ErrorCode> {
  let mut codes = Vec::new();
  if asks {
    codes.push(ErrorCode::BadRequest);
  }
  codes.push(ErrorCode::Unauthorized);
  codes.extend_from_slice(own);
  codes
}

/// The refusals a send can answer.
const SEND_REFUSALS: &[ErrorCode] = &[
  ErrorCode::SpeakerNotAllowed,
  ErrorCode::Gone,
  ErrorCode::UnsupportedCapability,
  ErrorCode::NoPictureSent,
  ErrorCode::SpeakerUnnumbered,
  ErrorCode::Platform,
  ErrorCode::NotReady,
  ErrorCode::Failed,
];

/// The answers short of the files that a fetch can give: the files withheld, and the refusals.
const FILES_REFUSALS: &[ErrorCode] = &[
  ErrorCode::Unavailable,
  ErrorCode::Gone,
  ErrorCode::UnsupportedCapability,
  ErrorCode::Platform,
  ErrorCode::NotReady,
  ErrorCode::Failed,
];

/// Every provider route, in the order `docs/protocol.md` lists them.
fn provider_routes(generator: &mut SchemaGenerator) -> Vec<Route> {
  let room = key(generator, "roomId", "The room's number.");
  vec![
    Route {
      method: "get",
      path: "/declaration",
      id: "getDeclaration",
      tag: "declaration",
      summary: "The provider's self-declaration.",
      parameters: Vec::new(),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<ChatDeclaration>(generator))],
      codes: codes(false, &[]),
    },
    Route {
      method: "get",
      path: "/health",
      id: "getHealth",
      tag: "health",
      summary: "Whether the provider is ready, why not where it is not, and the provider service's version.",
      parameters: Vec::new(),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<ChatHealth>(generator))],
      codes: codes(false, &[]),
    },
    Route {
      method: "post",
      path: "/sends",
      id: "createSends",
      tag: "sends",
      summary: "One send into one room, answered with what it came to; the kind, not the path, carries the \
        difference. A send the provider does not perform is answered with its refusal, and one of a kind the \
        provider does not declare is refused `unsupported-capability`.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<ChatSendRequest>()),
      parts: true,
      answers: vec![(201, json_answer::<ChatSent>(generator))],
      codes: codes(true, SEND_REFUSALS),
    },
    Route {
      method: "get",
      path: "/rooms",
      id: "listAccountRooms",
      tag: "rooms",
      summary: "A page of the rooms the provider's account is in, in ascending order of room number and only \
        those after `after`, at most `limit`, each as `GET /rooms/{roomId}` answers it; refused \
        `unsupported-capability` where the provider does not declare `roomList`.",
      parameters: query::<AccountRoomsQuery>(generator),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<Page<ProviderRoom>>(generator))],
      codes: codes(true, &[ErrorCode::UnsupportedCapability, ErrorCode::NotReady, ErrorCode::Failed]),
    },
    Route {
      method: "post",
      path: "/rooms",
      id: "createRooms",
      tag: "rooms",
      summary: "A conversation started on the platform, a one-to-one with one contact or a group of several \
        under a title, for the account `accountId` names or the provider's own, answered with its room as \
        `GET /rooms/{roomId}` answers it, the one-to-one the account already had where it had one; refused \
        `unsupported-capability` where the provider does not declare the shape's capability, `startChat` or \
        `startGroup`, and `not-found` for a contact it does not know.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<ChatStartRequest>()),
      parts: false,
      answers: vec![(201, json_answer::<ProviderRoom>(generator))],
      codes: codes(
        true,
        &[
          ErrorCode::NotFound,
          ErrorCode::UnsupportedCapability,
          ErrorCode::Platform,
          ErrorCode::NotReady,
          ErrorCode::Failed,
        ],
      ),
    },
    Route {
      method: "get",
      path: "/contacts",
      id: "listContacts",
      tag: "contacts",
      summary: "A page of the contacts of the account `accountId` names, or of the provider's own, in ascending \
        order of contact number and only those after `after`, at most `limit`, each with the one-to-one the \
        account already has with it; refused `unsupported-capability` where the provider does not declare \
        `contactList`.",
      parameters: query::<ContactsQuery>(generator),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<Page<ChatContact>>(generator))],
      codes: codes(true, &[ErrorCode::UnsupportedCapability, ErrorCode::NotReady, ErrorCode::Failed]),
    },
    Route {
      method: "get",
      path: "/rooms/{roomId}",
      id: "getRoom",
      tag: "rooms",
      summary: "What the provider knows of a room: whether it owns it, the account's identity in it, owner or \
        group, the one person of a one-to-one, its title, and the voices it is read by and may be sent in; \
        `not-found` where it knows nothing of it.",
      parameters: vec![room.clone()],
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<ProviderRoom>(generator))],
      codes: codes(false, &[ErrorCode::NotFound, ErrorCode::NotReady, ErrorCode::Failed]),
    },
    Route {
      method: "get",
      path: "/rooms/{roomId}/members",
      id: "getRoomMembers",
      tag: "rooms",
      summary: "The members of a room, whole: each person's number in the `people` space where the provider gave \
        one, the name the room calls them and their status line; `not-found` with the provider's reason where it \
        holds none of them, and refused `unsupported-capability` where the provider does not declare \
        `memberList`.",
      parameters: vec![room.clone()],
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<ChatMembers>(generator))],
      codes: codes(
        true,
        &[
          ErrorCode::NotFound,
          ErrorCode::UnsupportedCapability,
          ErrorCode::Platform,
          ErrorCode::NotReady,
          ErrorCode::Failed,
        ],
      ),
    },
    Route {
      method: "get",
      path: "/owner",
      id: "getOwner",
      tag: "owner",
      summary: "The room to reach the account's owner in unprompted.",
      parameters: Vec::new(),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<ProviderOwner>(generator))],
      codes: codes(false, &[ErrorCode::NotReady, ErrorCode::Failed]),
    },
    Route {
      method: "get",
      path: "/rooms/{roomId}/messages/{messageId}/files",
      id: "listRoomsMessagesFiles",
      tag: "rooms",
      summary: "The files a message carried, fetched again from the platform, in one page; `unavailable` \
        where the platform would not give them, and refused `unsupported-capability` where the provider does \
        not declare `fetchMedia`.",
      parameters: [
        vec![room.clone(), key(generator, "messageId", "The message's number in its room.")],
        query::<FilesQuery>(generator),
      ]
      .concat(),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<Page<MessageFile>>(generator))],
      codes: codes(true, FILES_REFUSALS),
    },
    Route {
      method: "post",
      path: "/dialect/readings",
      id: "makeDialectReadings",
      tag: "dialect",
      summary: "The provider's dialect reading markdown: the markup, its plain words, the parts, the fold.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<DialectRead>()),
      parts: false,
      answers: vec![(200, json_answer::<DialectReading>(generator))],
      codes: codes(true, &[]),
    },
    Route {
      method: "post",
      path: "/dialect/checks",
      id: "makeDialectChecks",
      tag: "dialect",
      summary: "The provider's dialect checking markup it built.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<DialectCheck>()),
      parts: false,
      answers: vec![(200, json_answer::<DialectChecked>(generator))],
      codes: codes(true, &[]),
    },
    Route {
      method: "get",
      path: "/avatars",
      id: "listAvatars",
      tag: "avatars",
      summary: "A page of the profile pictures the provider keeps, oldest taken first and only those taken \
        after `after`, at most `limit` except that a page never ends between two pictures taken at the same \
        moment; empty where the provider does not declare `avatars`.",
      parameters: query::<AvatarsQuery>(generator),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<Page<ChatAvatar>>(generator))],
      codes: codes(true, &[ErrorCode::Failed]),
    },
    Route {
      method: "get",
      path: "/rooms/{roomId}/avatar",
      id: "readRoomsAvatar",
      tag: "rooms",
      summary: "A room's kept picture, as its bytes with its type as `Content-Type`; `not-found` where none is \
        kept or the provider does not declare `avatars`.",
      parameters: vec![room],
      body: None,
      parts: false,
      answers: vec![(200, bytes_answer())],
      codes: codes(false, &[ErrorCode::NotFound, ErrorCode::Failed]),
    },
    Route {
      method: "get",
      path: "/people/{personId}/avatar",
      id: "readPeopleAvatar",
      tag: "people",
      summary: "A person's kept picture, as its bytes with its type as `Content-Type`; `not-found` where none \
        is kept or the provider does not declare `avatars`.",
      parameters: vec![key(generator, "personId", "The person's number.")],
      body: None,
      parts: false,
      answers: vec![(200, bytes_answer())],
      codes: codes(false, &[ErrorCode::NotFound, ErrorCode::Failed]),
    },
  ]
}

/// Every receiving-side route, in the order `docs/protocol.md` lists them.
fn receiver_routes(generator: &mut SchemaGenerator) -> Vec<Route> {
  let room = key(generator, "roomId", "The room's number.");
  let message = key(generator, "messageId", "The message's number in its room.");
  let waits = [ErrorCode::NotReady, ErrorCode::Failed];
  vec![
    Route {
      method: "get",
      path: "/rooms",
      id: "listRooms",
      tag: "rooms",
      summary: "A page of the rooms of one provider whose past it may read back, each with the oldest message \
        held, in ascending order of room number and only those after `after`.",
      parameters: query::<RoomsQuery>(generator),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<Page<ReceiverRoom>>(generator))],
      codes: codes(true, &waits),
    },
    Route {
      method: "get",
      path: "/rooms/{roomId}",
      id: "getRoom",
      tag: "rooms",
      summary: "One room as the receiving side holds it: whether it is listened to, whether the receiving side \
        asked for it to be read, and the oldest message held; `not-found` where nothing is held of it.",
      parameters: [vec![room.clone()], query::<RoomQuery>(generator)].concat(),
      body: None,
      parts: false,
      answers: vec![(200, json_answer::<ReceiverRoom>(generator))],
      codes: codes(true, &[ErrorCode::NotFound, ErrorCode::NotReady, ErrorCode::Failed]),
    },
    Route {
      method: "put",
      path: "/rooms/{roomId}/messages/{messageId}",
      id: "putRoomsMessage",
      tag: "rooms",
      summary: "A message handed over: said live, changed, or read back from the past; answered with the \
        message as held, 201 where the receiving side did not hold it before and 200 where it did.",
      parameters: vec![room.clone(), message.clone()],
      body: Some(generator.subschema_for::<HandedMessage>()),
      parts: true,
      answers: vec![
        (200, json_answer::<HandedMessage>(generator)),
        (201, json_answer::<HandedMessage>(generator)),
      ],
      codes: codes(true, &waits),
    },
    Route {
      method: "patch",
      path: "/rooms/{roomId}/messages/{messageId}",
      id: "changeRoomsMessage",
      tag: "rooms",
      summary: "The files a gathering message promised, or why they could not be kept, as a merge patch; \
        `not-found` where the message is not held, and `conflict` where it promised no files.",
      parameters: vec![room, message],
      body: Some(generator.subschema_for::<GatheredPatch>()),
      parts: true,
      answers: vec![(200, json_answer::<HandedMessage>(generator))],
      codes: codes(
        true,
        &[ErrorCode::NotFound, ErrorCode::Conflict, ErrorCode::NotReady, ErrorCode::Failed],
      ),
    },
    Route {
      method: "post",
      path: "/avatar-changes",
      id: "createAvatarChanges",
      tag: "avatar-changes",
      summary: "Word that the kept profile pictures of these rooms and people changed.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<AvatarChange>()),
      parts: false,
      answers: vec![(201, json_answer::<AvatarChange>(generator))],
      codes: codes(true, &waits),
    },
    Route {
      method: "post",
      path: "/reaction-changes",
      id: "createReactionChanges",
      tag: "reaction-changes",
      summary: "One message's reactions as the platform shows them now.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<ReactionChange>()),
      parts: false,
      answers: vec![(201, json_answer::<ReactionChange>(generator))],
      codes: codes(true, &[ErrorCode::NotFound, ErrorCode::NotReady, ErrorCode::Failed]),
    },
    Route {
      method: "post",
      path: "/deletions",
      id: "createDeletions",
      tag: "deletions",
      summary: "Messages of one room deleted on the platform.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<Deletion>()),
      parts: false,
      answers: vec![(201, json_answer::<Deletion>(generator))],
      codes: codes(true, &waits),
    },
    Route {
      method: "post",
      path: "/typing-marks",
      id: "createTypingMarks",
      tag: "typing-marks",
      summary: "A person writing in a room now, or no longer, standing for `lastsMs` unless told again, \
        six seconds where it is null; a mark about a room nothing is held of is passed over.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<TypingMark>()),
      parts: false,
      answers: vec![(201, json_answer::<TypingMark>(generator))],
      codes: codes(true, &waits),
    },
    Route {
      method: "post",
      path: "/presence-changes",
      id: "createPresenceChanges",
      tag: "presence-changes",
      summary: "Whether people are reachable now; where the change is `whole`, a person not listed is \
        unknown.",
      parameters: Vec::new(),
      body: Some(generator.subschema_for::<PresenceChange>()),
      parts: false,
      answers: vec![(201, json_answer::<PresenceChange>(generator))],
      codes: codes(true, &waits),
    },
  ]
}

//! The OpenAPI document of the protocol: every route `docs/protocol.md` lists, in both directions, with its
//! request body, its answer and the error answers it can give, for a service written in any language. The
//! body schemas come from the types this crate sends and reads, each described as its sender writes it.

use schemars::generate::{SchemaGenerator, SchemaSettings};
use schemars::{JsonSchema, Schema, json_schema};
use serde_json::{Map, Value, json};

use crate::bodies::{
  ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, OwnerRoom, RoomIdentity,
  RoomKindAnswer, RoomOwns, RoomPerson, RoomQuery, RoomTitle,
};
use crate::declaration::ChatDeclaration;
use crate::error::WireError;
use crate::media::{ChatMediaRequest, ChatMediaResult};
use crate::message::{ChatGathered, ChatHandover};
use crate::send::{ChatSendRequest, ChatSendResult};

/// Where the schemas the routes refer to stand in the document.
const SCHEMAS: &str = "/components/schemas";

/// The protocol's OpenAPI document.
pub fn openapi() -> Value {
  let mut generator = SchemaSettings::draft2020_12()
    .for_serialize()
    .with(|settings| {
      settings.definitions_path = SCHEMAS.into();
      settings.meta_schema = None;
    })
    .into_generator();
  let error = generator.subschema_for::<WireError>();
  let paths: Map<String, Value> = routes(&mut generator)
    .into_iter()
    .map(|route| (route.path.to_owned(), json!({ route.method: route.operation() })))
    .collect();
  let answered = |description: &str| {
    json!({
      "description": description,
      "content": { "application/json": { "schema": error } },
    })
  };
  json!({
    "openapi": "3.1.0",
    "info": {
      "title": "The chat wire protocol",
      "version": env!("CARGO_PKG_VERSION"),
      "description": DESCRIPTION,
    },
    "tags": [
      {
        "name": "provider",
        "description": "Served by a provider at the base address the receiver is configured with for it, and \
          called by the receiver.",
      },
      {
        "name": "receiver",
        "description": "Served by the receiver at the base address a provider is configured with, and called \
          by a provider.",
      },
    ],
    "security": [{ "bearer": [] }],
    "paths": paths,
    "components": {
      "schemas": generator.take_definitions(true),
      "responses": {
        "BadRequest": answered(
          "`bad-request`: the body is not JSON, or not the shape the route takes; the sentence names what is \
           missing."
        ),
        "Unauthorized": answered("`unauthorized`: the bearer token is missing or wrong."),
        "NotReady": answered(
          "`not-ready`: the service is up but not ready to answer this route, such as a provider whose \
           platform connection is not open yet."
        ),
        "Failed": answered("`failed`: the service failed while answering; the sentence is the failure."),
      },
      "securitySchemes": {
        "bearer": {
          "type": "http",
          "scheme": "bearer",
          "description": "The token of the service called, from the caller's own configuration.",
        },
      },
    },
  })
}

/// The document as the repository keeps it: two-space indented JSON with a closing newline.
pub fn openapi_json() -> String {
  let mut text = serde_json::to_string_pretty(&openapi()).expect("a JSON value always renders");
  text.push('\n');
  text
}

const DESCRIPTION: &str = "The routes between chat providers and the receiver, with JSON bodies. Paths are \
  relative to the base address the caller is configured with, and a service may mount them under a prefix of \
  its own. Any other method and path is answered 404 `no-route` in the error shape. A route a service serves of \
  its own beside these answers 404 `nothing-held` where it keeps nothing at the address its path names, so a \
  caller tells an empty address from a wrong path by `code`. A refusal is an answer: a \
  send the provider does not perform is answered 200 with a typed refusal. A caller offers a request again \
  only when the connection was refused, so a request that reached a listener is never made twice.";

/// Which side serves a route.
#[derive(Clone, Copy)]
enum Side {
  Provider,
  Receiver,
}

/// One route as the document states it.
struct Route {
  side: Side,
  method: &'static str,
  path: &'static str,
  id: &'static str,
  summary: &'static str,
  body: Option<Schema>,
  answer: Schema,
  /// Whether the route can answer 409 `not-ready` and 500 `failed`: true of the provider routes that wait on
  /// the provider's health, and of every hand-over, which the receiver may not be able to take.
  can_fail: bool,
}

impl Route {
  fn operation(self) -> Value {
    let tag = match self.side {
      Side::Provider => "provider",
      Side::Receiver => "receiver",
    };
    let answered = |name: &str| json!({ "$ref": format!("#/components/responses/{name}") });
    let mut responses = Map::new();
    responses.insert(
      "200".into(),
      json!({
        "description": "Answered.",
        "content": { "application/json": { "schema": self.answer } },
      }),
    );
    if self.body.is_some() {
      responses.insert("400".into(), answered("BadRequest"));
    }
    responses.insert("401".into(), answered("Unauthorized"));
    if self.can_fail {
      responses.insert("409".into(), answered("NotReady"));
      responses.insert("500".into(), answered("Failed"));
    }
    let mut operation = json!({
      "tags": [tag],
      "operationId": self.id,
      "summary": self.summary,
      "responses": responses,
    });
    if let Some(body) = self.body {
      operation["requestBody"] = json!({
        "required": true,
        "content": { "application/json": { "schema": body } },
      });
    }
    operation
  }
}

/// A route a provider serves that takes no body.
fn get<Answer: JsonSchema>(
  generator: &mut SchemaGenerator,
  path: &'static str,
  id: &'static str,
  summary: &'static str,
  can_fail: bool,
) -> Route {
  Route {
    side: Side::Provider,
    method: "get",
    path,
    id,
    summary,
    body: None,
    answer: generator.subschema_for::<Answer>(),
    can_fail,
  }
}

/// A route a provider serves that takes a body.
fn post<Body: JsonSchema, Answer: JsonSchema>(
  generator: &mut SchemaGenerator,
  path: &'static str,
  id: &'static str,
  summary: &'static str,
  can_fail: bool,
) -> Route {
  Route {
    side: Side::Provider,
    method: "post",
    path,
    id,
    summary,
    body: Some(generator.subschema_for::<Body>()),
    answer: generator.subschema_for::<Answer>(),
    can_fail,
  }
}

/// A hand-over the receiver serves, answered with an empty object once it is taken.
fn receiver<Body: JsonSchema>(
  generator: &mut SchemaGenerator,
  path: &'static str,
  id: &'static str,
  summary: &'static str,
) -> Route {
  Route {
    side: Side::Receiver,
    method: "post",
    path,
    id,
    summary,
    body: Some(generator.subschema_for::<Body>()),
    answer: json_schema!({
      "description": "The hand-over was taken.",
      "type": "object",
      "maxProperties": 0,
    }),
    can_fail: true,
  }
}

/// Every route of the protocol. The declaration, health and dialect routes answer whatever state the
/// provider is in; the other provider routes wait on its health.
fn routes(generator: &mut SchemaGenerator) -> Vec<Route> {
  vec![
    get::<ChatDeclaration>(
      generator,
      "/declaration",
      "declaration",
      "The provider's self-declaration.",
      false,
    ),
    get::<ChatHealth>(generator, "/health", "health", "Whether the provider is ready.", false),
    post::<ChatSendRequest, ChatSendResult>(
      generator,
      "/send",
      "send",
      "One send into one room. A send of a verb the provider does not declare is refused \
       `unsupported-capability`.",
      true,
    ),
    post::<ChatMediaRequest, ChatMediaResult>(
      generator,
      "/media",
      "media",
      "A picture or a file a message carried, fetched again from the platform; refused where the provider \
       does not declare `fetchMedia`.",
      true,
    ),
    post::<DialectRead, DialectReading>(
      generator,
      "/dialect/read",
      "dialectRead",
      "The provider's dialect reading markdown.",
      false,
    ),
    post::<DialectCheck, DialectChecked>(
      generator,
      "/dialect/check",
      "dialectCheck",
      "The provider's dialect checking markup it built.",
      false,
    ),
    post::<RoomQuery, RoomOwns>(
      generator,
      "/room/owns",
      "roomOwns",
      "Whether the room is the provider's.",
      true,
    ),
    post::<RoomQuery, RoomIdentity>(
      generator,
      "/room/identity",
      "roomIdentity",
      "The sender number the agent's own words carry in the room.",
      true,
    ),
    post::<RoomQuery, RoomKindAnswer>(
      generator,
      "/room/is-owner",
      "roomIsOwner",
      "Whether the room is the owner's private chat or any other room.",
      true,
    ),
    post::<RoomQuery, RoomPerson>(
      generator,
      "/room/private-of",
      "roomPrivateOf",
      "The one person in a one-to-one with the agent.",
      true,
    ),
    post::<RoomQuery, RoomTitle>(generator, "/room/title", "roomTitle", "The room's title.", true),
    get::<OwnerRoom>(
      generator,
      "/owner-room",
      "ownerRoom",
      "The room to reach the owner in unprompted.",
      true,
    ),
    receiver::<ChatHandover>(generator, "/message", "message", "A message a room said."),
    receiver::<ChatHandover>(
      generator,
      "/attachment",
      "attachment",
      "A message carrying a file, with the file, or with `gathering` in its context where the file follows \
       as `gathered`.",
    ),
    receiver::<ChatHandover>(
      generator,
      "/edited",
      "edited",
      "A message already handed over, said again with different words.",
    ),
    receiver::<ChatGathered>(
      generator,
      "/gathered",
      "gathered",
      "The file a message's `gathering` promised, or why it was not kept.",
    ),
  ]
}

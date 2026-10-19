//! The OpenAPI document of the protocol: every route `docs/protocol.md` lists, in both directions, with its
//! request body, its answer and the error answers it can give, for a service written in any language. The
//! body schemas come from the types this crate sends and reads, each described as its sender writes it.

use schemars::generate::{SchemaGenerator, SchemaSettings};
use schemars::{JsonSchema, Schema, json_schema};
use serde_json::{Map, Value, json};

use crate::avatar::{AvatarList, ChatAvatarKind};
use crate::bodies::{
  ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, OwnerRoom, RoomIdentity,
  RoomKindAnswer, RoomOwns, RoomPerson, RoomQuery, RoomTitle,
};
use crate::declaration::ChatDeclaration;
use crate::error::WireError;
use crate::media::{ChatMediaRequest, ChatMediaResult};
use crate::message::{ChatGathered, ChatHandover};
use crate::receiver_bodies::{AvatarsUpdated, HistoryKept, Listened, ListenedQuery, RoomsOf, RoomsOfQuery};
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
          "`bad-request`: the body is not JSON, or the body or the query is not the shape the route takes; the \
           sentence names what is missing."
        ),
        "Unauthorized": answered("`unauthorized`: the bearer token is missing or wrong."),
        "NothingHeld": answered(
          "`nothing-held`: the route is right, but nothing is kept at the address the path names; a path whose \
           parameters name no address the route can hold is answered `no-route`."
        ),
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
  its own. Bodies are JSON, except a profile picture, which is answered as its own bytes. Times are integers of \
  epoch milliseconds. Any other method and path is answered 404 `no-route` in the error shape. A route that keeps \
  pictures, and any route a service serves of its own beside these, answers 404 `nothing-held` where it keeps \
  nothing at the address its path names, so a caller tells an empty address from a wrong path by `code`. A route \
  answering a list takes one cursor, `after`, and `limit`. A refusal is an answer: a \
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
  /// The path and query parameters, as OpenAPI parameter objects.
  parameters: Vec<Value>,
  body: Option<Schema>,
  /// The content of the 200 answer, by media type.
  answer: Value,
  /// Whether the route can answer 409 `not-ready`: true of the provider routes that wait on the provider's
  /// health, and of every receiver route, which the receiver may not be ready to take or answer.
  can_wait: bool,
  /// Whether the route can answer 500 `failed`: true of every route that asks the provider or the receiver
  /// for its answer.
  can_fail: bool,
  /// Whether the route can answer 404 `nothing-held`.
  holds: bool,
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
        "content": self.answer,
      }),
    );
    if self.body.is_some() || self.parameters.iter().any(|parameter| parameter["in"] == "query") {
      responses.insert("400".into(), answered("BadRequest"));
    }
    responses.insert("401".into(), answered("Unauthorized"));
    if self.holds {
      responses.insert("404".into(), answered("NothingHeld"));
    }
    if self.can_wait {
      responses.insert("409".into(), answered("NotReady"));
    }
    if self.can_fail {
      responses.insert("500".into(), answered("Failed"));
    }
    let mut operation = json!({
      "tags": [tag],
      "operationId": self.id,
      "summary": self.summary,
      "responses": responses,
    });
    if !self.parameters.is_empty() {
      operation["parameters"] = self.parameters.into();
    }
    if let Some(body) = self.body {
      operation["requestBody"] = json!({
        "required": true,
        "content": { "application/json": { "schema": body } },
      });
    }
    operation
  }
}

/// A JSON answer of this type.
fn json_answer<Answer: JsonSchema>(generator: &mut SchemaGenerator) -> Value {
  json!({ "application/json": { "schema": generator.subschema_for::<Answer>() } })
}

/// One path or query parameter.
fn parameter(name: &str, place: &str, schema: Schema, description: &str) -> Value {
  json!({
    "name": name,
    "in": place,
    "required": place == "path",
    "description": description,
    "schema": schema,
  })
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
    parameters: Vec::new(),
    body: None,
    answer: json_answer::<Answer>(generator),
    can_wait: can_fail,
    can_fail,
    holds: false,
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
    parameters: Vec::new(),
    body: Some(generator.subschema_for::<Body>()),
    answer: json_answer::<Answer>(generator),
    can_wait: can_fail,
    can_fail,
    holds: false,
  }
}

/// The two avatar routes a provider serves: the list of the pictures it keeps, and one picture.
fn avatar_routes(generator: &mut SchemaGenerator) -> [Route; 2] {
  let list = Route {
    side: Side::Provider,
    method: "get",
    path: "/avatars",
    id: "avatars",
    summary: "The profile pictures the provider keeps, oldest taken first and only those taken after `after`, \
      at most `limit` except that a list never ends between two pictures taken at the same moment; empty \
      where the provider does not declare `avatars`.",
    parameters: vec![
      parameter(
        "after",
        "query",
        generator.subschema_for::<i64>(),
        "A time: only the pictures taken after it.",
      ),
      parameter("limit", "query", generator.subschema_for::<u32>(), "The most pictures to list."),
    ],
    body: None,
    answer: json_answer::<AvatarList>(generator),
    can_wait: false,
    can_fail: true,
    holds: false,
  };
  let picture = Route {
    side: Side::Provider,
    method: "get",
    path: "/avatars/{kind}/{id}",
    id: "avatar",
    summary: "The picture the provider keeps for a room or a person, as its bytes with its type as \
      `Content-Type`; `nothing-held` where none is kept or the provider does not declare `avatars`.",
    parameters: vec![
      parameter(
        "kind",
        "path",
        generator.subschema_for::<ChatAvatarKind>(),
        "Whose picture: a room's or a person's.",
      ),
      parameter("id", "path", generator.subschema_for::<i64>(), "The room's or the person's number."),
    ],
    body: None,
    answer: json!({ "image/*": { "schema": { "description": "The picture's bytes." } } }),
    can_wait: false,
    can_fail: true,
    holds: true,
  };
  [list, picture]
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
    parameters: Vec::new(),
    body: Some(generator.subschema_for::<Body>()),
    answer: json!({
      "application/json": {
        "schema": json_schema!({
          "description": "The hand-over was taken.",
          "type": "object",
          "maxProperties": 0,
        }),
      },
    }),
    can_wait: true,
    can_fail: true,
    holds: false,
  }
}

/// A route the receiver serves that answers with a body of its own.
fn receiver_answering<Body: JsonSchema, Answer: JsonSchema>(
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
    parameters: Vec::new(),
    body: Some(generator.subschema_for::<Body>()),
    answer: json_answer::<Answer>(generator),
    can_wait: true,
    can_fail: true,
    holds: false,
  }
}

/// Every route of the protocol. The declaration, health, dialect and avatar routes answer whatever state the
/// provider is in; the other provider routes wait on its health.
fn routes(generator: &mut SchemaGenerator) -> Vec<Route> {
  let [avatars, avatar] = avatar_routes(generator);
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
    avatars,
    avatar,
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
      "A message already handed over, changed: the receiver keeps its words and its sender as handed over.",
    ),
    receiver::<ChatGathered>(
      generator,
      "/gathered",
      "gathered",
      "The file a message's `gathering` promised, or why it was not kept.",
    ),
    receiver_answering::<ChatHandover, HistoryKept>(
      generator,
      "/history",
      "history",
      "A past message read back from a room, archived where the receiver lacks it and waking nobody; `kept` \
       says whether it was archived.",
    ),
    receiver::<AvatarsUpdated>(
      generator,
      "/avatars/updated",
      "avatarsUpdated",
      "Word that the kept profile pictures of these rooms and people changed.",
    ),
    receiver_answering::<ListenedQuery, Listened>(
      generator,
      "/rooms/listened",
      "roomsListened",
      "Whether the receiver listens to the room.",
    ),
    receiver_answering::<RoomsOfQuery, RoomsOf>(
      generator,
      "/rooms/of",
      "roomsOf",
      "The provider's rooms whose past it may read back, each with the oldest message the receiver holds of \
       it, in ascending order of room number and only those after `after`, at most `limit`.",
    ),
  ]
}

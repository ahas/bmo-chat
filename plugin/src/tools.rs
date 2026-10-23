//! The harness tools: one for each provider route a harness drives a provider through, each a thin call of
//! one method of the provider client. The MCP server lists them and the provider document states them under
//! `x-tools`, both from this one table.

use std::future::Future;
use std::pin::Pin;

use bmo_chat_client::{
  AccountRoomsQuery, AvatarsQuery, ChatMediaRequest, ChatProviderClient, ChatSendRequest, ClientFailure,
  DialectCheck, DialectRead,
};
use schemars::generate::SchemaGenerator;
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What a tool's call came to short of the route's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolFailure {
  /// The arguments are not the shape the tool takes.
  Arguments(String),
  /// The call reached no answer of the route's shape.
  Client(ClientFailure),
}

impl ToolFailure {
  /// The failure as the tool error's content: the error shape the provider answered with and its status, or
  /// the sentence saying why nothing answered.
  pub fn content(&self) -> Value {
    match self {
      Self::Arguments(why) => json!({ "code": "bad-request", "error": why }),
      Self::Client(ClientFailure::Answered {
        status,
        answer: Some(answer),
        ..
      }) => json!({ "status": status, "code": answer.code, "error": answer.error }),
      Self::Client(failure @ ClientFailure::Answered { status, .. }) => {
        json!({ "status": status, "error": failure.to_string() })
      }
      Self::Client(failure) => json!({ "error": failure.to_string() }),
    }
  }
}

/// A tool's answer: the route's answer as JSON, or why there is none.
pub type Answered = Pin<Box<dyn Future<Output = Result<Value, ToolFailure>> + Send>>;

type Call = Box<dyn Fn(ChatProviderClient, Value) -> Answered + Send + Sync>;

/// One tool: its name, what it does, the route it calls, its input and output, and the call itself.
pub struct ChatTool {
  pub name: &'static str,
  pub description: &'static str,
  /// The route the tool calls, as its method and path.
  pub route: &'static str,
  input: fn(&mut SchemaGenerator) -> Schema,
  output: fn(&mut SchemaGenerator) -> Schema,
  call: Call,
}

impl ChatTool {
  /// The tool's input, its nested types referred to where `generator` keeps its definitions.
  pub fn input(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.input)(generator)
  }

  /// The tool's output, referred to where `generator` keeps its definitions.
  pub fn output(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.output)(generator)
  }

  /// Calls the tool's route on the provider `client` reaches, with the tool's arguments.
  pub fn call(&self, client: ChatProviderClient, arguments: Value) -> Answered {
    (self.call)(client, arguments)
  }
}

/// A tool whose input is `I` and whose answer is `O`, calling `call` with the arguments read as `I`.
fn tool<I, O, F, Fut>(
  name: &'static str,
  description: &'static str,
  route: &'static str,
  call: F,
) -> ChatTool
where
  I: DeserializeOwned + JsonSchema,
  O: Serialize + JsonSchema,
  F: Fn(ChatProviderClient, I) -> Fut + Send + Sync + 'static,
  Fut: Future<Output = Result<O, ClientFailure>> + Send + 'static,
{
  ChatTool {
    name,
    description,
    route,
    input: I::json_schema,
    output: SchemaGenerator::subschema_for::<O>,
    call: Box::new(move |client, arguments| {
      let input = match serde_json::from_value::<I>(arguments) {
        Ok(input) => input,
        Err(error) => {
          let why = format!("The arguments are not the shape this tool takes: {error}.");
          return Box::pin(std::future::ready(Err(ToolFailure::Arguments(why))));
        }
      };
      let answer = call(client, input);
      Box::pin(async move {
        let answer = answer.await.map_err(ToolFailure::Client)?;
        Ok(serde_json::to_value(answer).expect("a route's answer always renders as JSON"))
      })
    }),
  }
}

/// No input.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Nothing {}

/// A body sent as it is.
#[derive(Deserialize, JsonSchema)]
#[schemars(inline)]
struct Body<T> {
  body: T,
}

/// One room, by its number.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Room {
  room_id: i64,
}

/// One message's files: its room, its number and when it was sent.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Files {
  room_id: i64,
  message_id: i64,
  /// When the message was sent, in epoch milliseconds.
  #[serde(default)]
  sent_at: Option<i64>,
}

/// The ten tools, in the order the protocol lists their routes.
pub fn tools() -> Vec<ChatTool> {
  vec![
    tool(
      "chat_declaration",
      "The provider's self-declaration: its key and label, its modes, capabilities, dialect and numbering.",
      "GET /declaration",
      |client, Nothing {}| async move { client.declaration().await },
    ),
    tool(
      "chat_health",
      "Whether the provider is ready, why not where it is not, and its version.",
      "GET /health",
      |client, Nothing {}| async move { client.health().await },
    ),
    tool(
      "chat_send",
      "Performs one send on the platform; the kind, not the route, carries the difference. A send the \
       provider does not perform answers with its refusal.",
      "POST /sends",
      |client, Body { body }: Body<ChatSendRequest>| async move { client.send(&body).await },
    ),
    tool(
      "chat_rooms",
      "A page of the rooms the provider's account is in, in ascending order of room number, only those after \
       `after`; refused where the provider does not declare roomList.",
      "GET /rooms",
      |client, query: AccountRoomsQuery| async move { client.account_rooms(&query).await },
    ),
    tool(
      "chat_room",
      "What the provider knows of a room: whether it owns it, the account's identity in it, whether it is the \
       owner's private chat, the one person of a one-to-one, its title, and the voices it is read by and may \
       be sent in.",
      "GET /rooms/{roomId}",
      |client, Room { room_id }| async move { client.room(room_id).await },
    ),
    tool(
      "chat_owner",
      "The room to reach the account's owner in unprompted.",
      "GET /owner",
      |client, Nothing {}| async move { client.owner().await },
    ),
    tool(
      "chat_message_files",
      "The files a message carried, fetched again from the platform; refused where the provider does not \
       declare fetchMedia.",
      "GET /rooms/{roomId}/messages/{messageId}/files",
      |client,
       Files {
         room_id,
         message_id,
         sent_at,
       }| async move {
        let request = ChatMediaRequest {
          room_id,
          message_id,
          sent_at,
        };
        client.message_files(&request).await
      },
    ),
    tool(
      "chat_dialect_reading",
      "The provider's dialect reading markdown: the markup that would go out, its plain words, the parts each \
       fitting one message, and the fold.",
      "POST /dialect/readings",
      |client, Body { body }: Body<DialectRead>| async move { client.dialect_reading(body.text).await },
    ),
    tool(
      "chat_dialect_check",
      "The provider's dialect checking markup it built: the fault, or none, and what a reader sees of it.",
      "POST /dialect/checks",
      |client, Body { body }: Body<DialectCheck>| async move { client.dialect_check(body.markup).await },
    ),
    tool(
      "chat_avatars",
      "A page of the profile pictures the provider keeps, oldest taken first, only those taken after `after`.",
      "GET /avatars",
      |client, query: AvatarsQuery| async move { client.avatars(&query).await },
    ),
  ]
}

//! The `bmo-chat` command: `bmo-chat mcp --provider <url>` starts an MCP server on standard input and output
//! that lists exactly the protocol's thirteen tools, and each call reaches the provider at the address through
//! the client, behind the token. The command runs as its own process, spoken to over its pipes, and is taken
//! down before each test ends.

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const TOKEN: &str = "the-provider-token";

/// How long a test lets the server take to answer one request before it fails; a guard, never a wait.
const PATIENCE: Duration = Duration::from_secs(30);

/// The command, started with these arguments and environment, and spoken to line by line.
struct Server {
  child: Child,
  input: ChildStdin,
  output: Lines<BufReader<ChildStdout>>,
  next: u64,
}

impl Server {
  fn start(arguments: &[&str], token: Option<&str>) -> Self {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bmo-chat"));
    command
      .args(arguments)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::null());
    command.env_remove("BMO_CHAT_TOKEN").kill_on_drop(true);
    if let Some(token) = token {
      command.env("BMO_CHAT_TOKEN", token);
    }
    let mut child = command.spawn().expect("the command starts");
    let input = child.stdin.take().expect("the command's input is piped");
    let output = BufReader::new(child.stdout.take().expect("the command's output is piped")).lines();
    Self {
      child,
      input,
      output,
      next: 1,
    }
  }

  async fn send(&mut self, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    self.input.write_all(line.as_bytes()).await.expect("the server reads its input");
    self.input.flush().await.expect("the server reads its input");
  }

  /// One request, answered with the result of the answer bearing its id.
  async fn ask(&mut self, method: &str, params: Value) -> Value {
    let id = self.next;
    self.next += 1;
    self
      .send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
      .await;
    loop {
      let line = tokio::time::timeout(PATIENCE, self.output.next_line())
        .await
        .expect("the server answers in time")
        .expect("the server's output reads")
        .expect("the server answers before it closes its output");
      let answer: Value = serde_json::from_str(&line).expect("the server writes JSON lines");
      if answer["id"] == id {
        assert!(answer["error"].is_null(), "{method} is answered without a protocol error: {answer}");
        return answer["result"].clone();
      }
    }
  }

  async fn initialized(arguments: &[&str], token: Option<&str>) -> Self {
    let mut server = Self::start(arguments, token);
    let info = server
      .ask(
        "initialize",
        json!({
          "protocolVersion": "2025-11-25",
          "capabilities": {},
          "clientInfo": { "name": "tester", "version": "1.0.0" },
        }),
      )
      .await;
    assert_eq!(info["serverInfo"]["name"], "bmo-chat");
    assert_eq!(info["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(info["capabilities"]["tools"].is_object(), "the server serves tools: {info}");
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    server
  }

  async fn call(&mut self, tool: &str, arguments: Value) -> Value {
    self.ask("tools/call", json!({ "name": tool, "arguments": arguments })).await
  }

  async fn stop(mut self) {
    drop(self.input);
    self.child.kill().await.ok();
    self.child.wait().await.expect("the command is taken down");
  }
}

/// What the stand-in provider was asked: the method, the path with its query, the authorization header and
/// the body.
type Asked = Arc<Mutex<Vec<(String, String, Option<String>, Value)>>>;

/// A stand-in provider on the loopback: its declaration, a send it refuses, and a picture page; anything else
/// is no route.
struct Stand {
  address: std::net::SocketAddr,
  asked: Asked,
  stop: oneshot::Sender<()>,
  served: JoinHandle<()>,
}

impl Stand {
  async fn start() -> Self {
    async fn answer(
      State(asked): State<Asked>,
      method: axum::http::Method,
      uri: Uri,
      headers: HeaderMap,
      body: axum::body::Bytes,
    ) -> Response {
      let authorization = headers.get("authorization").map(|value| value.to_str().unwrap().to_owned());
      let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
      let at = uri.path_and_query().map(|at| at.as_str().to_owned()).unwrap_or_default();
      asked.lock().unwrap().push((method.to_string(), at, authorization.clone(), body));
      if authorization != Some(format!("Bearer {TOKEN}")) {
        let wrong = json!({ "code": "unauthorized", "error": "The bearer token is missing or wrong." });
        return (StatusCode::UNAUTHORIZED, Json(wrong)).into_response();
      }
      match (method.as_str(), uri.path()) {
        ("GET", "/declaration") => Json(declaration()).into_response(),
        ("POST", "/sends") => {
          let refused =
            json!({ "code": "unsupported-capability", "error": "Stand-in cannot send a sticker." });
          (StatusCode::UNPROCESSABLE_ENTITY, Json(refused)).into_response()
        }
        ("GET", "/avatars") => Json(json!({ "items": [], "more": false })).into_response(),
        _ => {
          let none = json!({ "code": "no-route", "error": "There is no route here." });
          (StatusCode::NOT_FOUND, Json(none)).into_response()
        }
      }
    }
    let asked = Asked::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new().fallback(answer).with_state(asked.clone());
    let (stop, stopped) = oneshot::channel::<()>();
    let served = tokio::spawn(async move {
      axum::serve(listener, router)
        .with_graceful_shutdown(async {
          stopped.await.ok();
        })
        .await
        .expect("the stand-in runs");
    });
    Self {
      address,
      asked,
      stop,
      served,
    }
  }

  fn url(&self) -> String {
    format!("http://{}", self.address)
  }

  async fn close(self) {
    self.stop.send(()).ok();
    self.served.await.expect("the stand-in stops cleanly");
  }
}

fn declaration() -> Value {
  json!({
    "key": "stand-in", "label": "Stand-in", "configured": null,
    "modes": [{
      "voice": "bot-account", "reach": "all-messages", "standing": "active", "reason": null, "canReplyTo": true,
    }],
    "can": ["text"],
    "dialect": {
      "name": "plain",
      "limits": { "text": null, "caption": null, "albumPhotos": null, "fileBytes": null },
      "writing": "This room shows no formatting.",
      "foldable": false,
    },
    "shows": null,
    "numbering": {
      "rooms": "native", "people": "native", "messages": "native", "ranges": { "rooms": null, "people": null },
    },
    "speakers": false,
  })
}

/// The text of a tool's answer, read as JSON.
fn text_of(result: &Value) -> Value {
  let text = result["content"][0]["text"].as_str().expect("a tool answers with text");
  serde_json::from_str(text).expect("the text is JSON")
}

#[tokio::test]
async fn the_server_starts_and_lists_exactly_the_thirteen_tools_each_standing_alone() {
  let mut server = Server::initialized(&["mcp", "--provider", "http://[::1]:9"], None).await;
  let listed = server.ask("tools/list", json!({})).await;
  server.stop().await;

  let tools = listed["tools"].as_array().unwrap();
  let names: Vec<&str> = tools.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
  assert_eq!(
    names,
    [
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
    ]
  );
  for tool in tools {
    let name = &tool["name"];
    assert!(tool["description"].as_str().is_some_and(|words| !words.is_empty()), "{name}");
    for schema in [&tool["inputSchema"], &tool["outputSchema"]] {
      assert_eq!(schema["type"], "object", "{name}");
      let mut references = Vec::new();
      collect_references(schema, &mut references);
      for reference in references {
        let pointer = reference.strip_prefix('#').expect("a reference stays within the schema");
        assert!(schema.pointer(pointer).is_some(), "{name}: {reference} stands inside the schema");
      }
    }
  }
}

fn collect_references(value: &Value, found: &mut Vec<String>) {
  match value {
    Value::Object(object) => {
      if let Some(Value::String(reference)) = object.get("$ref") {
        found.push(reference.clone());
      }
      object.values().for_each(|value| collect_references(value, found));
    }
    Value::Array(values) => values.iter().for_each(|value| collect_references(value, found)),
    _ => {}
  }
}

#[tokio::test]
async fn a_tool_answers_with_its_route_and_a_refusal_comes_back_as_a_tool_error_with_its_code() {
  let stand = Stand::start().await;
  let mut server = Server::initialized(&["mcp", "--provider", &stand.url()], Some(TOKEN)).await;
  let declared = server.call("chat_declaration", json!({})).await;
  let send = json!({ "body": { "kind": "sticker", "roomId": -100, "fields": { "emoji": "\u{1f44d}" } } });
  let refused = server.call("chat_send", send).await;
  let page = server.call("chat_avatars", json!({ "after": 1000, "limit": 5 })).await;
  let wrong = server.call("chat_room", json!({ "room": "seven" })).await;
  server.stop().await;
  let asked = stand.asked.lock().unwrap().clone();
  stand.close().await;

  assert_eq!(declared["isError"], false);
  assert_eq!(declared["structuredContent"], declaration());
  assert_eq!(text_of(&declared), declaration());
  assert_eq!(refused["isError"], true);
  assert!(refused["structuredContent"].is_null(), "a tool error carries no structured content");
  assert_eq!(
    text_of(&refused),
    json!({ "status": 422, "code": "unsupported-capability", "error": "Stand-in cannot send a sticker." })
  );
  assert_eq!(page["structuredContent"], json!({ "items": [], "more": false }));
  assert_eq!(wrong["isError"], true);
  assert_eq!(text_of(&wrong)["code"], "bad-request", "arguments not of the tool's shape are refused");
  let bearer = Some(format!("Bearer {TOKEN}"));
  assert_eq!(
    asked,
    [
      ("GET".to_owned(), "/declaration".to_owned(), bearer.clone(), Value::Null),
      (
        "POST".to_owned(),
        "/sends".to_owned(),
        bearer.clone(),
        json!({ "kind": "sticker", "roomId": -100, "fields": { "emoji": "\u{1f44d}" } }),
      ),
      ("GET".to_owned(), "/avatars?after=1000&limit=5".to_owned(), bearer, Value::Null),
    ],
    "arguments not of the tool's shape reach no route"
  );
}

#[tokio::test]
async fn the_token_is_taken_from_the_flag_before_the_environment() {
  let stand = Stand::start().await;
  let mut flagged =
    Server::initialized(&["mcp", "--provider", &stand.url(), "--token", TOKEN], Some("another-token"))
      .await;
  let declared = flagged.call("chat_declaration", json!({})).await;
  flagged.stop().await;
  let mut unflagged =
    Server::initialized(&["mcp", "--provider", &stand.url()], Some("another-token")).await;
  let refused = unflagged.call("chat_declaration", json!({})).await;
  unflagged.stop().await;
  stand.close().await;

  assert_eq!(declared["isError"], false);
  assert_eq!(text_of(&refused)["status"], 401);
  assert_eq!(text_of(&refused)["code"], "unauthorized");
}

#[tokio::test]
async fn a_provider_nothing_listens_at_is_a_tool_error_saying_so() {
  let address = {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap()
  };
  let mut server =
    Server::initialized(&["mcp", "--provider", &format!("http://{address}")], Some(TOKEN)).await;
  let failed = server.call("chat_health", json!({})).await;
  server.stop().await;

  assert_eq!(failed["isError"], true);
  let content = text_of(&failed);
  assert!(content.get("code").is_none(), "nothing answered, so no code: {content}");
  assert!(content["error"].as_str().unwrap().contains(&address.to_string()), "{content}");
}

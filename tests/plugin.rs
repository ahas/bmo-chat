//! The harness plugin's MCP server over the real provider router: each of the twelve tools, called as a harness
//! calls it, answers exactly what the client answers for its route, from a stand-in provider served by
//! bmo-chat. The server is spoken to over an in-process pipe, and every listener is taken down before the
//! test ends.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use bmo_chat_plugin::ChatServer;
use rmcp::ServiceExt;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

const TOKEN: &str = "the-provider-token";

/// A stand-in provider that can do everything, keeps what it was asked, and answers the same each time.
#[derive(Default)]
struct Stand {
  asked: Mutex<Vec<String>>,
}

impl Stand {
  fn asked(&self, what: impl Into<String>) {
    self.asked.lock().unwrap().push(what.into());
  }
}

#[async_trait]
impl bmo_chat::ChatProvider for Stand {
  fn key(&self) -> &str {
    "stand-in"
  }
  fn label(&self) -> &str {
    "Stand-in"
  }
  fn version(&self) -> &str {
    "3.1.0"
  }
  fn configured(&self) -> Option<String> {
    None
  }
  async fn start(&self, _: Arc<dyn bmo_chat::ChatReceiver>) -> Result<(), bmo_chat::ChatFailure> {
    Ok(())
  }
  async fn stop(&self) {}
  fn modes(&self) -> Vec<bmo_chat::ChatReadMode> {
    vec![bmo_chat::ChatReadMode {
      voice: bmo_chat::ChatVoice::BotAccount,
      reach: bmo_chat::ChatReach::AllMessages,
      standing: bmo_chat::ChatModeStanding::Active,
      reason: None,
      can_reply_to: true,
    }]
  }
  fn can(&self) -> BTreeSet<bmo_chat::ChatCapability> {
    bmo_chat::ChatCapability::ALL.iter().copied().collect()
  }
  fn dialect(&self) -> &dyn bmo_chat::ChatDialect {
    &bmo_chat::PlainDialect
  }
  fn numbering(&self) -> bmo_chat::ChatNumbering {
    bmo_chat::ChatNumbering {
      rooms: bmo_chat::ChatNumberSpace::Native,
      people: bmo_chat::ChatNumberSpace::Native,
      messages: bmo_chat::ChatNumberSpace::Native,
      ranges: bmo_chat::ChatNumberRanges::default(),
    }
  }
  async fn room(&self, room_id: i64) -> Result<Option<bmo_chat::ProviderRoom>, bmo_chat::ChatFailure> {
    self.asked(format!("room {room_id}"));
    Ok(Some(bmo_chat::ProviderRoom {
      id: room_id,
      owns: true,
      identity: Some(7),
      kind: bmo_chat::RoomKind::Group,
      private_to: None,
      title: Some("The garden".into()),
      modes: None,
    }))
  }
  async fn owner_room(&self) -> Result<Option<i64>, bmo_chat::ChatFailure> {
    self.asked("owner");
    Ok(Some(42))
  }
  async fn account_rooms(&self) -> Result<Vec<bmo_chat::ProviderRoom>, bmo_chat::ChatFailure> {
    self.asked("rooms");
    Ok(vec![bmo_chat::ProviderRoom {
      id: -100,
      owns: true,
      identity: Some(7),
      kind: bmo_chat::RoomKind::Group,
      private_to: None,
      title: Some("The garden".into()),
      modes: None,
    }])
  }
  async fn contacts(&self) -> Result<Vec<bmo_chat::ChatContact>, bmo_chat::ChatFailure> {
    self.asked("contacts");
    Ok(vec![bmo_chat::ChatContact {
      id: 42,
      name: "A person".into(),
      kind: bmo_chat::ChatContactKind::Person,
      room_id: None,
    }])
  }
  async fn start_chat(
    &self,
    start: bmo_chat::ChatStart,
  ) -> Result<bmo_chat::ChatStartResult, bmo_chat::ChatFailure> {
    self.asked(format!("start {start:?}"));
    Ok(bmo_chat::ChatStartResult::Started(bmo_chat::ProviderRoom {
      id: 42,
      owns: true,
      identity: None,
      kind: bmo_chat::RoomKind::Group,
      private_to: Some(42),
      title: Some("A person".into()),
      modes: Some(vec![bmo_chat::ChatVoice::HumanReader]),
    }))
  }
  async fn send(
    &self,
    request: bmo_chat::ChatSendRequest,
  ) -> Result<bmo_chat::ChatSendResult, bmo_chat::ChatFailure> {
    self.asked(format!("send {:?} to {}", request.send.kind(), request.room_id));
    Ok(bmo_chat::ChatSendResult::Sent(bmo_chat::ChatSent {
      message_id: Some(77),
      missed: vec![],
    }))
  }
  async fn fetch_media(
    &self,
    request: bmo_chat::ChatMediaRequest,
  ) -> Result<bmo_chat::ChatMediaResult, bmo_chat::ChatFailure> {
    self.asked(format!("files {} {} {:?}", request.room_id, request.message_id, request.sent_at));
    Ok(bmo_chat::ChatMediaResult::Fetched(vec![bmo_chat::MessageFile {
      id: "file-1".into(),
      name: "a.png".into(),
      mime: "image/png".into(),
      bytes: vec![137, 80],
      kind: Some(bmo_chat::ChatFileKind::Photo),
    }]))
  }
  async fn avatars(&self) -> Result<Vec<bmo_chat::ChatAvatar>, bmo_chat::ChatFailure> {
    self.asked("avatars");
    Ok(vec![bmo_chat::ChatAvatar {
      kind: bmo_chat::ChatAvatarKind::Person,
      id: 42,
      taken_at: 2_000,
    }])
  }
}

/// The MCP server's other end: what a harness writes and reads.
struct Harness {
  input: WriteHalf<DuplexStream>,
  output: Lines<BufReader<ReadHalf<DuplexStream>>>,
  next: u64,
}

impl Harness {
  async fn send(&mut self, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    self.input.write_all(line.as_bytes()).await.expect("the server reads");
  }

  async fn ask(&mut self, method: &str, params: Value) -> Value {
    let id = self.next;
    self.next += 1;
    self
      .send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
      .await;
    loop {
      let line = tokio::time::timeout(Duration::from_secs(30), self.output.next_line())
        .await
        .expect("the server answers in time")
        .expect("the pipe reads")
        .expect("the server answers before it closes the pipe");
      let answer: Value = serde_json::from_str(&line).expect("the server writes JSON lines");
      if answer["id"] == id {
        assert!(answer["error"].is_null(), "{method}: {answer}");
        return answer["result"].clone();
      }
    }
  }

  /// A tool's structured answer, which must not be an error.
  async fn tool(&mut self, name: &str, arguments: Value) -> Value {
    let result = self.ask("tools/call", json!({ "name": name, "arguments": arguments })).await;
    assert_eq!(result["isError"], false, "{name}: {result}");
    result["structuredContent"].clone()
  }
}

#[tokio::test]
async fn each_tool_answers_what_the_client_answers_for_its_route() {
  let stand = Arc::new(Stand::default());
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let router = Router::new().nest("/provider", bmo_chat::provider_router(stand.clone(), TOKEN));
  let (stop, stopped) = oneshot::channel::<()>();
  let listening = tokio::spawn(async move {
    axum::serve(listener, router)
      .with_graceful_shutdown(async {
        stopped.await.ok();
      })
      .await
      .expect("the provider runs");
  });
  let base = format!("http://{address}/provider/");
  let client = bmo_chat_client::ChatProviderClient::new(base.clone(), TOKEN);

  let (ours, theirs) = tokio::io::duplex(1 << 20);
  let (read, write) = tokio::io::split(theirs);
  // The session starts once the harness has said `initialize`, so it is served on a task of its own.
  let serving = tokio::spawn(
    ChatServer::new(bmo_chat_client::ChatProviderClient::new(base, TOKEN)).serve((read, write)),
  );
  let (output, input) = tokio::io::split(ours);
  let mut harness = Harness {
    input,
    output: BufReader::new(output).lines(),
    next: 1,
  };
  let started = harness
    .ask(
      "initialize",
      json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "clientInfo": { "name": "tester", "version": "1.0.0" },
      }),
    )
    .await;
  assert_eq!(started["serverInfo"]["name"], "bmo-chat");
  let served = match serving.await.expect("the session's task runs") {
    Ok(served) => served,
    Err(error) => panic!("the MCP session starts: {error}"),
  };
  harness.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;

  let send = json!({ "kind": "text", "roomId": -100, "fields": { "text": "Hello **there**" } });
  let send_request: bmo_chat_client::ChatSendRequest = serde_json::from_value(send.clone()).unwrap();
  let media = bmo_chat_client::ChatMediaRequest {
    room_id: -100,
    message_id: 5,
    sent_at: Some(1_000),
  };
  let query = bmo_chat_client::AvatarsQuery {
    after: Some(1_000),
    limit: Some(10),
  };
  let rooms = bmo_chat_client::AccountRoomsQuery {
    after: Some(-200),
    limit: Some(5),
  };
  let contacts = bmo_chat_client::ContactsQuery {
    after: Some(7),
    limit: Some(5),
  };
  let start = bmo_chat_client::ChatStart::OneToOne { contact_id: 42 };
  let calls = [
    ("chat_declaration", json!({}), as_json(client.declaration().await.unwrap())),
    ("chat_health", json!({}), as_json(client.health().await.unwrap())),
    ("chat_send", json!({ "body": send }), as_json(client.send(&send_request).await.unwrap())),
    (
      "chat_rooms",
      json!({ "after": -200, "limit": 5 }),
      as_json(client.account_rooms(&rooms).await.unwrap()),
    ),
    (
      "chat_start",
      json!({ "body": { "contactId": 42 } }),
      as_json(client.start_chat(&start).await.unwrap()),
    ),
    (
      "chat_contacts",
      json!({ "after": 7, "limit": 5 }),
      as_json(client.contacts(&contacts).await.unwrap()),
    ),
    ("chat_room", json!({ "roomId": -100 }), as_json(client.room(-100).await.unwrap())),
    ("chat_owner", json!({}), as_json(client.owner().await.unwrap())),
    (
      "chat_message_files",
      json!({ "roomId": -100, "messageId": 5, "sentAt": 1_000 }),
      as_json(client.message_files(&media).await.unwrap()),
    ),
    (
      "chat_dialect_reading",
      json!({ "body": { "text": "Hello **there**" } }),
      as_json(client.dialect_reading("Hello **there**").await.unwrap()),
    ),
    (
      "chat_dialect_check",
      json!({ "body": { "markup": "Hello there" } }),
      as_json(client.dialect_check("Hello there").await.unwrap()),
    ),
    (
      "chat_avatars",
      json!({ "after": 1_000, "limit": 10 }),
      as_json(client.avatars(&query).await.unwrap()),
    ),
  ];
  let by_client = stand.asked.lock().unwrap().clone();
  stand.asked.lock().unwrap().clear();
  for (tool, arguments, answer) in &calls {
    assert_eq!(&harness.tool(tool, arguments.clone()).await, answer, "{tool}");
  }
  let by_tools = stand.asked.lock().unwrap().clone();

  drop(harness);
  served.cancel().await.expect("the MCP session ends");
  drop(client);
  stop.send(()).ok();
  listening.await.expect("the provider stops cleanly");

  assert_eq!(by_tools, by_client, "each tool asked the provider what its route asks");
  assert_eq!(
    by_tools,
    [
      "send Text to -100",
      "rooms",
      "start OneToOne { contact_id: 42 }",
      "contacts",
      "room -100",
      "owner",
      "files -100 5 Some(1000)",
      "avatars"
    ],
  );
}

/// An answer of the client as the JSON it crosses as.
fn as_json(answer: impl serde::Serialize) -> Value {
  serde_json::to_value(answer).expect("an answer renders as JSON")
}

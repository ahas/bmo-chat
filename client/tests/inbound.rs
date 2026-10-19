//! The server for the receiver routes, driven in process over a stand-in receiver: every hand-over route,
//! its token, its body handed over decoded, and the error shape, as `docs/protocol.md` states them.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::Request;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bmo_chat_client::{
  ChatGathered, ChatHandover, ChatMessage, ChatReceiver, ChatVoice, ErrorCode, GatheredOutcome,
  InboundChatContext, MessageFile, RoomFacts, RoomKind, WireError, receiver_router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "the-receiver-token";

/// One hand-over as the stand-in was given it.
#[derive(Debug, Clone, PartialEq)]
enum Taken {
  Message(ChatHandover),
  Attachment(ChatHandover),
  Edited(ChatHandover),
  Gathered(ChatGathered),
}

/// The stand-in receiver. It keeps every hand-over it takes, and refuses each with the error it is told to.
#[derive(Default)]
struct Desk {
  taken: Mutex<Vec<Taken>>,
  refusing: Mutex<Option<WireError>>,
}

impl Desk {
  fn taken(&self) -> Vec<Taken> {
    self.taken.lock().unwrap().clone()
  }

  fn refuses(&self, error: WireError) {
    *self.refusing.lock().unwrap() = Some(error);
  }

  fn take(&self, taken: Taken) -> Result<(), WireError> {
    if let Some(error) = self.refusing.lock().unwrap().clone() {
      return Err(error);
    }
    self.taken.lock().unwrap().push(taken);
    Ok(())
  }
}

#[async_trait]
impl ChatReceiver for Desk {
  async fn message(&self, handover: ChatHandover) -> Result<(), WireError> {
    self.take(Taken::Message(handover))
  }
  async fn attachment(&self, handover: ChatHandover) -> Result<(), WireError> {
    self.take(Taken::Attachment(handover))
  }
  async fn edited(&self, handover: ChatHandover) -> Result<(), WireError> {
    self.take(Taken::Edited(handover))
  }
  async fn gathered(&self, gathered: ChatGathered) -> Result<(), WireError> {
    self.take(Taken::Gathered(gathered))
  }
}

fn router(desk: &Arc<Desk>) -> Router {
  receiver_router(desk.clone(), TOKEN)
}

/// One request with the given authorization header and raw body, answered with its status and JSON body.
async fn call_raw(
  router: &Router,
  authorization: Option<&str>,
  method: &str,
  path: &str,
  body: Vec<u8>,
) -> (u16, Value) {
  let mut request = Request::builder().method(method).uri(path).header("content-type", "application/json");
  if let Some(authorization) = authorization {
    request = request.header("authorization", authorization);
  }
  let answer = router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
  let status = answer.status().as_u16();
  let bytes = to_bytes(answer.into_body(), usize::MAX).await.unwrap();
  (status, serde_json::from_slice(&bytes).expect("every answer is JSON"))
}

async fn post(router: &Router, path: &str, body: &Value) -> (u16, Value) {
  let bearer = format!("Bearer {TOKEN}");
  call_raw(router, Some(&bearer), "POST", path, serde_json::to_vec(body).unwrap()).await
}

fn message(text: &str) -> ChatMessage {
  ChatMessage {
    chat_id: -100,
    message_id: 5,
    sender_id: Some(42),
    sender_name: "A Person".into(),
    sent_at: 1_760_000_000,
    text: text.into(),
    reply_to_message_id: None,
    via: "stand-in".into(),
    album: None,
    mode: ChatVoice::BotAccount,
  }
}

fn message_wire(text: &str) -> Value {
  json!({
    "chatId": -100, "messageId": 5, "senderId": 42, "senderName": "A Person", "sentAt": 1_760_000_000,
    "text": text, "replyToMessageId": null, "via": "stand-in", "album": null, "mode": "bot-account",
  })
}

#[tokio::test]
async fn every_hand_over_is_handed_to_the_receiver_decoded_and_answered_with_an_empty_object() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);

  let bare = json!({ "message": message_wire("said") });
  let carried = json!({
    "message": message_wire("carried"),
    "context": { "carriesFile": true, "gathering": true, "tag": "echo-1" },
    "room": { "kind": "owner", "identity": 7, "privateTo": null },
  });
  let with_file = json!({
    "message": message_wire("carried along"),
    "files": [{ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=" }],
  });
  let kept = json!({
    "chatId": -100, "messageId": 5, "via": "stand-in", "outcome": "kept",
    "files": [{ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=" }],
  });
  let not_kept = json!({
    "chatId": -100, "messageId": 5, "via": "stand-in", "outcome": "not-kept",
    "reason": "The file was too large to keep.",
  });
  for (path, body) in [
    ("/message", &bare),
    ("/attachment", &carried),
    ("/attachment", &with_file),
    ("/edited", &bare),
    ("/gathered", &kept),
    ("/gathered", &not_kept),
  ] {
    assert_eq!(post(&router, path, body).await, (200, json!({})), "{path}");
  }

  let file = MessageFile {
    id: "file-1".into(),
    name: "a.png".into(),
    mime: "image/png".into(),
    bytes: vec![137, 80],
  };
  let gathered = |outcome| ChatGathered {
    chat_id: -100,
    message_id: 5,
    via: "stand-in".into(),
    outcome,
  };
  assert_eq!(
    desk.taken(),
    [
      Taken::Message(ChatHandover {
        message: message("said"),
        context: None,
        room: None,
        files: vec![],
      }),
      Taken::Attachment(ChatHandover {
        message: message("carried"),
        context: Some(InboundChatContext {
          carries_file: true,
          gathering: true,
          tag: Some("echo-1".into()),
          ..InboundChatContext::default()
        }),
        room: Some(RoomFacts {
          kind: Some(RoomKind::Owner),
          identity: Some(7),
          private_to: None,
        }),
        files: vec![],
      }),
      Taken::Attachment(ChatHandover {
        message: message("carried along"),
        context: None,
        room: None,
        files: vec![file.clone()],
      }),
      Taken::Edited(ChatHandover {
        message: message("said"),
        context: None,
        room: None,
        files: vec![],
      }),
      Taken::Gathered(gathered(GatheredOutcome::Kept { files: vec![file] })),
      Taken::Gathered(gathered(GatheredOutcome::NotKept {
        reason: "The file was too large to keep.".into(),
      })),
    ]
  );
}

#[tokio::test]
async fn a_missing_or_wrong_token_is_answered_unauthorized_and_nothing_is_handed_over() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let body = serde_json::to_vec(&json!({ "message": message_wire("said") })).unwrap();
  for authorization in [None, Some("Bearer another-token"), Some(TOKEN)] {
    for path in ["/message", "/nowhere"] {
      let (status, answer) = call_raw(&router, authorization, "POST", path, body.clone()).await;
      assert_eq!(status, 401, "{authorization:?} {path}");
      assert_eq!(answer["code"], "unauthorized");
      assert_eq!(answer["error"], "The bearer token is missing or wrong.");
    }
  }
  assert_eq!(desk.taken(), []);
}

#[tokio::test]
async fn a_body_not_of_the_route_shape_is_a_bad_request_naming_what_is_missing() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let (status, answer) = post(&router, "/gathered", &json!({ "chatId": -100, "messageId": 5 })).await;
  assert_eq!(status, 400);
  assert_eq!(answer["code"], "bad-request");
  let error = answer["error"].as_str().unwrap();
  assert!(error.starts_with("The body is not the shape this route takes: "), "{error}");
  assert!(error.contains("via"), "{error}");

  let bearer = format!("Bearer {TOKEN}");
  let (status, answer) = call_raw(&router, Some(&bearer), "POST", "/message", b"not json".to_vec()).await;
  assert_eq!((status, answer["code"].clone()), (400, json!("bad-request")));
  assert_eq!(desk.taken(), []);
}

#[tokio::test]
async fn an_unknown_path_or_method_is_no_route() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let bearer = format!("Bearer {TOKEN}");
  for (method, path) in [("POST", "/archived"), ("POST", "/filed"), ("GET", "/message")] {
    let (status, answer) = call_raw(&router, Some(&bearer), method, path, vec![]).await;
    assert_eq!(status, 404, "{method} {path}");
    assert_eq!(
      answer,
      json!({ "code": "no-route", "error": format!("There is no route at {method} {path}.") })
    );
  }
}

#[tokio::test]
async fn a_hand_over_the_receiver_could_not_take_is_answered_with_its_error() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let body = json!({ "message": message_wire("said") });
  for error in [
    WireError::new(ErrorCode::Failed, "The archive is closed."),
    WireError::new(ErrorCode::NotReady, "The receiver is still starting."),
  ] {
    desk.refuses(error.clone());
    let (status, answer) = post(&router, "/message", &body).await;
    assert_eq!(status, error.code.status());
    assert_eq!(answer, serde_json::to_value(&error).unwrap());
  }
  assert_eq!(desk.taken(), []);
}

#[tokio::test]
async fn a_large_file_is_taken_whole() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let bytes = vec![7u8; 5 * 1024 * 1024];
  let body = json!({
    "message": message_wire("a large file"),
    "files": [{ "id": "file-1", "name": "large.bin", "mime": "application/octet-stream", "bytes": STANDARD.encode(&bytes) }],
  });
  assert_eq!(post(&router, "/attachment", &body).await, (200, json!({})));
  match &desk.taken()[..] {
    [Taken::Attachment(handover)] => assert_eq!(handover.files[0].bytes, bytes),
    taken => panic!("one attachment is taken, not {taken:?}"),
  }
}

#[tokio::test]
async fn the_routes_can_be_mounted_under_a_prefix() {
  let desk = Arc::new(Desk::default());
  let router = Router::new().nest("/receiver", router(&desk));
  let (status, _) = post(&router, "/receiver/message", &json!({ "message": message_wire("said") })).await;
  assert_eq!(status, 200);
  assert_eq!(desk.taken().len(), 1);
}

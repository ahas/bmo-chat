//! The server for the receiving-side routes, driven in process over a stand-in receiving side: every route,
//! its method, its token, what it hands the receiving side decoded, its answer and status, and the error
//! shape, as `docs/protocol.md` states them.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::Request;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use bmo_chat_client::{
  AvatarChange, ChatReceiver, ChatVoice, ErrorCode, GatheredPatch, HandedMessage, HeldMessage,
  InboundChatContext, MessageFile, MessageOrigin, RETRY_AFTER_SECONDS, ReceiverRoom, RoomFacts, RoomKind,
  RoomQuery, RoomsQuery, WireError, receiver_router,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "the-receiving-token";
const ROOM: i64 = -100;

/// One call as the stand-in was given it.
#[derive(Debug, Clone, PartialEq)]
enum Taken {
  HandOver(i64, i64, Box<HandedMessage>),
  Gathered(i64, i64, GatheredPatch),
  AvatarsChanged(AvatarChange),
  ReactionsChanged(bmo_chat_client::ReactionChange),
  Deleted(bmo_chat_client::Deletion),
  Room(i64, RoomQuery),
  Rooms(RoomsQuery),
}

/// A hand-over as the stand-in keeps it.
fn handed_over(room_id: i64, message_id: i64, message: HandedMessage) -> Taken {
  Taken::HandOver(room_id, message_id, Box::new(message))
}

/// The stand-in receiving side. It keeps every call it takes, holds the messages handed over, and refuses each
/// call with the error it is told to.
#[derive(Default)]
struct Desk {
  taken: Mutex<Vec<Taken>>,
  held: Mutex<Vec<(i64, i64, HandedMessage)>>,
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

  /// The rooms it holds, in ascending order of number: every room from -300 to -100, a hundred apart.
  fn rooms() -> Vec<ReceiverRoom> {
    [-300, -200, -100]
      .map(|id| ReceiverRoom {
        id,
        listened: id == ROOM,
        read: id == ROOM,
        oldest_message_id: Some(5),
        oldest_sent_at: Some(1_760_000_000_000),
      })
      .to_vec()
  }
}

#[async_trait]
impl ChatReceiver for Desk {
  /// A message held already is left as it is, unless the hand-over is an edit.
  async fn hand_over(
    &self,
    room_id: i64,
    message_id: i64,
    message: HandedMessage,
  ) -> Result<HeldMessage, WireError> {
    self.take(handed_over(room_id, message_id, message.clone()))?;
    let mut held = self.held.lock().unwrap();
    let place = held.iter().position(|(room, id, _)| (*room, *id) == (room_id, message_id));
    Ok(match place {
      None => {
        held.push((room_id, message_id, message.clone()));
        HeldMessage {
          created: true,
          message,
        }
      }
      Some(place) if message.origin == MessageOrigin::Edit => {
        held[place].2 = message.clone();
        HeldMessage {
          created: false,
          message,
        }
      }
      Some(place) => HeldMessage {
        created: false,
        message: held[place].2.clone(),
      },
    })
  }
  async fn gathered(
    &self,
    room_id: i64,
    message_id: i64,
    patch: GatheredPatch,
  ) -> Result<HandedMessage, WireError> {
    self.take(Taken::Gathered(room_id, message_id, patch.clone()))?;
    let mut held = self.held.lock().unwrap();
    let Some((_, _, message)) = held.iter_mut().find(|(room, id, _)| (*room, *id) == (room_id, message_id))
    else {
      return Err(WireError::new(ErrorCode::NotFound, "No such message is held."));
    };
    if !message.gathering {
      return Err(WireError::new(ErrorCode::Conflict, "The message promised no files."));
    }
    message.files = patch.files;
    message.gather_failure = patch.gather_failure;
    message.gathering = false;
    Ok(message.clone())
  }
  async fn avatars_changed(&self, change: AvatarChange) -> Result<(), WireError> {
    self.take(Taken::AvatarsChanged(change))
  }
  async fn reactions_changed(&self, change: bmo_chat_client::ReactionChange) -> Result<(), WireError> {
    self.take(Taken::ReactionsChanged(change))
  }
  async fn deleted(&self, deletion: bmo_chat_client::Deletion) -> Result<(), WireError> {
    self.take(Taken::Deleted(deletion))
  }
  async fn room(&self, room_id: i64, query: RoomQuery) -> Result<Option<ReceiverRoom>, WireError> {
    self.take(Taken::Room(room_id, query))?;
    Ok(Desk::rooms().into_iter().find(|room| room.id == room_id))
  }
  async fn rooms(&self, query: RoomsQuery) -> Result<Vec<ReceiverRoom>, WireError> {
    self.take(Taken::Rooms(query.clone()))?;
    let mut rooms: Vec<ReceiverRoom> = Desk::rooms()
      .into_iter()
      .filter(|room| query.after.is_none_or(|after| room.id > after))
      .collect();
    if let Some(limit) = query.limit {
      rooms.truncate(limit as usize);
    }
    Ok(rooms)
  }
}

fn router(desk: &Arc<Desk>) -> Router {
  receiver_router(desk.clone(), TOKEN)
}

/// An answer: its status, its `Retry-After` where it gave one, and its JSON body.
struct Answer {
  status: u16,
  retry_after: Option<String>,
  body: Value,
}

/// One request with the given authorization header and raw body.
async fn call_raw(
  router: &Router,
  authorization: Option<&str>,
  method: &str,
  path: &str,
  body: Vec<u8>,
) -> Answer {
  let mut request = Request::builder().method(method).uri(path).header("content-type", "application/json");
  if let Some(authorization) = authorization {
    request = request.header("authorization", authorization);
  }
  let answer = router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
  let status = answer.status().as_u16();
  let retry_after = answer.headers().get("retry-after").map(|value| value.to_str().unwrap().to_owned());
  let bytes = to_bytes(answer.into_body(), usize::MAX).await.unwrap();
  Answer {
    status,
    retry_after,
    body: serde_json::from_slice(&bytes).expect("every answer is JSON"),
  }
}

/// One request with the token, answered with its status and its JSON body.
async fn call(router: &Router, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
  let bearer = format!("Bearer {TOKEN}");
  let body = body.map(|body| serde_json::to_vec(body).unwrap()).unwrap_or_default();
  let Answer { status, body, .. } = call_raw(router, Some(&bearer), method, path, body).await;
  (status, body)
}

fn message(origin: MessageOrigin, text: &str) -> HandedMessage {
  HandedMessage {
    origin,
    sender_id: Some(42),
    sender_name: "A Person".into(),
    sent_at: 1_760_000_000_000,
    text: text.into(),
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

fn message_wire(origin: &str, text: &str) -> Value {
  json!({
    "origin": origin, "senderId": 42, "senderName": "A Person", "sentAt": 1_760_000_000_000_i64,
    "text": text, "replyToMessageId": null, "via": "stand-in", "album": null, "mode": "bot-account",
    "context": {}, "room": {}, "files": [], "gathering": false,
  })
}

fn file() -> MessageFile {
  MessageFile {
    id: "file-1".into(),
    name: "a.png".into(),
    mime: "image/png".into(),
    bytes: vec![137, 80].into(),
    kind: None,
  }
}

const MESSAGE: &str = "/rooms/-100/messages/5";

#[tokio::test]
async fn a_hand_over_is_answered_with_the_message_held_created_where_it_was_not_held_before() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let said = message_wire("live", "said");
  assert_eq!(call(&router, "PUT", MESSAGE, Some(&said)).await, (201, said.clone()));
  assert_eq!(
    call(&router, "PUT", MESSAGE, Some(&message_wire("history", "said long ago"))).await,
    (200, said.clone()),
    "a past message already held is left as it is"
  );
  let changed = message_wire("edit", "said again");
  assert_eq!(call(&router, "PUT", MESSAGE, Some(&changed)).await, (200, changed));
  let carried = json!({
    "origin": "live", "senderId": 42, "senderName": "A Person", "sentAt": 1_760_000_000_000_i64,
    "text": "carried", "replyToMessageId": null, "via": "stand-in", "album": null, "mode": "bot-account",
    "context": { "carriesFile": true, "tag": "echo-1" },
    "room": { "kind": "owner", "identity": 7 },
    "files": [{ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=" }],
    "gathering": false,
  });
  let mut answered = carried.clone();
  answered["files"] = json!([{ "id": "file-1", "name": "a.png", "mime": "image/png" }]);
  assert_eq!(
    call(&router, "PUT", "/rooms/-100/messages/6", Some(&carried)).await,
    (201, answered),
    "the answer names each file without its bytes"
  );
  let mut unborne = carried.clone();
  unborne["files"] = json!([{ "id": "file-2", "name": "b.png", "mime": "image/png" }]);
  let (status, answer) = call(&router, "PUT", "/rooms/-100/messages/8", Some(&unborne)).await;
  assert_eq!(
    (status, &answer["code"]),
    (400, &json!("bad-request")),
    "a file handed over carries its bytes"
  );

  assert_eq!(
    desk.taken(),
    [
      handed_over(ROOM, 5, message(MessageOrigin::Live, "said")),
      handed_over(ROOM, 5, message(MessageOrigin::History, "said long ago")),
      handed_over(ROOM, 5, message(MessageOrigin::Edit, "said again")),
      handed_over(
        ROOM,
        6,
        HandedMessage {
          context: InboundChatContext {
            carries_file: true,
            tag: Some("echo-1".into()),
            ..InboundChatContext::default()
          },
          room: RoomFacts {
            kind: Some(RoomKind::Owner),
            identity: Some(7),
            private_to: None,
          },
          files: vec![file()],
          ..message(MessageOrigin::Live, "carried")
        }
      ),
    ]
  );
}

#[tokio::test]
async fn the_gathered_files_patch_the_message_held() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let mut gathering = message_wire("live", "carried");
  gathering["gathering"] = json!(true);
  assert_eq!(call(&router, "PUT", MESSAGE, Some(&gathering)).await.0, 201);
  let kept =
    json!({ "files": [{ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=" }] });
  let mut patched = message_wire("live", "carried");
  patched["files"] = json!([{ "id": "file-1", "name": "a.png", "mime": "image/png" }]);
  assert_eq!(call(&router, "PATCH", MESSAGE, Some(&kept)).await, (200, patched));
  assert_eq!(
    call(&router, "PATCH", MESSAGE, Some(&kept)).await,
    (409, json!({ "code": "conflict", "error": "The message promised no files." })),
    "the receiving side's own refusal crosses at its code's status"
  );
  assert_eq!(
    call(&router, "PATCH", "/rooms/-100/messages/9", Some(&kept)).await,
    (404, json!({ "code": "not-found", "error": "No such message is held." }))
  );
  assert_eq!(desk.taken()[1], Taken::Gathered(ROOM, 5, GatheredPatch::kept(vec![file()])));
}

#[tokio::test]
async fn word_of_pictures_is_answered_created_with_itself() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let change = json!({ "via": "stand-in", "roomIds": [-100], "personIds": [42, 43] });
  assert_eq!(call(&router, "POST", "/avatar-changes", Some(&change)).await, (201, change));
  assert_eq!(
    desk.taken(),
    [Taken::AvatarsChanged(AvatarChange {
      via: "stand-in".into(),
      room_ids: vec![-100],
      person_ids: vec![42, 43],
    })]
  );
}

#[tokio::test]
async fn a_room_is_answered_as_held_and_one_not_held_is_not_found() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  assert_eq!(
    call(&router, "GET", "/rooms/-100?via=stand-in", None).await,
    (
      200,
      json!({
        "id": -100, "listened": true, "read": true, "oldestMessageId": 5,
        "oldestSentAt": 1_760_000_000_000_i64,
      })
    )
  );
  assert_eq!(
    call(&router, "GET", "/rooms/7?via=stand-in", None).await,
    (404, json!({ "code": "not-found", "error": "Nothing is held of room 7." }))
  );
  let asking = RoomQuery {
    via: "stand-in".into(),
  };
  assert_eq!(desk.taken(), [Taken::Room(ROOM, asking.clone()), Taken::Room(7, asking)]);
}

#[tokio::test]
async fn the_rooms_are_paged_by_asking_the_receiving_side_for_one_more_than_the_limit() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let ids = |answer: &Value| -> Vec<i64> {
    answer["items"]
      .as_array()
      .unwrap()
      .iter()
      .map(|room| room["id"].as_i64().unwrap())
      .collect()
  };
  let (status, page) = call(&router, "GET", "/rooms?via=stand-in&limit=2", None).await;
  assert_eq!((status, ids(&page), page["more"].clone()), (200, vec![-300, -200], json!(true)));
  let (status, page) = call(&router, "GET", "/rooms?via=stand-in&after=-200&limit=2", None).await;
  assert_eq!((status, ids(&page), page["more"].clone()), (200, vec![-100], json!(false)));
  let (status, page) = call(&router, "GET", "/rooms?via=stand-in", None).await;
  assert_eq!((status, ids(&page), page["more"].clone()), (200, vec![-300, -200, -100], json!(false)));
  let asked = |after, limit| {
    Taken::Rooms(RoomsQuery {
      via: "stand-in".into(),
      after,
      limit,
    })
  };
  assert_eq!(desk.taken(), [asked(None, Some(3)), asked(Some(-200), Some(3)), asked(None, None)]);
}

#[tokio::test]
async fn a_missing_or_wrong_token_is_answered_unauthorized_and_nothing_is_handed_over() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let body = serde_json::to_vec(&message_wire("live", "said")).unwrap();
  for authorization in [None, Some("Bearer another-token"), Some(TOKEN)] {
    for (method, path) in [("PUT", MESSAGE), ("GET", "/rooms?via=stand-in"), ("POST", "/nowhere")] {
      let answer = call_raw(&router, authorization, method, path, body.clone()).await;
      assert_eq!(answer.status, 401, "{authorization:?} {method} {path}");
      assert_eq!(
        answer.body,
        json!({ "code": "unauthorized", "error": "The bearer token is missing or wrong." })
      );
    }
  }
  assert_eq!(desk.taken(), []);
}

#[tokio::test]
async fn a_body_or_query_not_of_the_route_shape_is_a_bad_request_naming_what_is_missing() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let (status, answer) = call(&router, "POST", "/avatar-changes", Some(&json!({ "roomIds": [] }))).await;
  assert_eq!((status, answer["code"].clone()), (400, json!("bad-request")));
  let error = answer["error"].as_str().unwrap();
  assert!(error.starts_with("The body is not the shape this route takes: "), "{error}");
  assert!(error.contains("via"), "{error}");
  let bearer = format!("Bearer {TOKEN}");
  let answer = call_raw(&router, Some(&bearer), "PUT", MESSAGE, b"not json".to_vec()).await;
  assert_eq!((answer.status, answer.body["code"].clone()), (400, json!("bad-request")));
  let (status, answer) = call(&router, "GET", "/rooms", None).await;
  assert_eq!(
    (status, answer["code"].clone()),
    (400, json!("bad-request")),
    "the rooms are asked by `via`"
  );
  let error = answer["error"].as_str().unwrap();
  assert!(error.starts_with("The query is not the shape this route takes: "), "{error}");
  assert_eq!(desk.taken(), []);
}

#[tokio::test]
async fn an_unknown_path_or_method_or_a_key_that_is_not_a_number_is_no_route() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  for (method, path) in [
    ("POST", "/message"),
    ("POST", "/rooms"),
    ("GET", "/rooms/-100/messages/5"),
    ("GET", "/rooms/seven?via=stand-in"),
    ("PUT", "/rooms/-100/messages/five"),
  ] {
    let (status, answer) = call(&router, method, path, None).await;
    let path = path.split('?').next().unwrap();
    assert_eq!(
      (status, answer),
      (
        404,
        json!({ "code": "no-route", "error": format!("There is no route at {method} {path}.") })
      ),
      "{method} {path}"
    );
  }
  assert_eq!(desk.taken(), []);
}

#[tokio::test]
async fn a_call_the_receiving_side_could_not_take_is_answered_with_its_error() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let body = serde_json::to_vec(&message_wire("live", "said")).unwrap();
  let bearer = format!("Bearer {TOKEN}");
  desk.refuses(WireError::new(ErrorCode::Failed, "The archive is closed."));
  let answer = call_raw(&router, Some(&bearer), "PUT", MESSAGE, body.clone()).await;
  assert_eq!(
    (answer.status, answer.body, answer.retry_after),
    (500, json!({ "code": "failed", "error": "The archive is closed." }), None)
  );
  desk.refuses(WireError::new(ErrorCode::NotReady, "The receiving side is still starting."));
  let answer = call_raw(&router, Some(&bearer), "PUT", MESSAGE, body).await;
  assert_eq!(
    (answer.status, answer.body, answer.retry_after),
    (
      503,
      json!({ "code": "not-ready", "error": "The receiving side is still starting." }),
      Some(RETRY_AFTER_SECONDS.to_string())
    )
  );
  assert_eq!(desk.taken(), []);
}

#[tokio::test]
async fn a_large_file_is_taken_whole() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let bytes = vec![7u8; 5 * 1024 * 1024];
  let mut body = message_wire("live", "a large file");
  body["files"] = json!([{
    "id": "file-1", "name": "large.bin", "mime": "application/octet-stream", "bytes": STANDARD.encode(&bytes),
  }]);
  assert_eq!(call(&router, "PUT", MESSAGE, Some(&body)).await.0, 201);
  match &desk.taken()[..] {
    [Taken::HandOver(_, _, message)] => assert_eq!(message.files[0].bytes, bytes.into()),
    taken => panic!("one hand-over is taken, not {taken:?}"),
  }
}

#[tokio::test]
async fn the_routes_can_be_mounted_under_a_prefix() {
  let desk = Arc::new(Desk::default());
  let router = Router::new().nest("/receiving", router(&desk));
  let said = message_wire("live", "said");
  let (status, _) = call(&router, "PUT", &format!("/receiving{MESSAGE}"), Some(&said)).await;
  assert_eq!(status, 201);
  assert_eq!(desk.taken().len(), 1);
}

#[tokio::test]
async fn a_receiving_side_that_shows_neither_typing_nor_presence_takes_both_and_passes_them_over() {
  let desk = Arc::new(Desk::default());
  let router = router(&desk);
  let mark = json!({ "via": "stand-in", "roomId": -100, "personId": 42, "on": true, "lastsMs": null });
  assert_eq!(call(&router, "POST", "/typing-marks", Some(&mark)).await, (201, mark));
  let change = json!({
    "via": "stand-in",
    "people": [{ "personId": 42, "state": "online", "lastSeenAt": null }],
    "whole": false,
  });
  assert_eq!(call(&router, "POST", "/presence-changes", Some(&change)).await, (201, change));
  let not_a_mark = json!({ "via": "stand-in", "roomId": -100, "on": true });
  assert_eq!(call(&router, "POST", "/typing-marks", Some(&not_a_mark)).await.0, 400);
  assert_eq!(desk.taken(), [], "the default hands the stand-in nothing");
}

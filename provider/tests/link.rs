//! The link to the receiving side against a receiving side listening on the loopback: every receiving-side
//! route, its method, its token, its body and its answer, and offering a request again only while the
//! connection is refused. No test waits on the real clock, and every listener a test starts is taken down
//! before it ends.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{any, put};
use bmo_chat::{
  AvatarChange, ChatCapability, ChatDialect, ChatFailure, ChatNumberRanges, ChatNumberSpace, ChatNumbering,
  ChatPatience, ChatPresence, ChatPresenceState, ChatProvider, ChatReadMode, ChatReceiver,
  ChatReceiverLink, ChatSendRequest, ChatSendResult, ChatVoice, ChatWait, ErrorCode, GatheredPatch,
  HandedMessage, HeldMessage, InboundChatContext, LinkFailure, MessageFile, MessageOrigin, Page,
  PersonPresence, PlainDialect, PresenceChange, ProviderRoom, ReceiverRoom, RoomFacts, RoomQuery,
  RoomsQuery, TypingMark, WireError,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const TOKEN: &str = "the-receiving-token";
const ROOM: i64 = -100;

/// One call the receiving side took: its method, its path and query, the authorization header and the body.
#[derive(Debug, Clone, PartialEq)]
struct Took {
  method: String,
  path: String,
  authorization: Option<String>,
  body: Value,
}

type Handed = Arc<Mutex<Vec<Took>>>;

/// The room the stand-in holds, as it answers it.
fn held_room() -> Value {
  json!({ "id": ROOM, "listened": true, "read": true, "oldestMessageId": 5, "oldestSentAt": null })
}

/// A receiving side that takes every call under `/receiving` and keeps it. A hand-over is answered with the
/// message as handed, 200 for an edit and 201 for any other origin; a gathered patch with the message holding
/// the patch's files; word of pictures, a typing mark and a presence change each with itself; room `ROOM`
/// as held and any other room as not found; and the rooms with a page of that one room.
fn receiving(handed: Handed) -> Router {
  async fn take(
    State(handed): State<Handed>,
    Path(route): Path<String>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
  ) -> Response {
    let authorization = headers.get("authorization").map(|value| value.to_str().unwrap().to_owned());
    let body: Value = if body.is_empty() {
      Value::Null
    } else {
      serde_json::from_slice(&body).expect("the link sends JSON")
    };
    let path = match uri.query() {
      Some(query) => format!("{route}?{query}"),
      None => route.clone(),
    };
    let answer = match (method.as_str(), route.as_str()) {
      ("PUT", _) if body["origin"] == "edit" => (StatusCode::OK, Json(body.clone())).into_response(),
      ("PUT", _) => (StatusCode::CREATED, Json(body.clone())).into_response(),
      ("PATCH", _) => {
        let mut message = serde_json::to_value(handed_message(MessageOrigin::Live, "carried")).unwrap();
        message["files"] = body["files"].clone();
        message["gathering"] = json!(false);
        Json(message).into_response()
      }
      ("POST", "avatar-changes" | "typing-marks" | "presence-changes") => {
        (StatusCode::CREATED, Json(body.clone())).into_response()
      }
      ("GET", "rooms") => Json(json!({ "items": [held_room()], "more": true })).into_response(),
      ("GET", "rooms/-100") => Json(held_room()).into_response(),
      _ => (
        StatusCode::NOT_FOUND,
        Json(WireError::new(ErrorCode::NotFound, "Nothing is held there.")),
      )
        .into_response(),
    };
    handed.lock().unwrap().push(Took {
      method: method.to_string(),
      path,
      authorization,
      body,
    });
    answer
  }
  Router::new().nest("/receiving", Router::new().route("/{*route}", any(take)).with_state(handed))
}

/// A server on the loopback, until it is closed.
struct Listening {
  address: SocketAddr,
  stop: oneshot::Sender<()>,
  served: JoinHandle<()>,
}

impl Listening {
  async fn on(listener: TcpListener, router: Router) -> Self {
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let served = tokio::spawn(async move {
      axum::serve(listener, router)
        .with_graceful_shutdown(async {
          stopped.await.ok();
        })
        .await
        .expect("the server runs");
    });
    Self {
      address,
      stop,
      served,
    }
  }

  async fn start(router: Router) -> Self {
    Self::on(TcpListener::bind("127.0.0.1:0").await.unwrap(), router).await
  }

  fn base(&self) -> String {
    format!("http://{}/receiving/", self.address)
  }

  async fn close(self) {
    self.stop.send(()).ok();
    self.served.await.expect("the server stops cleanly");
  }
}

/// An address nothing listens at.
async fn closed_address() -> SocketAddr {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  listener.local_addr().unwrap()
}

/// A waiter that returns at once and keeps every interval it was asked to wait.
#[derive(Default)]
struct Recording {
  waits: Mutex<Vec<Duration>>,
}

impl Recording {
  fn waits(&self) -> Vec<Duration> {
    self.waits.lock().unwrap().clone()
  }
}

#[async_trait]
impl ChatWait for Recording {
  async fn wait(&self, interval: Duration) {
    self.waits.lock().unwrap().push(interval);
  }
}

fn patience(waiter: Arc<dyn ChatWait>, window: u64, every: u64) -> ChatPatience {
  ChatPatience {
    window: Duration::from_secs(window),
    every: Duration::from_secs(every),
    waiter,
  }
}

fn handed_message(origin: MessageOrigin, text: &str) -> HandedMessage {
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
    context: InboundChatContext {
      carries_file: true,
      ..InboundChatContext::default()
    },
    room: RoomFacts::default(),
    files: vec![],
    gathering: true,
    gather_failure: None,
  }
}

fn file() -> MessageFile {
  MessageFile {
    id: "file-1".into(),
    name: "a.png".into(),
    mime: "image/png".into(),
    bytes: vec![137, 80],
    kind: None,
  }
}

fn link_failure(failure: ChatFailure) -> LinkFailure {
  failure
    .downcast_ref::<LinkFailure>()
    .expect("the link fails with a link failure")
    .clone()
}

#[tokio::test]
async fn every_call_reaches_its_route_with_its_method_the_token_and_its_body() {
  let handed = Handed::default();
  let listening = Listening::start(receiving(handed.clone())).await;
  let waiter = Arc::new(Recording::default());
  let link = ChatReceiverLink::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));

  let said = handed_message(MessageOrigin::Live, "said");
  let changed = handed_message(MessageOrigin::Edit, "said again");
  assert_eq!(
    link.hand_over(ROOM, 5, said.clone()).await.expect("the message is taken"),
    HeldMessage {
      created: true,
      message: said.clone()
    }
  );
  assert_eq!(
    link.hand_over(ROOM, 5, changed.clone()).await.expect("the edit is taken"),
    HeldMessage {
      created: false,
      message: changed.clone()
    },
    "a 200 answer says the receiving side held the message before"
  );
  let kept = GatheredPatch::kept(vec![file()]);
  let not_kept = GatheredPatch::not_kept("The file was too large to keep.");
  let gathered = link.gathered(ROOM, 5, kept.clone()).await.expect("the kept files are taken");
  assert_eq!((gathered.files, gathered.gathering), (vec![file()], false));
  link.gathered(ROOM, 5, not_kept.clone()).await.expect("the reason is taken");
  let change = AvatarChange {
    via: "stand-in".into(),
    room_ids: vec![ROOM],
    person_ids: vec![42],
  };
  link.avatars_changed(change.clone()).await.expect("the word is taken");
  let asking = RoomQuery {
    via: "stand-in".into(),
  };
  let room = ReceiverRoom {
    id: ROOM,
    listened: true,
    read: true,
    oldest_message_id: Some(5),
    oldest_sent_at: None,
  };
  assert_eq!(link.room(ROOM, asking.clone()).await.expect("the room is answered"), Some(room));
  assert_eq!(
    link.room(13, asking).await.expect("a room not held is no failure"),
    None,
    "a room answered not-found reads as none"
  );
  let rooms = RoomsQuery {
    via: "stand-in".into(),
    after: Some(-200),
    limit: Some(10),
  };
  assert_eq!(
    link.rooms(rooms).await.expect("the rooms are answered"),
    Page {
      items: vec![room],
      more: true
    }
  );
  drop(link);
  listening.close().await;

  let bearer = Some(format!("Bearer {TOKEN}"));
  let took = |method: &str, path: &str, body: Value| Took {
    method: method.into(),
    path: path.into(),
    authorization: bearer.clone(),
    body,
  };
  let expected = [
    took("PUT", "rooms/-100/messages/5", serde_json::to_value(&said).unwrap()),
    took("PUT", "rooms/-100/messages/5", serde_json::to_value(&changed).unwrap()),
    took("PATCH", "rooms/-100/messages/5", serde_json::to_value(&kept).unwrap()),
    took("PATCH", "rooms/-100/messages/5", serde_json::to_value(&not_kept).unwrap()),
    took("POST", "avatar-changes", serde_json::to_value(&change).unwrap()),
    took("GET", "rooms/-100?via=stand-in", Value::Null),
    took("GET", "rooms/13?via=stand-in", Value::Null),
    took("GET", "rooms?after=-200&limit=10&via=stand-in", Value::Null),
  ];
  assert_eq!(*handed.lock().unwrap(), expected);
  assert_eq!(handed.lock().unwrap()[2].body["files"][0]["bytes"], "iVA=");
  assert_eq!(
    handed.lock().unwrap()[3].body,
    json!({ "files": [], "gatherFailure": "The file was too large to keep." })
  );
  assert_eq!(waiter.waits(), [], "nothing was refused, so nothing waited");
}

#[tokio::test]
async fn an_answer_that_is_not_success_is_never_offered_again() {
  let offers = Arc::new(AtomicUsize::new(0));
  let counted = offers.clone();
  let refusing = Router::new().route(
    "/receiving/rooms/{room}/messages/{message}",
    put(move || {
      let counted = counted.clone();
      async move {
        counted.fetch_add(1, Ordering::SeqCst);
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          Json(WireError::new(ErrorCode::Failed, "The archive is closed.")),
        )
      }
    }),
  );
  let listening = Listening::start(refusing).await;
  let waiter = Arc::new(Recording::default());
  let link = ChatReceiverLink::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));
  let said = handed_message(MessageOrigin::Live, "said");
  let failure = link_failure(link.hand_over(ROOM, 5, said).await.unwrap_err());
  let url = format!("http://{}/receiving/rooms/-100/messages/5", listening.address);
  drop(link);
  listening.close().await;

  assert_eq!(
    failure,
    LinkFailure::Answered {
      url: url.clone(),
      status: 500,
      answer: Some(WireError::new(ErrorCode::Failed, "The archive is closed.")),
    }
  );
  assert_eq!(failure.to_string(), format!("{url} answered 500: The archive is closed."));
  assert_eq!(offers.load(Ordering::SeqCst), 1);
  assert_eq!(waiter.waits(), []);
}

#[tokio::test]
async fn a_success_not_of_the_routes_answer_is_undecodable() {
  let answering_nothing = Router::new().route("/receiving/rooms/{room}", any(|| async { Json(json!({})) }));
  let listening = Listening::start(answering_nothing).await;
  let link = ChatReceiverLink::new(listening.base(), TOKEN);
  let asking = RoomQuery {
    via: "stand-in".into(),
  };
  let failure = link_failure(link.room(ROOM, asking).await.unwrap_err());
  let url = format!("http://{}/receiving/rooms/-100", listening.address);
  drop(link);
  listening.close().await;
  assert!(
    matches!(&failure, LinkFailure::Undecodable { url: at, .. } if *at == url),
    "{failure:?}"
  );
}

#[tokio::test]
async fn an_unknown_route_answer_is_kept_as_it_came() {
  let listening = Listening::start(Router::new()).await;
  let link = ChatReceiverLink::new(listening.base(), TOKEN);
  let failure = link_failure(
    link
      .hand_over(ROOM, 5, handed_message(MessageOrigin::Edit, "said again"))
      .await
      .unwrap_err(),
  );
  drop(link);
  listening.close().await;
  assert!(
    matches!(
      failure,
      LinkFailure::Answered {
        status: 404,
        answer: None,
        ..
      }
    ),
    "{failure:?}"
  );
}

#[tokio::test]
async fn a_refused_connection_is_offered_again_until_the_window_is_spent() {
  let address = closed_address().await;
  for (window, every, waits) in [(6, 2, vec![2, 2, 2]), (5, 2, vec![2, 2]), (6, 0, vec![])] {
    let waiter = Arc::new(Recording::default());
    let link = ChatReceiverLink::new(format!("http://{address}"), TOKEN).with_patience(patience(
      waiter.clone(),
      window,
      every,
    ));
    let said = handed_message(MessageOrigin::Live, "said");
    let failure = link_failure(link.hand_over(ROOM, 5, said).await.unwrap_err());
    let waits: Vec<Duration> = waits.into_iter().map(Duration::from_secs).collect();
    match failure {
      LinkFailure::Unreachable { url, waited, why } => {
        assert_eq!(url, format!("http://{address}/rooms/-100/messages/5"));
        assert_eq!(waited, waits.iter().sum::<Duration>());
        assert!(why.to_lowercase().contains("refused"), "{why}");
      }
      failure => panic!("a refused connection is unreachable, not {failure:?}"),
    }
    assert_eq!(waiter.waits(), waits, "a window of {window}s every {every}s");
  }
}

/// A waiter that, on its second wait, starts the receiving side at the address that was refusing.
struct Restarting {
  address: SocketAddr,
  handed: Handed,
  waits: AtomicUsize,
  started: Mutex<Option<Listening>>,
}

#[async_trait]
impl ChatWait for Restarting {
  async fn wait(&self, _interval: Duration) {
    if self.waits.fetch_add(1, Ordering::SeqCst) == 1 {
      let listener = TcpListener::bind(self.address).await.expect("the freed address binds again");
      let listening = Listening::on(listener, receiving(self.handed.clone())).await;
      *self.started.lock().unwrap() = Some(listening);
    }
  }
}

#[tokio::test]
async fn a_receiving_side_that_comes_back_within_the_window_is_handed_the_request_once() {
  let address = closed_address().await;
  let restarting = Arc::new(Restarting {
    address,
    handed: Handed::default(),
    waits: AtomicUsize::new(0),
    started: Mutex::new(None),
  });
  let link = ChatReceiverLink::new(format!("http://{address}/receiving"), TOKEN).with_patience(patience(
    restarting.clone(),
    120,
    2,
  ));
  let said = handed_message(MessageOrigin::Live, "said");
  let taken = link.hand_over(ROOM, 5, said.clone()).await;
  drop(link);
  let listening = restarting.started.lock().unwrap().take().expect("the receiving side was started");
  listening.close().await;

  taken.expect("the message is taken once the receiving side is back");
  assert_eq!(restarting.waits.load(Ordering::SeqCst), 2);
  let handed = restarting.handed.lock().unwrap();
  assert_eq!(handed.len(), 1);
  assert_eq!((handed[0].method.as_str(), handed[0].path.as_str()), ("PUT", "rooms/-100/messages/5"));
  assert_eq!(handed[0].body, serde_json::to_value(&said).unwrap());
}

#[tokio::test]
async fn a_connection_dropped_after_it_was_taken_is_never_offered_again() {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = listener.local_addr().unwrap();
  let accepted = Arc::new(AtomicUsize::new(0));
  let counted = accepted.clone();
  let dropping = tokio::spawn(async move {
    loop {
      let (stream, _) = listener.accept().await.unwrap();
      counted.fetch_add(1, Ordering::SeqCst);
      drop(stream);
    }
  });
  let waiter = Arc::new(Recording::default());
  let link =
    ChatReceiverLink::new(format!("http://{address}"), TOKEN).with_patience(patience(waiter.clone(), 6, 2));
  let carried = handed_message(MessageOrigin::Live, "carried");
  let failure = link_failure(link.hand_over(ROOM, 5, carried).await.unwrap_err());
  drop(link);
  dropping.abort();
  assert!(dropping.await.unwrap_err().is_cancelled());

  assert!(matches!(failure, LinkFailure::Transport { .. }), "{failure:?}");
  assert_eq!(accepted.load(Ordering::SeqCst), 1);
  assert_eq!(waiter.waits(), []);
}

#[test]
fn the_default_patience_is_two_minutes_every_two_seconds() {
  let patience = ChatPatience::default();
  assert_eq!(patience.window, Duration::from_secs(120));
  assert_eq!(patience.every, Duration::from_secs(2));
}

/// A provider that declares only the capabilities it is set to, as they stand at each call.
struct Declaring(Mutex<BTreeSet<ChatCapability>>);

impl Declaring {
  fn declare(&self, can: &[ChatCapability]) {
    *self.0.lock().unwrap() = can.iter().copied().collect();
  }
}

#[async_trait]
impl ChatProvider for Declaring {
  fn key(&self) -> &str {
    "stand-in"
  }
  fn label(&self) -> &str {
    "Stand-in"
  }
  fn version(&self) -> &str {
    "1.0.0"
  }
  fn configured(&self) -> Option<String> {
    None
  }
  async fn start(&self, _receiver: Arc<dyn ChatReceiver>) -> Result<(), ChatFailure> {
    Ok(())
  }
  async fn stop(&self) {}
  fn modes(&self) -> Vec<ChatReadMode> {
    Vec::new()
  }
  fn can(&self) -> BTreeSet<ChatCapability> {
    self.0.lock().unwrap().clone()
  }
  fn dialect(&self) -> &dyn ChatDialect {
    &PlainDialect
  }
  fn numbering(&self) -> ChatNumbering {
    ChatNumbering {
      rooms: ChatNumberSpace::Native,
      people: ChatNumberSpace::Native,
      messages: ChatNumberSpace::Native,
      ranges: ChatNumberRanges {
        rooms: None,
        people: None,
      },
    }
  }
  async fn room(&self, _room_id: i64) -> Result<Option<ProviderRoom>, ChatFailure> {
    Ok(None)
  }
  async fn owner_room(&self) -> Result<Option<i64>, ChatFailure> {
    Ok(None)
  }
  async fn send(&self, request: ChatSendRequest) -> Result<ChatSendResult, ChatFailure> {
    Ok(ChatSendResult::Refused(self.unsupported(request.send.kind().into())))
  }
}

#[tokio::test]
async fn a_mark_or_a_change_goes_only_while_the_provider_declares_what_hears_it() {
  let handed = Handed::default();
  let listening = Listening::start(receiving(handed.clone())).await;
  let waiter = Arc::new(Recording::default());
  let mark = TypingMark {
    via: "stand-in".into(),
    room_id: ROOM,
    person_id: 42,
    on: true,
    lasts_ms: None,
  };
  let change = PresenceChange {
    via: "stand-in".into(),
    people: vec![PersonPresence {
      person_id: 42,
      presence: ChatPresence {
        state: ChatPresenceState::Away,
        last_seen_at: Some(1_700_000_000_000),
      },
    }],
    whole: true,
  };
  let tell = |link: ChatReceiverLink| {
    let (mark, change) = (mark.clone(), change.clone());
    async move {
      link.typing_marked(mark).await.expect("a mark is never refused for being undeclared");
      link
        .presence_changed(change)
        .await
        .expect("a change is never refused for being undeclared");
    }
  };
  let link = ChatReceiverLink::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));
  tell(link.clone()).await;

  let provider = Arc::new(Declaring(Mutex::default()));
  let told = link.declared_by(&provider);
  tell(told.clone()).await;
  provider.declare(&[ChatCapability::PeopleTyping]);
  tell(told.clone()).await;
  provider.declare(&[ChatCapability::Presence]);
  tell(told.clone()).await;
  drop(provider);
  tell(told).await;
  listening.close().await;

  let bearer = Some(format!("Bearer {TOKEN}"));
  assert_eq!(
    *handed.lock().unwrap(),
    [
      Took {
        method: "POST".into(),
        path: "typing-marks".into(),
        authorization: bearer.clone(),
        body: json!({ "via": "stand-in", "roomId": ROOM, "personId": 42, "on": true, "lastsMs": null }),
      },
      Took {
        method: "POST".into(),
        path: "presence-changes".into(),
        authorization: bearer,
        body: json!({
          "via": "stand-in",
          "people": [{ "personId": 42, "state": "away", "lastSeenAt": 1_700_000_000_000_i64 }],
          "whole": true,
        }),
      },
    ],
    "a link told of no provider, one declaring neither, or one no longer running sends nothing"
  );
  assert_eq!(waiter.waits(), [], "nothing was refused, so nothing waited");
}

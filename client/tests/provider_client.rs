//! The provider client against a stand-in provider listening on the loopback: every provider route, its
//! method, token and body, its answer decoded, typed refusals and the error shape, and offering a request
//! again only while the connection is refused. No test waits on the real clock, and every listener a test
//! starts is taken down before it ends.

use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use bmo_chat_client::{
  ChatCapability, ChatDeclaration, ChatDialectDeclaration, ChatDialectLimits, ChatHealth, ChatMarkup,
  ChatMediaRequest, ChatMediaResult, ChatModeStanding, ChatNumberSpace, ChatNumbering, ChatPatience,
  ChatProviderClient, ChatReach, ChatReadMode, ChatRefusal, ChatSend, ChatSendRequest, ChatSendResult,
  ChatVoice, ChatWait, ClientFailure, DialectChecked, DialectReading, ErrorCode, FileItem, MissedPicture,
  MissedPictureReason, RoomKind, WireError,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const TOKEN: &str = "the-provider-token";

/// What the stand-in was asked: the method, the path, the authorization header and the body, if any.
type Asked = Vec<(String, String, Option<String>, Option<Value>)>;

/// A stand-in provider: it answers each path it is given an answer for, behind the token, and keeps every
/// request it is asked.
#[derive(Clone, Default)]
struct Stand {
  answers: Arc<HashMap<String, (StatusCode, Value)>>,
  asked: Arc<Mutex<Asked>>,
}

impl Stand {
  fn answering(answers: impl IntoIterator<Item = (&'static str, StatusCode, Value)>) -> Self {
    let answers = answers
      .into_iter()
      .map(|(path, status, body)| (path.to_owned(), (status, body)))
      .collect();
    Self {
      answers: Arc::new(answers),
      asked: Arc::default(),
    }
  }

  fn asked(&self) -> Asked {
    self.asked.lock().unwrap().clone()
  }

  fn router(&self) -> Router {
    async fn answer(
      State(stand): State<Stand>,
      method: Method,
      uri: Uri,
      headers: HeaderMap,
      body: Bytes,
    ) -> Response {
      let authorization = headers.get("authorization").map(|value| value.to_str().unwrap().to_owned());
      let path = uri.path().to_owned();
      let body = (!body.is_empty()).then(|| serde_json::from_slice(&body).expect("the client posts JSON"));
      if body.is_some() {
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
      }
      stand
        .asked
        .lock()
        .unwrap()
        .push((method.to_string(), path.clone(), authorization.clone(), body));
      if authorization != Some(format!("Bearer {TOKEN}")) {
        let wrong = WireError::new(ErrorCode::Unauthorized, "The token is missing or wrong.");
        return (StatusCode::UNAUTHORIZED, Json(wrong)).into_response();
      }
      match stand.answers.get(&path) {
        Some((status, body)) => (*status, Json(body.clone())).into_response(),
        None => (StatusCode::NOT_FOUND, "nothing here").into_response(),
      }
    }
    Router::new().fallback(answer).with_state(self.clone())
  }
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
    format!("http://{}/provider/", self.address)
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

fn declaration() -> (ChatDeclaration, Value) {
  let declaration = ChatDeclaration {
    key: "platform".into(),
    label: "Platform".into(),
    configured: None,
    modes: vec![ChatReadMode {
      voice: ChatVoice::BotAccount,
      reach: ChatReach::AllMessages,
      standing: ChatModeStanding::Active,
      reason: None,
      can_reply_to: true,
    }],
    can: BTreeSet::from([ChatCapability::Text, ChatCapability::FetchMedia]),
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
    },
    speakers: false,
  };
  let wire = json!({
    "key": "platform", "label": "Platform", "configured": null,
    "modes": [{
      "voice": "bot-account", "reach": "all-messages", "standing": "active", "reason": null,
      "canReplyTo": true,
    }],
    "can": ["text", "fetchMedia"],
    "dialect": {
      "name": "plain",
      "limits": { "text": 4096, "caption": null, "albumPhotos": null, "fileBytes": null },
      "writing": "This room shows no formatting.",
      "foldable": false,
    },
    "shows": null,
    "numbering": { "rooms": "native", "people": "native", "messages": "native" },
    "speakers": false,
  });
  (declaration, wire)
}

fn text(words: &str) -> ChatSendRequest {
  ChatSendRequest::new(
    -100,
    ChatSend::Text {
      text: words.into(),
      reply_to: None,
      silent: false,
      fold: false,
    },
  )
}

#[tokio::test]
async fn every_provider_route_is_called_with_its_method_token_and_body_and_its_answer_decodes() {
  let (declared, declaration_wire) = declaration();
  let ok = StatusCode::OK;
  let stand = Stand::answering([
    ("/provider/declaration", ok, declaration_wire),
    ("/provider/health", ok, json!({ "ready": true, "reason": null })),
    ("/provider/send", ok, json!({ "sent": true, "messageId": 77 })),
    (
      "/provider/media",
      ok,
      json!({ "outcome": "fetched", "name": "a.png", "mime": "image/png", "bytes": "iVA=" }),
    ),
    (
      "/provider/dialect/read",
      ok,
      json!({ "markup": "hello", "plain": "hello", "formatted": false, "parts": ["hello"], "fold": null }),
    ),
    ("/provider/dialect/check", ok, json!({ "fault": null, "visible": "shut" })),
    ("/provider/room/owns", ok, json!({ "owns": true })),
    ("/provider/room/identity", ok, json!({ "identity": 7 })),
    ("/provider/room/is-owner", ok, json!({ "kind": "owner" })),
    ("/provider/room/private-of", ok, json!({ "person": null })),
    ("/provider/room/title", ok, json!({ "title": "A Room" })),
    ("/provider/owner-room", ok, json!({ "chatId": -100 })),
  ]);
  let listening = Listening::start(stand.router()).await;
  let waiter = Arc::new(Recording::default());
  let client =
    ChatProviderClient::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));
  let media = ChatMediaRequest {
    chat_id: -100,
    message_id: 5,
    sent_at: Some(1_760_000_000),
  };

  assert_eq!(client.declaration().await.unwrap(), declared);
  assert_eq!(client.health().await.unwrap(), ChatHealth::ready());
  assert_eq!(
    client.send(&text("hello")).await.unwrap(),
    ChatSendResult::Sent {
      message_id: Some(77),
      missed: vec![],
    }
  );
  assert_eq!(
    client.media(&media).await.unwrap(),
    ChatMediaResult::Fetched(FileItem {
      name: "a.png".into(),
      mime: "image/png".into(),
      bytes: vec![137, 80],
    })
  );
  assert_eq!(
    client.dialect_read("hello").await.unwrap(),
    DialectReading {
      markup: ChatMarkup {
        markup: "hello".into(),
        plain: "hello".into(),
        formatted: false,
      },
      parts: vec!["hello".into()],
      fold: None,
    }
  );
  assert_eq!(
    client.dialect_check("shut").await.unwrap(),
    DialectChecked {
      fault: None,
      visible: "shut".into(),
    }
  );
  assert!(client.room_owns(-100).await.unwrap());
  assert_eq!(client.room_identity(-100).await.unwrap(), Some(7));
  assert_eq!(client.room_kind(-100).await.unwrap(), RoomKind::Owner);
  assert_eq!(client.room_private_of(-100).await.unwrap(), None);
  assert_eq!(client.room_title(-100).await.unwrap(), Some("A Room".into()));
  assert_eq!(client.owner_room().await.unwrap(), Some(-100));
  drop(client);
  listening.close().await;

  let bearer = Some(format!("Bearer {TOKEN}"));
  let room = Some(json!({ "chatId": -100 }));
  let expected: Asked = [
    ("GET", "declaration", None),
    ("GET", "health", None),
    ("POST", "send", Some(json!({ "verb": "text", "chatId": -100, "text": "hello" }))),
    (
      "POST",
      "media",
      Some(json!({ "chatId": -100, "messageId": 5, "sentAt": 1_760_000_000 })),
    ),
    ("POST", "dialect/read", Some(json!({ "text": "hello" }))),
    ("POST", "dialect/check", Some(json!({ "markup": "shut" }))),
    ("POST", "room/owns", room.clone()),
    ("POST", "room/identity", room.clone()),
    ("POST", "room/is-owner", room.clone()),
    ("POST", "room/private-of", room.clone()),
    ("POST", "room/title", room.clone()),
    ("GET", "owner-room", None),
  ]
  .into_iter()
  .map(|(method, route, body)| (method.to_owned(), format!("/provider/{route}"), bearer.clone(), body))
  .collect();
  assert_eq!(stand.asked(), expected);
  assert_eq!(waiter.waits(), [], "nothing was refused, so nothing waited");
}

#[tokio::test]
async fn refusals_are_answers_and_decode_typed() {
  let ok = StatusCode::OK;
  let unsupported = json!({
    "code": "unsupported-capability", "message": "Platform cannot pin a message.", "label": "Platform",
    "capability": "pin",
  });
  let stand = Stand::answering([
    (
      "/provider/send",
      ok,
      json!({ "sent": false, "refusal": unsupported.clone(), "gone": false }),
    ),
    ("/provider/media", ok, json!({ "outcome": "refused", "refusal": unsupported })),
  ]);
  let listening = Listening::start(stand.router()).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let refusal = ChatRefusal::UnsupportedCapability {
    label: "Platform".into(),
    capability: ChatCapability::Pin,
  };

  let sent = client.send(&text("hello")).await.unwrap();
  let fetched = client
    .media(&ChatMediaRequest {
      chat_id: -100,
      message_id: 5,
      sent_at: None,
    })
    .await
    .unwrap();
  drop(client);
  listening.close().await;

  assert_eq!(
    sent,
    ChatSendResult::Refused {
      refusal: refusal.clone(),
      gone: false,
    }
  );
  assert_eq!(fetched, ChatMediaResult::Refused { refusal });
}

#[tokio::test]
async fn missed_pictures_and_an_unavailable_file_decode_typed() {
  let ok = StatusCode::OK;
  let stand = Stand::answering([
    (
      "/provider/send",
      ok,
      json!({
        "sent": true, "messageId": null,
        "missed": [{
          "address": "/elsewhere/a.png",
          "reason": { "code": "outside-data-root", "message": "whatever the provider rendered" },
        }],
      }),
    ),
    (
      "/provider/media",
      ok,
      json!({ "outcome": "unavailable", "reason": "The platform no longer has it." }),
    ),
  ]);
  let listening = Listening::start(stand.router()).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let sent = client.send(&text("an album went")).await.unwrap();
  let fetched = client
    .media(&ChatMediaRequest {
      chat_id: -100,
      message_id: 5,
      sent_at: None,
    })
    .await
    .unwrap();
  drop(client);
  listening.close().await;

  assert_eq!(
    sent,
    ChatSendResult::Sent {
      message_id: None,
      missed: vec![MissedPicture {
        address: "/elsewhere/a.png".into(),
        reason: MissedPictureReason::OutsideDataRoot,
      }],
    }
  );
  assert_eq!(
    fetched,
    ChatMediaResult::Unavailable {
      reason: "The platform no longer has it.".into(),
    }
  );
}

#[tokio::test]
async fn the_error_shape_is_decoded_and_never_offered_again() {
  let not_ready = WireError::new(ErrorCode::NotReady, "The platform connection is not open yet.");
  let stand = Stand::answering([(
    "/provider/room/owns",
    StatusCode::CONFLICT,
    serde_json::to_value(&not_ready).unwrap(),
  )]);
  let listening = Listening::start(stand.router()).await;
  let waiter = Arc::new(Recording::default());
  let client =
    ChatProviderClient::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));
  let failure = client.room_owns(-100).await.unwrap_err();
  let url = format!("http://{}/provider/room/owns", listening.address);
  drop(client);
  listening.close().await;

  assert_eq!(
    failure,
    ClientFailure::Answered {
      url: url.clone(),
      status: 409,
      answer: Some(not_ready),
    }
  );
  assert_eq!(
    failure.to_string(),
    format!("{url} answered 409: The platform connection is not open yet.")
  );
  assert_eq!(stand.asked().len(), 1);
  assert_eq!(waiter.waits(), []);
}

#[tokio::test]
async fn a_wrong_token_is_answered_unauthorized() {
  let stand =
    Stand::answering([("/provider/health", StatusCode::OK, json!({ "ready": true, "reason": null }))]);
  let listening = Listening::start(stand.router()).await;
  let client = ChatProviderClient::new(listening.base(), "another-token");
  let failure = client.health().await.unwrap_err();
  drop(client);
  listening.close().await;

  match failure {
    ClientFailure::Answered {
      status: 401,
      answer: Some(answer),
      ..
    } => assert_eq!(answer.code, ErrorCode::Unauthorized),
    failure => panic!("a wrong token is answered 401, not {failure:?}"),
  }
  assert_eq!(stand.asked()[0].2, Some("Bearer another-token".into()));
}

#[tokio::test]
async fn an_answer_without_the_error_shape_is_kept_as_it_came() {
  let stand = Stand::default();
  let listening = Listening::start(stand.router()).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let failure = client.room_title(-100).await.unwrap_err();
  drop(client);
  listening.close().await;
  assert!(
    matches!(
      failure,
      ClientFailure::Answered {
        status: 404,
        answer: None,
        ..
      }
    ),
    "{failure:?}"
  );
}

#[tokio::test]
async fn a_success_in_another_shape_is_undecodable() {
  let stand = Stand::answering([("/provider/health", StatusCode::OK, json!({ "ready": "yes" }))]);
  let listening = Listening::start(stand.router()).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let failure = client.health().await.unwrap_err();
  drop(client);
  listening.close().await;
  match failure {
    ClientFailure::Undecodable { url, why } => {
      assert!(url.ends_with("/provider/health"), "{url}");
      assert!(why.contains("invalid type"), "{why}");
    }
    failure => panic!("a success in another shape is undecodable, not {failure:?}"),
  }
}

#[tokio::test]
async fn each_client_reaches_only_its_own_provider() {
  let first = Stand::answering([("/provider/room/title", StatusCode::OK, json!({ "title": "First" }))]);
  let second = Stand::answering([("/provider/room/title", StatusCode::OK, json!({ "title": "Second" }))]);
  let first_listening = Listening::start(first.router()).await;
  let second_listening = Listening::start(second.router()).await;
  let to_first = ChatProviderClient::new(first_listening.base(), TOKEN);
  let to_second = ChatProviderClient::new(second_listening.base(), TOKEN);
  let titles = (to_first.room_title(-1).await.unwrap(), to_second.room_title(-2).await.unwrap());
  drop((to_first, to_second));
  first_listening.close().await;
  second_listening.close().await;

  assert_eq!(titles, (Some("First".into()), Some("Second".into())));
  assert_eq!(first.asked()[0].3, Some(json!({ "chatId": -1 })));
  assert_eq!(second.asked()[0].3, Some(json!({ "chatId": -2 })));
  assert_eq!((first.asked().len(), second.asked().len()), (1, 1));
}

#[tokio::test]
async fn a_refused_connection_is_offered_again_until_the_window_is_spent() {
  let address = closed_address().await;
  for (window, every, waits) in [(6, 2, vec![2, 2, 2]), (5, 2, vec![2, 2]), (6, 0, vec![])] {
    let waiter = Arc::new(Recording::default());
    let client = ChatProviderClient::new(format!("http://{address}"), TOKEN).with_patience(patience(
      waiter.clone(),
      window,
      every,
    ));
    let failure = client.send(&text("hello")).await.unwrap_err();
    let waits: Vec<Duration> = waits.into_iter().map(Duration::from_secs).collect();
    match failure {
      ClientFailure::Unreachable { url, waited, why } => {
        assert_eq!(url, format!("http://{address}/send"));
        assert_eq!(waited, waits.iter().sum::<Duration>());
        assert!(why.to_lowercase().contains("refused"), "{why}");
      }
      failure => panic!("a refused connection is unreachable, not {failure:?}"),
    }
    assert_eq!(waiter.waits(), waits, "a window of {window}s every {every}s");
  }
}

/// A waiter that, on its second wait, starts the provider at the address that was refusing.
struct Restarting {
  address: SocketAddr,
  stand: Stand,
  waits: AtomicUsize,
  started: Mutex<Option<Listening>>,
}

#[async_trait]
impl ChatWait for Restarting {
  async fn wait(&self, _interval: Duration) {
    if self.waits.fetch_add(1, Ordering::SeqCst) == 1 {
      let listener = TcpListener::bind(self.address).await.expect("the freed address binds again");
      let listening = Listening::on(listener, self.stand.router()).await;
      *self.started.lock().unwrap() = Some(listening);
    }
  }
}

#[tokio::test]
async fn a_provider_that_comes_back_within_the_window_is_sent_the_request_once() {
  let address = closed_address().await;
  let restarting = Arc::new(Restarting {
    address,
    stand: Stand::answering([("/provider/send", StatusCode::OK, json!({ "sent": true, "messageId": 9 }))]),
    waits: AtomicUsize::new(0),
    started: Mutex::new(None),
  });
  let client = ChatProviderClient::new(format!("http://{address}/provider"), TOKEN)
    .with_patience(patience(restarting.clone(), 120, 2));
  let sent = client.send(&text("hello")).await;
  drop(client);
  let listening = restarting.started.lock().unwrap().take().expect("the provider was started");
  listening.close().await;

  assert_eq!(
    sent.expect("the send goes once the provider is back"),
    ChatSendResult::Sent {
      message_id: Some(9),
      missed: vec![],
    }
  );
  assert_eq!(restarting.waits.load(Ordering::SeqCst), 2);
  let asked = restarting.stand.asked();
  assert_eq!(asked.len(), 1);
  assert_eq!(asked[0].1, "/provider/send");
  assert_eq!(asked[0].3, Some(serde_json::to_value(text("hello")).unwrap()));
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
  let client = ChatProviderClient::new(format!("http://{address}"), TOKEN).with_patience(patience(
    waiter.clone(),
    6,
    2,
  ));
  let failure = client.send(&text("hello")).await.unwrap_err();
  drop(client);
  dropping.abort();
  assert!(dropping.await.unwrap_err().is_cancelled());

  assert!(matches!(failure, ClientFailure::Transport { .. }), "{failure:?}");
  assert_eq!(accepted.load(Ordering::SeqCst), 1);
  assert_eq!(waiter.waits(), []);
}

#[test]
fn the_default_patience_is_two_minutes_every_two_seconds() {
  let patience = ChatPatience::default();
  assert_eq!(patience.window, Duration::from_secs(120));
  assert_eq!(patience.every, Duration::from_secs(2));
}

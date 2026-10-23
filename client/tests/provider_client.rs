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
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use bmo_chat_client::{
  AccountRoomsQuery, AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatCapability, ChatDeclaration,
  ChatDialectDeclaration, ChatDialectLimits, ChatHealth, ChatMarkup, ChatMediaRequest, ChatModeStanding,
  ChatNumberRanges, ChatNumberSpace, ChatNumbering, ChatPatience, ChatPicture, ChatProviderClient,
  ChatReach, ChatReadMode, ChatSend, ChatSendRequest, ChatSent, ChatVoice, ChatWait, ClientFailure,
  DialectChecked, DialectReading, ErrorCode, MessageFile, MissedPicture, MissedPictureReason, Page,
  ProviderOwner, ProviderRoom, RoomKind, WireError,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const TOKEN: &str = "the-provider-token";

/// What the stand-in was asked: the method, the path with its query, the authorization header and the body,
/// if any.
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
      let asked_at = uri.path_and_query().map(|at| at.as_str().to_owned()).unwrap_or_default();
      let body = (!body.is_empty()).then(|| serde_json::from_slice(&body).expect("the client posts JSON"));
      if body.is_some() {
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
      }
      stand
        .asked
        .lock()
        .unwrap()
        .push((method.to_string(), asked_at, authorization.clone(), body));
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
      ranges: ChatNumberRanges::default(),
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
    "numbering": {
      "rooms": "native", "people": "native", "messages": "native",
      "ranges": { "rooms": null, "people": null },
    },
    "speakers": false,
  });
  (declaration, wire)
}

fn text(words: &str) -> ChatSendRequest {
  ChatSendRequest::new(
    -100,
    ChatSend::Text {
      text: words.into(),
      silent: false,
      fold: false,
    },
  )
}

#[tokio::test]
async fn every_provider_route_is_called_with_its_method_token_and_body_and_its_answer_decodes() {
  let (declared, declaration_wire) = declaration();
  let ok = StatusCode::OK;
  let file = json!({ "id": "file-1", "name": "a.png", "mime": "image/png", "bytes": "iVA=" });
  let stand = Stand::answering([
    ("/provider/declaration", ok, declaration_wire),
    ("/provider/health", ok, json!({ "ready": true, "reason": null, "version": "1.0.0" })),
    ("/provider/sends", StatusCode::CREATED, json!({ "messageId": 77, "missed": [] })),
    (
      "/provider/rooms/-100",
      ok,
      json!({ "id": -100, "owns": true, "identity": 7, "kind": "owner", "privateTo": null, "title": "A Room" }),
    ),
    ("/provider/owner", ok, json!({ "roomId": -100 })),
    ("/provider/rooms/-100/messages/5/files", ok, json!({ "items": [file], "more": false })),
    (
      "/provider/dialect/readings",
      ok,
      json!({ "markup": "hello", "plain": "hello", "formatted": false, "parts": ["hello"], "fold": null }),
    ),
    ("/provider/dialect/checks", ok, json!({ "fault": null, "visible": "shut" })),
  ]);
  let listening = Listening::start(stand.router()).await;
  let waiter = Arc::new(Recording::default());
  let client =
    ChatProviderClient::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));
  let media = ChatMediaRequest {
    room_id: -100,
    message_id: 5,
    sent_at: Some(1_760_000_000_000),
  };

  assert_eq!(client.declaration().await.unwrap(), declared);
  assert_eq!(client.health().await.unwrap(), ChatHealth::ready("1.0.0"));
  assert_eq!(
    client.send(&text("hello")).await.unwrap(),
    ChatSent {
      message_id: Some(77),
      missed: vec![],
    }
  );
  assert_eq!(
    client.room(-100).await.unwrap(),
    ProviderRoom {
      id: -100,
      owns: true,
      identity: Some(7),
      kind: RoomKind::Owner,
      private_to: None,
      title: Some("A Room".into()),
    }
  );
  assert_eq!(
    client.owner().await.unwrap(),
    ProviderOwner {
      room_id: Some(-100)
    }
  );
  assert_eq!(
    client.message_files(&media).await.unwrap(),
    Page {
      items: vec![MessageFile {
        id: "file-1".into(),
        name: "a.png".into(),
        mime: "image/png".into(),
        bytes: vec![137, 80],
        kind: None,
      }],
      more: false,
    }
  );
  assert_eq!(
    client.dialect_reading("hello").await.unwrap(),
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
  drop(client);
  listening.close().await;

  let bearer = Some(format!("Bearer {TOKEN}"));
  let expected: Asked = [
    ("GET", "declaration", None),
    ("GET", "health", None),
    (
      "POST",
      "sends",
      Some(json!({ "kind": "text", "roomId": -100, "fields": { "text": "hello" } })),
    ),
    ("GET", "rooms/-100", None),
    ("GET", "owner", None),
    ("GET", "rooms/-100/messages/5/files?sentAt=1760000000000", None),
    ("POST", "dialect/readings", Some(json!({ "text": "hello" }))),
    ("POST", "dialect/checks", Some(json!({ "markup": "shut" }))),
  ]
  .into_iter()
  .map(|(method, route, body)| (method.to_owned(), format!("/provider/{route}"), bearer.clone(), body))
  .collect();
  assert_eq!(stand.asked(), expected);
  assert_eq!(waiter.waits(), [], "nothing was refused, so nothing waited");
}

#[tokio::test]
async fn refusals_come_back_in_the_error_shape_with_their_code_and_status() {
  let unsupported =
    WireError::new(ErrorCode::UnsupportedCapability, "Platform cannot fetch a picture or a file later.");
  let gone = WireError::new(ErrorCode::Gone, "Platform: Message 5 is no longer there.");
  let stand = Stand::answering([
    ("/provider/sends", StatusCode::GONE, serde_json::to_value(&gone).unwrap()),
    (
      "/provider/rooms/-100/messages/5/files",
      StatusCode::UNPROCESSABLE_ENTITY,
      serde_json::to_value(&unsupported).unwrap(),
    ),
  ]);
  let listening = Listening::start(stand.router()).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);

  let sent = client.send(&text("hello")).await.unwrap_err();
  let fetched = client
    .message_files(&ChatMediaRequest {
      room_id: -100,
      message_id: 5,
      sent_at: None,
    })
    .await
    .unwrap_err();
  drop(client);
  listening.close().await;

  assert_eq!(sent.code(), Some(ErrorCode::Gone));
  assert!(
    matches!(sent, ClientFailure::Answered { status: 410, answer: Some(answer), .. } if answer == gone)
  );
  assert_eq!(fetched.code(), Some(ErrorCode::UnsupportedCapability));
  assert!(
    matches!(fetched, ClientFailure::Answered { status: 422, answer: Some(answer), .. } if answer == unsupported)
  );
}

#[tokio::test]
async fn missed_pictures_decode_typed_and_an_unavailable_file_comes_back_unavailable() {
  let unavailable = WireError::new(ErrorCode::Unavailable, "The platform no longer has it.");
  let stand = Stand::answering([
    (
      "/provider/sends",
      StatusCode::CREATED,
      json!({
        "messageId": null,
        "missed": [{
          "address": "/elsewhere/a.png",
          "reason": { "code": "outside-data-root", "message": "whatever the provider rendered" },
        }],
      }),
    ),
    (
      "/provider/rooms/-100/messages/5/files",
      StatusCode::NOT_FOUND,
      serde_json::to_value(&unavailable).unwrap(),
    ),
  ]);
  let listening = Listening::start(stand.router()).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let sent = client.send(&text("an album went")).await.unwrap();
  let fetched = client
    .message_files(&ChatMediaRequest {
      room_id: -100,
      message_id: 5,
      sent_at: None,
    })
    .await
    .unwrap_err();
  drop(client);
  listening.close().await;

  assert_eq!(
    sent,
    ChatSent {
      message_id: None,
      missed: vec![MissedPicture {
        address: "/elsewhere/a.png".into(),
        reason: MissedPictureReason::OutsideDataRoot,
      }],
    }
  );
  assert_eq!(fetched.code(), Some(ErrorCode::Unavailable));
}

#[tokio::test]
async fn the_error_shape_is_decoded_and_never_offered_again() {
  let not_ready = WireError::new(ErrorCode::NotReady, "The platform connection is not open yet.");
  let stand = Stand::answering([(
    "/provider/owner",
    StatusCode::SERVICE_UNAVAILABLE,
    serde_json::to_value(&not_ready).unwrap(),
  )]);
  let listening = Listening::start(stand.router()).await;
  let waiter = Arc::new(Recording::default());
  let client =
    ChatProviderClient::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));
  let failure = client.owner().await.unwrap_err();
  let url = format!("http://{}/provider/owner", listening.address);
  drop(client);
  listening.close().await;

  assert_eq!(
    failure,
    ClientFailure::Answered {
      url: url.clone(),
      status: 503,
      answer: Some(not_ready),
    }
  );
  assert_eq!(
    failure.to_string(),
    format!("{url} answered 503: The platform connection is not open yet.")
  );
  assert_eq!(stand.asked().len(), 1);
  assert_eq!(waiter.waits(), []);
}

#[tokio::test]
async fn a_wrong_token_is_answered_unauthorized() {
  let stand = Stand::answering([(
    "/provider/health",
    StatusCode::OK,
    json!({ "ready": true, "reason": null, "version": "1.0.0" }),
  )]);
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
  let failure = client.room(-100).await.unwrap_err();
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
  let owner = |room_id: i64| json!({ "roomId": room_id });
  let first = Stand::answering([("/provider/owner", StatusCode::OK, owner(-1))]);
  let second = Stand::answering([("/provider/owner", StatusCode::OK, owner(-2))]);
  let first_listening = Listening::start(first.router()).await;
  let second_listening = Listening::start(second.router()).await;
  let to_first = ChatProviderClient::new(first_listening.base(), TOKEN);
  let to_second = ChatProviderClient::new(second_listening.base(), TOKEN);
  let owners = (to_first.owner().await.unwrap(), to_second.owner().await.unwrap());
  drop((to_first, to_second));
  first_listening.close().await;
  second_listening.close().await;

  assert_eq!(owners, (ProviderOwner { room_id: Some(-1) }, ProviderOwner { room_id: Some(-2) }));
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
        assert_eq!(url, format!("http://{address}/sends"));
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
    stand: Stand::answering([(
      "/provider/sends",
      StatusCode::CREATED,
      json!({ "messageId": 9, "missed": [] }),
    )]),
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
    ChatSent {
      message_id: Some(9),
      missed: vec![],
    }
  );
  assert_eq!(restarting.waits.load(Ordering::SeqCst), 2);
  let asked = restarting.stand.asked();
  assert_eq!(asked.len(), 1);
  assert_eq!(asked[0].1, "/provider/sends");
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

/// A provider's avatar routes: a page of one picture whatever the query, the picture of room -100,
/// `not-found` for person 7, and no route at all for anything else.
fn avatar_routes() -> Router {
  async fn list() -> Json<Value> {
    Json(json!({
      "items": [{ "kind": "person", "id": 7, "takenAt": 1_760_000_000_000_i64 }],
      "more": true,
    }))
  }
  async fn room(Path(id): Path<String>) -> Response {
    match id.as_str() {
      "-100" => ([("content-type", "image/png")], vec![137_u8, 80]).into_response(),
      _ => (StatusCode::NOT_FOUND, "nothing here").into_response(),
    }
  }
  async fn person(Path(id): Path<String>) -> Response {
    match id.as_str() {
      "7" => {
        let error = WireError::new(ErrorCode::NotFound, "No profile picture is kept for person 7.");
        (StatusCode::NOT_FOUND, Json(error)).into_response()
      }
      _ => (StatusCode::NOT_FOUND, "nothing here").into_response(),
    }
  }
  Router::new()
    .route("/provider/avatars", get(list))
    .route("/provider/rooms/{id}/avatar", get(room))
    .route("/provider/people/{id}/avatar", get(person))
}

#[tokio::test]
async fn a_kept_picture_comes_as_its_bytes_and_not_found_as_none() {
  let listening = Listening::start(avatar_routes()).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let listed = client.avatars(&AvatarsQuery::default()).await.unwrap();
  let kept = client.avatar(ChatAvatarKind::Room, -100).await.unwrap();
  let none = client.avatar(ChatAvatarKind::Person, 7).await.unwrap();
  let wrong = client.avatar(ChatAvatarKind::Person, 8).await.unwrap_err();
  drop(client);
  listening.close().await;

  let avatar = ChatAvatar {
    kind: ChatAvatarKind::Person,
    id: 7,
    taken_at: 1_760_000_000_000,
  };
  assert_eq!(
    listed,
    Page {
      items: vec![avatar],
      more: true
    }
  );
  assert_eq!(
    kept,
    Some(ChatPicture {
      mime: "image/png".into(),
      bytes: vec![137, 80],
    })
  );
  assert_eq!(none, None);
  assert!(
    matches!(
      wrong,
      ClientFailure::Answered {
        status: 404,
        answer: None,
        ..
      }
    ),
    "{wrong:?}"
  );
}

#[tokio::test]
async fn the_list_query_carries_only_what_was_given() {
  let asked = Arc::new(Mutex::new(Vec::new()));
  let kept = asked.clone();
  let router = Router::new().route(
    "/provider/avatars",
    get(move |RawQuery(query): RawQuery| {
      let kept = kept.clone();
      async move {
        kept.lock().unwrap().push(query);
        Json(json!({ "items": [], "more": false }))
      }
    }),
  );
  let listening = Listening::start(router).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let query = AvatarsQuery {
    after: Some(1_759_000_000_000),
    limit: Some(20),
  };
  client.avatars(&query).await.unwrap();
  client.avatars(&AvatarsQuery::default()).await.unwrap();
  client
    .avatars(&AvatarsQuery {
      after: None,
      limit: Some(5),
    })
    .await
    .unwrap();
  drop(client);
  listening.close().await;
  assert_eq!(
    *asked.lock().unwrap(),
    [Some("after=1759000000000&limit=20".to_owned()), None, Some("limit=5".to_owned())]
  );
}

#[tokio::test]
async fn the_rooms_of_the_account_come_as_a_page_and_their_refusal_with_its_code() {
  let asked = Arc::new(Mutex::new(Vec::new()));
  let kept = asked.clone();
  let router = Router::new().route(
    "/provider/rooms",
    get(move |RawQuery(query): RawQuery| {
      let kept = kept.clone();
      async move {
        kept.lock().unwrap().push(query.clone());
        if query.is_none() {
          let refused = WireError::new(
            ErrorCode::UnsupportedCapability,
            "Platform cannot list the rooms of its account.",
          );
          return (StatusCode::UNPROCESSABLE_ENTITY, Json(refused)).into_response();
        }
        let room = json!({
          "id": -100, "owns": true, "identity": 7, "kind": "group", "privateTo": null, "title": "A Room",
        });
        Json(json!({ "items": [room], "more": true })).into_response()
      }
    }),
  );
  let listening = Listening::start(router).await;
  let client = ChatProviderClient::new(listening.base(), TOKEN);
  let query = AccountRoomsQuery {
    after: Some(-200),
    limit: Some(1),
  };
  let listed = client.account_rooms(&query).await.unwrap();
  let refused = client.account_rooms(&AccountRoomsQuery::default()).await.unwrap_err();
  drop(client);
  listening.close().await;

  assert_eq!(
    listed,
    Page {
      items: vec![ProviderRoom {
        id: -100,
        owns: true,
        identity: Some(7),
        kind: RoomKind::Group,
        private_to: None,
        title: Some("A Room".into()),
      }],
      more: true,
    }
  );
  assert_eq!(refused.code(), Some(ErrorCode::UnsupportedCapability), "{refused:?}");
  assert_eq!(*asked.lock().unwrap(), [Some("after=-200&limit=1".to_owned()), None]);
}

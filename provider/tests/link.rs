//! The receiver link against a receiver listening on the loopback: every hand-over route, its token and
//! body, and offering a request again only while the connection is refused. No test waits on the real
//! clock, and every listener a test starts is taken down before it ends.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::post;
use bmo_chat::{
  ChatFailure, ChatGathered, ChatHandover, ChatMessage, ChatPatience, ChatReceiver, ChatReceiverLink,
  ChatVoice, ChatWait, ErrorCode, GatheredOutcome, InboundChatContext, LinkFailure, MessageFile, WireError,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const TOKEN: &str = "the-receiver-token";

/// What the receiver was handed: the route, the authorization header and the body.
type Handed = Arc<Mutex<Vec<(String, Option<String>, Value)>>>;

/// A receiver that takes every hand-over under `/receiver` and keeps it.
fn receiver(handed: Handed) -> Router {
  async fn take(
    State(handed): State<Handed>,
    Path(route): Path<String>,
    headers: HeaderMap,
    body: Bytes,
  ) -> Json<Value> {
    let authorization = headers.get("authorization").map(|value| value.to_str().unwrap().to_owned());
    let body = serde_json::from_slice(&body).expect("the link posts JSON");
    handed.lock().unwrap().push((route, authorization, body));
    Json(json!({}))
  }
  Router::new().nest("/receiver", Router::new().route("/{route}", post(take)).with_state(handed))
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
    format!("http://{}/receiver/", self.address)
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

fn handover(text: &str) -> ChatHandover {
  ChatHandover {
    message: ChatMessage {
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
    },
    context: Some(InboundChatContext {
      carries_file: true,
      gathering: true,
      ..InboundChatContext::default()
    }),
    room: None,
    files: vec![],
  }
}

fn link_failure(failure: ChatFailure) -> LinkFailure {
  failure
    .downcast_ref::<LinkFailure>()
    .expect("the link fails with a link failure")
    .clone()
}

#[tokio::test]
async fn every_hand_over_reaches_its_route_with_the_token_and_its_body() {
  let handed = Handed::default();
  let listening = Listening::start(receiver(handed.clone())).await;
  let waiter = Arc::new(Recording::default());
  let link = ChatReceiverLink::new(listening.base(), TOKEN).with_patience(patience(waiter.clone(), 6, 2));

  let kept = ChatGathered {
    chat_id: -100,
    message_id: 5,
    via: "stand-in".into(),
    outcome: GatheredOutcome::Kept {
      files: vec![MessageFile {
        id: "file-1".into(),
        name: "a.png".into(),
        mime: "image/png".into(),
        bytes: vec![137, 80],
      }],
    },
  };
  let not_kept = ChatGathered {
    outcome: GatheredOutcome::NotKept {
      reason: "The file was too large to keep.".into(),
    },
    ..kept.clone()
  };
  link.message(handover("said")).await.expect("the message is taken");
  link.attachment(handover("carried")).await.expect("the attachment is taken");
  link.edited(handover("said again")).await.expect("the edit is taken");
  link.gathered(kept.clone()).await.expect("the kept file is taken");
  link.gathered(not_kept.clone()).await.expect("the not-kept outcome is taken");
  drop(link);
  listening.close().await;

  let bearer = Some(format!("Bearer {TOKEN}"));
  let expected = [
    ("message", serde_json::to_value(handover("said")).unwrap()),
    ("attachment", serde_json::to_value(handover("carried")).unwrap()),
    ("edited", serde_json::to_value(handover("said again")).unwrap()),
    ("gathered", serde_json::to_value(&kept).unwrap()),
    ("gathered", serde_json::to_value(&not_kept).unwrap()),
  ]
  .map(|(route, body)| (route.to_owned(), bearer.clone(), body));
  assert_eq!(*handed.lock().unwrap(), expected);
  assert_eq!(handed.lock().unwrap()[3].2["files"][0]["bytes"], "iVA=");
  assert_eq!(handed.lock().unwrap()[4].2["outcome"], "not-kept");
  assert_eq!(waiter.waits(), [], "nothing was refused, so nothing waited");
}

#[tokio::test]
async fn an_answer_that_is_not_success_is_never_offered_again() {
  let offers = Arc::new(AtomicUsize::new(0));
  let counted = offers.clone();
  let refusing = Router::new().route(
    "/receiver/message",
    post(move || {
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
  let failure = link_failure(link.message(handover("said")).await.unwrap_err());
  let url = format!("http://{}/receiver/message", listening.address);
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
async fn an_unknown_route_answer_is_kept_as_it_came() {
  let listening = Listening::start(Router::new()).await;
  let link = ChatReceiverLink::new(listening.base(), TOKEN);
  let failure = link_failure(link.edited(handover("said again")).await.unwrap_err());
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
    let failure = link_failure(link.message(handover("said")).await.unwrap_err());
    let waits: Vec<Duration> = waits.into_iter().map(Duration::from_secs).collect();
    match failure {
      LinkFailure::Unreachable { url, waited, why } => {
        assert_eq!(url, format!("http://{address}/message"));
        assert_eq!(waited, waits.iter().sum::<Duration>());
        assert!(why.to_lowercase().contains("refused"), "{why}");
      }
      failure => panic!("a refused connection is unreachable, not {failure:?}"),
    }
    assert_eq!(waiter.waits(), waits, "a window of {window}s every {every}s");
  }
}

/// A waiter that, on its second wait, starts the receiver at the address that was refusing.
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
      let listening = Listening::on(listener, receiver(self.handed.clone())).await;
      *self.started.lock().unwrap() = Some(listening);
    }
  }
}

#[tokio::test]
async fn a_receiver_that_comes_back_within_the_window_is_handed_the_request_once() {
  let address = closed_address().await;
  let restarting = Arc::new(Restarting {
    address,
    handed: Handed::default(),
    waits: AtomicUsize::new(0),
    started: Mutex::new(None),
  });
  let link = ChatReceiverLink::new(format!("http://{address}/receiver"), TOKEN).with_patience(patience(
    restarting.clone(),
    120,
    2,
  ));
  let taken = link.message(handover("said")).await;
  drop(link);
  let listening = restarting.started.lock().unwrap().take().expect("the receiver was started");
  listening.close().await;

  taken.expect("the message is taken once the receiver is back");
  assert_eq!(restarting.waits.load(Ordering::SeqCst), 2);
  let handed = restarting.handed.lock().unwrap();
  assert_eq!(handed.len(), 1);
  assert_eq!(handed[0].0, "message");
  assert_eq!(handed[0].2, serde_json::to_value(handover("said")).unwrap());
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
  let failure = link_failure(link.attachment(handover("carried")).await.unwrap_err());
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

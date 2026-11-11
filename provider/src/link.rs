//! The link to the receiving side: how a provider hands what a room said to the receiving side, and asks it
//! what it needs to know, over the receiving-side routes of `docs/protocol.md`.

use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, Method};
use serde::Serialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde_json::Value;

use crate::bodies::Page;
use crate::error::{ErrorCode, WireError};
use crate::message::{GatheredPatch, HandedMessage};
use crate::provider::{ChatFailure, ChatReceiver};
use crate::receiver_bodies::{
  AvatarChange, Deletion, HeldMessage, ReactionChange, ReceiverRoom, RoomQuery, RoomsQuery,
};

/// How a wait between two offers of a request is spent. The link counts the time it has waited by the
/// intervals it asked for, never by reading a clock, so a waiter that returns at once makes a whole window
/// pass at once.
#[async_trait]
pub trait ChatWait: Send + Sync {
  async fn wait(&self, interval: Duration);
}

/// Waits on the real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealClock;

#[async_trait]
impl ChatWait for RealClock {
  async fn wait(&self, interval: Duration) {
    tokio::time::sleep(interval).await;
  }
}

/// How long a request whose connection was refused keeps being offered, and how often.
#[derive(Clone)]
pub struct ChatPatience {
  /// The most time spent waiting before the request is given up.
  pub window: Duration,
  /// The wait before each new offer; zero offers a request only once.
  pub every: Duration,
  pub waiter: Arc<dyn ChatWait>,
}

impl Default for ChatPatience {
  /// Two minutes, offered every two seconds, on the real clock.
  fn default() -> Self {
    Self {
      window: Duration::from_secs(120),
      every: Duration::from_secs(2),
      waiter: Arc::new(RealClock),
    }
  }
}

impl fmt::Debug for ChatPatience {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ChatPatience")
      .field("window", &self.window)
      .field("every", &self.every)
      .finish_non_exhaustive()
  }
}

/// Why a call did not reach the receiving side, or was not taken or answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkFailure {
  /// Nothing listened at the address for the whole patience window.
  Unreachable {
    url: String,
    waited: Duration,
    why: String,
  },
  /// The receiving side answered, and not with success. Such a request is never offered again.
  Answered {
    url: String,
    status: u16,
    /// The error shape, where the answer carried one.
    answer: Option<WireError>,
  },
  /// The receiving side answered with success, in a body that is not the route's answer.
  Undecodable { url: String, why: String },
  /// The request failed after it may have reached a listener, so it is not offered again.
  Transport { url: String, why: String },
}

impl fmt::Display for LinkFailure {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Unreachable { url, waited, why } => {
        write!(f, "Nothing was listening at {url} after waiting {waited:?}: {why}")
      }
      Self::Answered {
        url,
        status,
        answer: Some(answer),
      } => write!(f, "{url} answered {status}: {}", answer.error),
      Self::Answered {
        url,
        status,
        answer: None,
      } => write!(f, "{url} answered {status}."),
      Self::Undecodable { url, why } => {
        write!(f, "{url} answered in a shape the route does not have: {why}")
      }
      Self::Transport { url, why } => write!(f, "The request to {url} failed: {why}"),
    }
  }
}

impl std::error::Error for LinkFailure {}

/// The receiving side as a provider reaches it: the receiving-side routes at a base address, behind its
/// token. The provider's own configuration gives both.
#[derive(Debug, Clone)]
pub struct ChatReceiverLink {
  base: String,
  token: String,
  patience: ChatPatience,
  client: Client,
}

impl ChatReceiverLink {
  /// A link to the receiving side at `base`, with the default patience. It reads no environment of its own,
  /// so no proxy is taken from it.
  pub fn new(base: impl Into<String>, token: impl Into<String>) -> Self {
    let client = Client::builder()
      .no_proxy()
      .build()
      .expect("a client with no TLS and no proxy always builds");
    Self {
      base: base.into().trim_end_matches('/').to_owned(),
      token: token.into(),
      patience: ChatPatience::default(),
      client,
    }
  }

  pub fn with_patience(mut self, patience: ChatPatience) -> Self {
    self.patience = patience;
    self
  }

  /// Makes the call behind the receiving side's token, offering it again only while the connection is
  /// refused and the patience window lasts, and gives a successful answer's status and bytes.
  async fn exchange(&self, method: Method, route: &str, call: Call) -> Result<Exchanged, LinkFailure> {
    let url = format!("{}/{route}", self.base);
    let body = call.body.map(|body| body.to_string());
    let ChatPatience {
      window,
      every,
      waiter,
    } = &self.patience;
    let mut waited = Duration::ZERO;
    loop {
      let mut request = self.client.request(method.clone(), &url).bearer_auth(&self.token);
      if let Some(query) = &call.query {
        request = request.query(query);
      }
      if let Some(body) = &body {
        request = request.header(CONTENT_TYPE, "application/json").body(body.clone());
      }
      let error = match request.send().await {
        Ok(answer) => {
          let status = answer.status();
          let bytes = answer.bytes().await.map_err(|error| LinkFailure::Transport {
            url: url.clone(),
            why: chain(&error),
          })?;
          if !status.is_success() {
            return Err(LinkFailure::Answered {
              url,
              status: status.as_u16(),
              answer: serde_json::from_slice(&bytes).ok(),
            });
          }
          return Ok(Exchanged {
            url,
            status: status.as_u16(),
            bytes: bytes.to_vec(),
          });
        }
        Err(error) => error,
      };
      if !refused(&error) {
        return Err(LinkFailure::Transport {
          url,
          why: chain(&error),
        });
      }
      if every.is_zero() || waited + *every > *window {
        return Err(LinkFailure::Unreachable {
          url,
          waited,
          why: chain(&error),
        });
      }
      waiter.wait(*every).await;
      waited += *every;
    }
  }

  /// Makes the call and decodes a successful answer as the route's.
  async fn call<T: DeserializeOwned>(
    &self,
    method: Method,
    route: &str,
    call: Call,
  ) -> Result<(u16, T), LinkFailure> {
    let Exchanged { url, status, bytes } = self.exchange(method, route, call).await?;
    let answer = serde_json::from_slice(&bytes).map_err(|error| LinkFailure::Undecodable {
      url,
      why: error.to_string(),
    })?;
    Ok((status, answer))
  }
}

/// What a call carries beside its method and route, each as the JSON it is written as.
#[derive(Default)]
struct Call {
  query: Option<Value>,
  body: Option<Value>,
}

impl Call {
  fn query(query: &impl Serialize) -> Self {
    Self {
      query: Some(serde_json::to_value(query).expect("a wire query always renders as JSON")),
      body: None,
    }
  }

  fn body(body: &impl Serialize) -> Self {
    Self {
      query: None,
      body: Some(serde_json::to_value(body).expect("a wire body always renders as JSON")),
    }
  }
}

/// A successful answer: the address asked, its status and its bytes.
struct Exchanged {
  url: String,
  status: u16,
  bytes: Vec<u8>,
}

#[async_trait]
impl ChatReceiver for ChatReceiverLink {
  async fn hand_over(
    &self,
    room_id: i64,
    message_id: i64,
    message: HandedMessage,
  ) -> Result<HeldMessage, ChatFailure> {
    let route = format!("rooms/{room_id}/messages/{message_id}");
    let call = Call::body(&message);
    let (status, message) = self.call(Method::PUT, &route, call).await?;
    Ok(HeldMessage {
      created: status == 201,
      message,
    })
  }

  async fn gathered(
    &self,
    room_id: i64,
    message_id: i64,
    patch: GatheredPatch,
  ) -> Result<HandedMessage, ChatFailure> {
    let route = format!("rooms/{room_id}/messages/{message_id}");
    let call = Call::body(&patch);
    let (_, message) = self.call(Method::PATCH, &route, call).await?;
    Ok(message)
  }

  async fn avatars_changed(&self, change: AvatarChange) -> Result<(), ChatFailure> {
    let call = Call::body(&change);
    let (_, IgnoredAny) = self.call(Method::POST, "avatar-changes", call).await?;
    Ok(())
  }

  async fn reactions_changed(&self, change: ReactionChange) -> Result<(), ChatFailure> {
    let call = Call::body(&change);
    let (_, IgnoredAny) = self.call(Method::POST, "reaction-changes", call).await?;
    Ok(())
  }

  async fn deleted(&self, deletion: Deletion) -> Result<(), ChatFailure> {
    let call = Call::body(&deletion);
    let (_, IgnoredAny) = self.call(Method::POST, "deletions", call).await?;
    Ok(())
  }

  async fn room(&self, room_id: i64, query: RoomQuery) -> Result<Option<ReceiverRoom>, ChatFailure> {
    let call = Call::query(&query);
    match self.call(Method::GET, &format!("rooms/{room_id}"), call).await {
      Ok((_, room)) => Ok(Some(room)),
      Err(LinkFailure::Answered {
        answer: Some(WireError {
          code: ErrorCode::NotFound,
          ..
        }),
        ..
      }) => Ok(None),
      Err(failure) => Err(failure.into()),
    }
  }

  async fn rooms(&self, query: RoomsQuery) -> Result<Page<ReceiverRoom>, ChatFailure> {
    let call = Call::query(&query);
    let (_, page) = self.call(Method::GET, "rooms", call).await?;
    Ok(page)
  }
}

/// Whether the connection was refused, so nothing listened and the request cannot have been received.
fn refused(error: &reqwest::Error) -> bool {
  let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
  while let Some(error) = cause {
    if error
      .downcast_ref::<io::Error>()
      .is_some_and(|error| error.kind() == io::ErrorKind::ConnectionRefused)
    {
      return true;
    }
    cause = error.source();
  }
  false
}

/// The error and every cause beneath it, in one line.
fn chain(error: &reqwest::Error) -> String {
  let mut words = error.to_string();
  let mut cause = std::error::Error::source(error);
  while let Some(error) = cause {
    words.push_str(": ");
    words.push_str(&error.to_string());
    cause = error.source();
  }
  words
}

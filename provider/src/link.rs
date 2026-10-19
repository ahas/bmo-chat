//! The receiver link: how a provider hands what a room said to the receiver, and asks it what it needs to
//! know, over the receiver routes of `docs/protocol.md`.

use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use reqwest::header::CONTENT_TYPE;
use serde::Serialize;
use serde::de::{DeserializeOwned, IgnoredAny};

use crate::error::WireError;
use crate::message::{ChatGathered, ChatHandover};
use crate::provider::{ChatFailure, ChatReceiver};
use crate::receiver_bodies::{
  AvatarsUpdated, HistoryKept, HistoryRoom, Listened, ListenedQuery, RoomsOf, RoomsOfQuery,
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

/// Why a call did not reach the receiver, or was not taken or answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkFailure {
  /// Nothing listened at the address for the whole patience window.
  Unreachable {
    url: String,
    waited: Duration,
    why: String,
  },
  /// The receiver answered, and not with success. Such a request is never offered again.
  Answered {
    url: String,
    status: u16,
    /// The error shape, where the answer carried one.
    answer: Option<WireError>,
  },
  /// The receiver answered with success, in a body that is not the route's answer.
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

/// The receiver as a provider reaches it: the receiver routes at a base address, behind the receiver's
/// token. The provider's own configuration gives both.
#[derive(Debug, Clone)]
pub struct ChatReceiverLink {
  base: String,
  token: String,
  patience: ChatPatience,
  client: Client,
}

impl ChatReceiverLink {
  /// A link to the receiver at `base`, with the default patience. It reads no environment of its own, so no
  /// proxy is taken from it.
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

  /// Posts `body` to the route, offering it again only while the connection is refused and the patience
  /// window lasts, and decodes a successful answer as the route's.
  async fn post<T: DeserializeOwned>(&self, route: &str, body: &impl Serialize) -> Result<T, LinkFailure> {
    let url = format!("{}/{route}", self.base);
    let body = serde_json::to_vec(body).map_err(|error| LinkFailure::Transport {
      url: url.clone(),
      why: error.to_string(),
    })?;
    let ChatPatience {
      window,
      every,
      waiter,
    } = &self.patience;
    let mut waited = Duration::ZERO;
    loop {
      let offered = self
        .client
        .post(&url)
        .bearer_auth(&self.token)
        .header(CONTENT_TYPE, "application/json")
        .body(body.clone())
        .send()
        .await;
      let error = match offered {
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
          return serde_json::from_slice(&bytes).map_err(|error| LinkFailure::Undecodable {
            url,
            why: error.to_string(),
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

  /// Posts a hand-over, whose answer says nothing beyond that it was taken.
  async fn hand_over(&self, route: &str, body: &impl Serialize) -> Result<(), ChatFailure> {
    let IgnoredAny = self.post(route, body).await?;
    Ok(())
  }
}

#[async_trait]
impl ChatReceiver for ChatReceiverLink {
  async fn message(&self, handover: ChatHandover) -> Result<(), ChatFailure> {
    self.hand_over("message", &handover).await
  }

  async fn attachment(&self, handover: ChatHandover) -> Result<(), ChatFailure> {
    self.hand_over("attachment", &handover).await
  }

  async fn edited(&self, handover: ChatHandover) -> Result<(), ChatFailure> {
    self.hand_over("edited", &handover).await
  }

  async fn gathered(&self, gathered: ChatGathered) -> Result<(), ChatFailure> {
    self.hand_over("gathered", &gathered).await
  }

  async fn history(&self, handover: ChatHandover) -> Result<bool, ChatFailure> {
    let answer: HistoryKept = self.post("history", &handover).await?;
    Ok(answer.kept)
  }

  async fn avatars_updated(&self, updated: AvatarsUpdated) -> Result<(), ChatFailure> {
    self.hand_over("avatars/updated", &updated).await
  }

  async fn listened(&self, query: ListenedQuery) -> Result<bool, ChatFailure> {
    let answer: Listened = self.post("rooms/listened", &query).await?;
    Ok(answer.listened)
  }

  async fn rooms_of(&self, query: RoomsOfQuery) -> Result<Vec<HistoryRoom>, ChatFailure> {
    let answer: RoomsOf = self.post("rooms/of", &query).await?;
    Ok(answer.rooms)
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

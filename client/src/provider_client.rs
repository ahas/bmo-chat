//! The provider client: how the receiver reaches one provider over the provider routes of
//! `docs/protocol.md`.

use std::fmt;
use std::io;
use std::time::Duration;

use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, RequestBuilder};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::avatar::{AvatarList, AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatPicture};
use crate::bodies::{
  ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, OwnerRoom, RoomIdentity,
  RoomKindAnswer, RoomOwns, RoomPerson, RoomQuery, RoomTitle,
};
use crate::declaration::ChatDeclaration;
use crate::error::{ErrorCode, WireError};
use crate::media::{ChatMediaRequest, ChatMediaResult};
use crate::message::RoomKind;
use crate::patience::ChatPatience;
use crate::send::{ChatSendRequest, ChatSendResult};

/// Why a call to a provider came to no answer of the route's shape. A refusal is an answer, so it is never
/// one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFailure {
  /// Nothing listened at the address for the whole patience window.
  Unreachable {
    url: String,
    waited: Duration,
    why: String,
  },
  /// The provider answered, and not with success. Such a request is never offered again.
  Answered {
    url: String,
    status: u16,
    /// The error shape, where the answer carried one.
    answer: Option<WireError>,
  },
  /// The provider answered with success, in a body that is not the route's answer.
  Undecodable { url: String, why: String },
  /// The request failed after it may have reached a listener, so it is not offered again.
  Transport { url: String, why: String },
}

impl fmt::Display for ClientFailure {
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

impl std::error::Error for ClientFailure {}

/// One provider as the receiver reaches it: the provider routes at a base address, behind the provider's
/// token. The receiver's own deployment configuration gives both, and the receiver holds one client for
/// each provider it lists.
#[derive(Debug, Clone)]
pub struct ChatProviderClient {
  base: String,
  token: String,
  patience: ChatPatience,
  client: Client,
}

impl ChatProviderClient {
  /// A client for the provider at `base`, with the default patience. It reads no environment of its own, so
  /// no proxy is taken from it.
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

  /// `GET /declaration`.
  pub async fn declaration(&self) -> Result<ChatDeclaration, ClientFailure> {
    self.get("declaration").await
  }

  /// `GET /health`.
  pub async fn health(&self) -> Result<ChatHealth, ClientFailure> {
    self.get("health").await
  }

  /// `POST /send`. A send the provider does not perform comes back as `ChatSendResult::Refused`.
  pub async fn send(&self, request: &ChatSendRequest) -> Result<ChatSendResult, ClientFailure> {
    self.post("send", request).await
  }

  /// `POST /media`.
  pub async fn media(&self, request: &ChatMediaRequest) -> Result<ChatMediaResult, ClientFailure> {
    self.post("media", request).await
  }

  /// `POST /dialect/read`.
  pub async fn dialect_read(&self, text: impl Into<String>) -> Result<DialectReading, ClientFailure> {
    self.post("dialect/read", &DialectRead { text: text.into() }).await
  }

  /// `POST /dialect/check`.
  pub async fn dialect_check(&self, markup: impl Into<String>) -> Result<DialectChecked, ClientFailure> {
    self
      .post(
        "dialect/check",
        &DialectCheck {
          markup: markup.into(),
        },
      )
      .await
  }

  /// `POST /room/owns`.
  pub async fn room_owns(&self, chat_id: i64) -> Result<bool, ClientFailure> {
    let answer: RoomOwns = self.post("room/owns", &RoomQuery { chat_id }).await?;
    Ok(answer.owns)
  }

  /// `POST /room/identity`.
  pub async fn room_identity(&self, chat_id: i64) -> Result<Option<i64>, ClientFailure> {
    let answer: RoomIdentity = self.post("room/identity", &RoomQuery { chat_id }).await?;
    Ok(answer.identity)
  }

  /// `POST /room/is-owner`.
  pub async fn room_kind(&self, chat_id: i64) -> Result<RoomKind, ClientFailure> {
    let answer: RoomKindAnswer = self.post("room/is-owner", &RoomQuery { chat_id }).await?;
    Ok(answer.kind)
  }

  /// `POST /room/private-of`.
  pub async fn room_private_of(&self, chat_id: i64) -> Result<Option<i64>, ClientFailure> {
    let answer: RoomPerson = self.post("room/private-of", &RoomQuery { chat_id }).await?;
    Ok(answer.person)
  }

  /// `POST /room/title`.
  pub async fn room_title(&self, chat_id: i64) -> Result<Option<String>, ClientFailure> {
    let answer: RoomTitle = self.post("room/title", &RoomQuery { chat_id }).await?;
    Ok(answer.title)
  }

  /// `GET /owner-room`.
  pub async fn owner_room(&self) -> Result<Option<i64>, ClientFailure> {
    let answer: OwnerRoom = self.get("owner-room").await?;
    Ok(answer.chat_id)
  }

  /// `GET /avatars`: the profile pictures the provider keeps, oldest taken first.
  pub async fn avatars(&self, query: &AvatarsQuery) -> Result<Vec<ChatAvatar>, ClientFailure> {
    let url = self.url("avatars");
    let answer: AvatarList = self.call(&url, self.client.get(&url).query(query)).await?;
    Ok(answer.avatars)
  }

  /// `GET /avatars/{kind}/{id}`: the picture the provider keeps for a room or a person, or null where it
  /// answers `nothing-held`.
  pub async fn avatar(&self, kind: ChatAvatarKind, id: i64) -> Result<Option<ChatPicture>, ClientFailure> {
    let url = self.url(&format!("avatars/{kind}/{id}"));
    match self.exchange(&url, self.client.get(&url)).await {
      Ok(Exchanged { mime, bytes }) => Ok(Some(ChatPicture {
        mime: mime.unwrap_or_else(|| "application/octet-stream".into()),
        bytes,
      })),
      Err(ClientFailure::Answered {
        answer: Some(WireError {
          code: ErrorCode::NothingHeld,
          ..
        }),
        ..
      }) => Ok(None),
      Err(failure) => Err(failure),
    }
  }

  async fn get<T: DeserializeOwned>(&self, route: &str) -> Result<T, ClientFailure> {
    let url = self.url(route);
    self.call(&url, self.client.get(&url)).await
  }

  async fn post<T: DeserializeOwned>(
    &self,
    route: &str,
    body: &impl Serialize,
  ) -> Result<T, ClientFailure> {
    let url = self.url(route);
    let body = serde_json::to_vec(body).map_err(|error| ClientFailure::Transport {
      url: url.clone(),
      why: error.to_string(),
    })?;
    let request = self.client.post(&url).header(CONTENT_TYPE, "application/json").body(body);
    self.call(&url, request).await
  }

  fn url(&self, route: &str) -> String {
    format!("{}/{route}", self.base)
  }

  /// Makes the call and decodes a successful answer as the route's.
  async fn call<T: DeserializeOwned>(
    &self,
    url: &str,
    request: RequestBuilder,
  ) -> Result<T, ClientFailure> {
    let Exchanged { bytes, .. } = self.exchange(url, request).await?;
    serde_json::from_slice(&bytes).map_err(|error| ClientFailure::Undecodable {
      url: url.to_owned(),
      why: error.to_string(),
    })
  }

  /// Makes the call behind the provider's token, offering it again only while the connection is refused and
  /// the patience window lasts, and gives a successful answer's type and bytes.
  async fn exchange(&self, url: &str, request: RequestBuilder) -> Result<Exchanged, ClientFailure> {
    let url = url.to_owned();
    let request = request.bearer_auth(&self.token);
    let ChatPatience {
      window,
      every,
      waiter,
    } = &self.patience;
    let mut waited = Duration::ZERO;
    loop {
      let offered = request.try_clone().expect("a request whose body is bytes always clones");
      let error = match offered.send().await {
        Ok(answer) => {
          let status = answer.status();
          let mime = answer
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
          let bytes = answer.bytes().await.map_err(|error| ClientFailure::Transport {
            url: url.clone(),
            why: chain(&error),
          })?;
          if !status.is_success() {
            return Err(ClientFailure::Answered {
              url,
              status: status.as_u16(),
              answer: serde_json::from_slice(&bytes).ok(),
            });
          }
          return Ok(Exchanged {
            mime,
            bytes: bytes.to_vec(),
          });
        }
        Err(error) => error,
      };
      if !refused(&error) {
        return Err(ClientFailure::Transport {
          url,
          why: chain(&error),
        });
      }
      if every.is_zero() || waited + *every > *window {
        return Err(ClientFailure::Unreachable {
          url,
          waited,
          why: chain(&error),
        });
      }
      waiter.wait(*every).await;
      waited += *every;
    }
  }
}

/// A successful answer: its `Content-Type`, where it gave one, and its bytes.
struct Exchanged {
  mime: Option<String>,
  bytes: Vec<u8>,
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

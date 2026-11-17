//! The provider client: how the receiving side, or a harness, reaches one provider over the provider routes
//! of `docs/protocol.md`.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use reqwest::header::CONTENT_TYPE;
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::avatar::{AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatPicture};
use crate::bodies::{
  AccountRoomsQuery, ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, Page,
  ProviderOwner, ProviderRoom,
};
use crate::contacts::{ChatContact, ChatStartRequest, ContactsQuery};
use crate::declaration::ChatDeclaration;
use crate::error::{ErrorCode, WireError};
use crate::file_body::read_with_parts;
use crate::media::{ChatMediaRequest, FilesQuery};
use crate::members::ChatMembers;
use crate::message::MessageFile;
use crate::parts::{Outgoing, Unread, multipart_boundary, spooled};
use crate::patience::ChatPatience;
use crate::send::{ChatSendRequest, ChatSent};

/// Why a call to a provider came to no answer of the route's shape. A refusal is answered in the error
/// shape, so it is `Answered` with the refusal's code.
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

impl ClientFailure {
  /// The code of the error shape the provider answered with, where it answered with one.
  pub fn code(&self) -> Option<ErrorCode> {
    match self {
      Self::Answered {
        answer: Some(answer),
        ..
      } => Some(answer.code),
      _ => None,
    }
  }
}

/// One provider as a caller reaches it: the provider routes at a base address, behind the provider's token.
/// The caller's own configuration gives both, and a receiving side holds one client for each provider it
/// lists. A file an answer carries as a part is written into the client's spool as it arrives.
#[derive(Debug, Clone)]
pub struct ChatProviderClient {
  base: String,
  token: String,
  patience: ChatPatience,
  spool: PathBuf,
  client: Client,
}

impl ChatProviderClient {
  /// A client for the provider at `base`, with the default patience and the system's temporary directory for
  /// its spool. It reads no environment of its own, so no proxy is taken from it.
  pub fn new(base: impl Into<String>, token: impl Into<String>) -> Self {
    let client = Client::builder()
      .no_proxy()
      .build()
      .expect("a client with no TLS and no proxy always builds");
    Self {
      base: base.into().trim_end_matches('/').to_owned(),
      token: token.into(),
      patience: ChatPatience::default(),
      spool: std::env::temp_dir(),
      client,
    }
  }

  pub fn with_patience(mut self, patience: ChatPatience) -> Self {
    self.patience = patience;
    self
  }

  /// The directory a file an answer carries as a part is written into as it arrives (`docs/contract.md`,
  /// "A file's bytes").
  pub fn with_spool(mut self, spool: impl Into<PathBuf>) -> Self {
    self.spool = spool.into();
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

  /// `POST /sends`. A send that carries a file stored on disk goes as a multipart body, the file read from
  /// the disk as it is sent. A send the provider does not perform comes back as `ClientFailure::Answered` with
  /// the refusal's code.
  pub async fn send(&self, request: &ChatSendRequest) -> Result<ChatSent, ClientFailure> {
    self.post("sends", request).await
  }

  /// `GET /rooms`: a page of the rooms the provider's account is in, in ascending order of room number. A
  /// provider that does not declare `roomList` comes back as `ClientFailure::Answered` with
  /// `unsupported-capability`.
  pub async fn account_rooms(
    &self,
    query: &AccountRoomsQuery,
  ) -> Result<Page<ProviderRoom>, ClientFailure> {
    self.get_with("rooms", query).await
  }

  /// `POST /rooms`: a conversation started on the platform, answered with its room. A shape the provider does
  /// not declare comes back as `ClientFailure::Answered` with `unsupported-capability`, a contact it does not
  /// know with `not-found`, and a start of no sound shape with `bad-request`.
  pub async fn start_chat(&self, start: &ChatStartRequest) -> Result<ProviderRoom, ClientFailure> {
    self.post("rooms", start).await
  }

  /// `GET /contacts`: a page of the contacts of the account the query names, or of the provider's own, in
  /// ascending order of contact number.
  /// A provider that does not declare `contactList` comes back as `ClientFailure::Answered` with
  /// `unsupported-capability`.
  pub async fn contacts(&self, query: &ContactsQuery) -> Result<Page<ChatContact>, ClientFailure> {
    self.get_with("contacts", query).await
  }

  /// `GET /rooms/{roomId}`. A room the provider knows nothing of comes back as `ClientFailure::Answered` with
  /// `not-found`.
  pub async fn room(&self, room_id: i64) -> Result<ProviderRoom, ClientFailure> {
    self.get(&format!("rooms/{room_id}")).await
  }

  /// `GET /rooms/{roomId}/members`: the room's members, whole. A provider that does not declare `memberList`
  /// comes back as `ClientFailure::Answered` with `unsupported-capability`, and a room whose members it does not
  /// hold with `not-found` and its reason.
  pub async fn members(&self, room_id: i64) -> Result<ChatMembers, ClientFailure> {
    self.get(&format!("rooms/{room_id}/members")).await
  }

  /// `GET /owner`.
  pub async fn owner(&self) -> Result<ProviderOwner, ClientFailure> {
    self.get("owner").await
  }

  /// `GET /rooms/{roomId}/messages/{messageId}/files`. A file the provider answers as a part comes back stored
  /// in the client's spool, and one it answers inline in memory. Files the platform would not give come back
  /// as `ClientFailure::Answered` with `unavailable`, and a refusal with the refusal's code.
  pub async fn message_files(
    &self,
    request: &ChatMediaRequest,
  ) -> Result<Page<MessageFile>, ClientFailure> {
    let ChatMediaRequest {
      room_id,
      message_id,
      sent_at,
    } = *request;
    let route = format!("rooms/{room_id}/messages/{message_id}/files");
    self.get_with(&route, &FilesQuery { sent_at }).await
  }

  /// `POST /dialect/readings`.
  pub async fn dialect_reading(&self, text: impl Into<String>) -> Result<DialectReading, ClientFailure> {
    self.post("dialect/readings", &DialectRead { text: text.into() }).await
  }

  /// `POST /dialect/checks`.
  pub async fn dialect_check(&self, markup: impl Into<String>) -> Result<DialectChecked, ClientFailure> {
    let markup = markup.into();
    self.post("dialect/checks", &DialectCheck { markup }).await
  }

  /// `GET /avatars`: a page of the profile pictures the provider keeps, oldest taken first.
  pub async fn avatars(&self, query: &AvatarsQuery) -> Result<Page<ChatAvatar>, ClientFailure> {
    self.get_with("avatars", query).await
  }

  /// `GET /rooms/{roomId}/avatar` or `GET /people/{personId}/avatar`: the picture the provider keeps for a
  /// room or a person, or null where it answers `not-found`.
  pub async fn avatar(&self, kind: ChatAvatarKind, id: i64) -> Result<Option<ChatPicture>, ClientFailure> {
    let url = match kind {
      ChatAvatarKind::Room => self.url(&format!("rooms/{id}/avatar")),
      ChatAvatarKind::Person => self.url(&format!("people/{id}/avatar")),
    };
    match self.exchange(&url, self.client.get(&url)).await {
      Ok(answer) => {
        let mime = content_type(&answer).unwrap_or("application/octet-stream").to_owned();
        let bytes = answer.bytes().await.map_err(|error| transport(&url, &error))?;
        Ok(Some(ChatPicture {
          mime,
          bytes: bytes.to_vec(),
        }))
      }
      Err(ClientFailure::Answered {
        answer: Some(WireError {
          code: ErrorCode::NotFound,
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

  async fn get_with<T: DeserializeOwned>(
    &self,
    route: &str,
    query: &impl Serialize,
  ) -> Result<T, ClientFailure> {
    let url = self.url(route);
    self.call(&url, self.client.get(&url).query(query)).await
  }

  async fn post<T: DeserializeOwned>(
    &self,
    route: &str,
    body: &impl Serialize,
  ) -> Result<T, ClientFailure> {
    let url = self.url(route);
    let body = Outgoing::of(body).map_err(|error| ClientFailure::Transport {
      url: url.clone(),
      why: error.to_string(),
    })?;
    let answer = self.exchange_with(&url, self.client.post(&url), Some(&body)).await?;
    self.decoded(&url, answer).await
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
    let answer = self.exchange(url, request).await?;
    self.decoded(url, answer).await
  }

  /// A successful answer decoded as the route's: its JSON, or its multipart body with each file part written
  /// into the spool as it arrives.
  async fn decoded<T: DeserializeOwned>(&self, url: &str, answer: Response) -> Result<T, ClientFailure> {
    let undecodable = |why: String| ClientFailure::Undecodable {
      url: url.to_owned(),
      why,
    };
    let boundary = multipart_boundary(content_type(&answer)).map_err(|unread| self.unread(url, unread))?;
    let Some(boundary) = boundary else {
      let bytes = answer.bytes().await.map_err(|error| transport(url, &error))?;
      return serde_json::from_slice(&bytes).map_err(|error| undecodable(error.to_string()));
    };
    let form = multer::Multipart::new(answer.bytes_stream(), boundary);
    let (json, parts) = spooled(form, &self.spool).await.map_err(|unread| self.unread(url, unread))?;
    read_with_parts(&json, parts).map_err(undecodable)
  }

  /// A multipart answer that could not be taken.
  fn unread(&self, url: &str, unread: Unread) -> ClientFailure {
    let url = url.to_owned();
    match unread {
      Unread::Shape(why) => ClientFailure::Undecodable { url, why },
      Unread::Broken(why) => ClientFailure::Transport { url, why },
      Unread::Unkept(error) => ClientFailure::Transport {
        url,
        why: format!("a part of the answer could not be written to {}: {error}", self.spool.display()),
      },
    }
  }

  /// Makes a call that carries no body of its own.
  async fn exchange(&self, url: &str, request: RequestBuilder) -> Result<Response, ClientFailure> {
    self.exchange_with(url, request, None).await
  }

  /// Makes the call behind the provider's token, offering it again only while the connection is refused and
  /// the patience window lasts, and gives a successful answer, its body not yet read. A body is taken anew
  /// for each offer, so a file it carries as a part is read from the disk again.
  async fn exchange_with(
    &self,
    url: &str,
    request: RequestBuilder,
    body: Option<&Outgoing>,
  ) -> Result<Response, ClientFailure> {
    let url = url.to_owned();
    let request = request.bearer_auth(&self.token);
    let ChatPatience {
      window,
      every,
      waiter,
    } = &self.patience;
    let mut waited = Duration::ZERO;
    loop {
      let mut offered = request.try_clone().expect("a request with no body yet always clones");
      if let Some(body) = body {
        let bytes = body.body().await.map_err(|error| ClientFailure::Transport {
          url: url.clone(),
          why: format!("a file the body carries could not be read: {error}"),
        })?;
        offered = offered.header(CONTENT_TYPE, body.content_type()).body(bytes);
      }
      let error = match offered.send().await {
        Ok(answer) => {
          let status = answer.status();
          if status.is_success() {
            return Ok(answer);
          }
          let bytes = answer.bytes().await.map_err(|error| transport(&url, &error))?;
          return Err(ClientFailure::Answered {
            url,
            status: status.as_u16(),
            answer: serde_json::from_slice(&bytes).ok(),
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

/// The answer's `Content-Type`, where it gave one.
fn content_type(answer: &Response) -> Option<&str> {
  answer.headers().get(CONTENT_TYPE).and_then(|value| value.to_str().ok())
}

/// An answer whose body broke off while it was read.
fn transport(url: &str, error: &reqwest::Error) -> ClientFailure {
  ClientFailure::Transport {
    url: url.to_owned(),
    why: chain(error),
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

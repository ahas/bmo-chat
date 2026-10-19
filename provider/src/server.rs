//! The server for the provider routes: every route `docs/protocol.md` lists for a provider, over any
//! provider, behind the token the serving service supplies.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::Uri;
use axum::http::header::CONTENT_TYPE;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use headers::authorization::Bearer;
use headers::{Authorization, HeaderMapExt};
use serde::de::DeserializeOwned;

use crate::avatar::{AvatarList, AvatarsQuery, ChatAvatarKind, ChatPicture};
use crate::bodies::{
  ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, OwnerRoom, RoomIdentity,
  RoomKindAnswer, RoomOwns, RoomPerson, RoomQuery, RoomTitle,
};
use crate::capability::ChatCapability;
use crate::declaration::ChatDeclaration;
use crate::error::{ErrorCode, WireError};
use crate::media::{ChatMediaRequest, ChatMediaResult};
use crate::message::MessageFile;
use crate::provider::{ChatFailure, ChatProvider};
use crate::send::{ChatSendRequest, ChatSendResult, ChatSent};

/// The provider routes over `provider`, each answering only a request that carries `token` as its bearer
/// token. The serving service mounts the router where it likes and supplies the token from its own
/// configuration. No body limit is set, since a send carries its files in its body.
pub fn provider_router(provider: Arc<dyn ChatProvider>, token: impl Into<String>) -> Router {
  let served = Served {
    provider,
    token: token.into().into(),
  };
  Router::new()
    .route("/declaration", get(declaration))
    .route("/health", get(health))
    .route("/send", post(send))
    .route("/media", post(media))
    .route("/dialect/read", post(dialect_read))
    .route("/dialect/check", post(dialect_check))
    .route("/room/owns", post(room_owns))
    .route("/room/identity", post(room_identity))
    .route("/room/is-owner", post(room_is_owner))
    .route("/room/private-of", post(room_private_of))
    .route("/room/title", post(room_title))
    .route("/owner-room", get(owner_room))
    .route("/avatars", get(avatars))
    .route("/avatars/{kind}/{id}", get(avatar))
    .fallback(no_route)
    .method_not_allowed_fallback(no_route)
    .layer(DefaultBodyLimit::disable())
    .layer(from_fn_with_state(served.clone(), authorized))
    .with_state(served)
}

#[derive(Clone)]
struct Served {
  provider: Arc<dyn ChatProvider>,
  token: Arc<str>,
}

/// A provider that failed while answering is answered `failed`, with the failure as the sentence.
impl From<ChatFailure> for WireError {
  fn from(failure: ChatFailure) -> Self {
    WireError::new(ErrorCode::Failed, failure.to_string())
  }
}

type Answered<T> = Result<Json<T>, WireError>;

async fn authorized(State(served): State<Served>, request: Request, next: Next) -> Response {
  let bearer = request.headers().typed_get::<Authorization<Bearer>>();
  match bearer {
    Some(Authorization(bearer)) if bearer.token() == &*served.token => next.run(request).await,
    _ => WireError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong.").into_response(),
  }
}

async fn no_route(request: Request) -> WireError {
  no_route_at(request.method().as_str(), request.uri())
}

fn no_route_at(method: &str, uri: &Uri) -> WireError {
  WireError::new(ErrorCode::NoRoute, format!("There is no route at {method} {}.", uri.path()))
}

/// The body read as the shape the route takes.
fn read<T: DeserializeOwned>(body: &Bytes) -> Result<T, WireError> {
  serde_json::from_slice(body).map_err(|error| {
    WireError::new(ErrorCode::BadRequest, format!("The body is not the shape this route takes: {error}."))
  })
}

/// The provider, where it is ready to answer the gated routes.
fn ready(served: &Served) -> Result<&dyn ChatProvider, WireError> {
  let provider = &*served.provider;
  let ChatHealth { ready, reason } = provider.health();
  if ready {
    return Ok(provider);
  }
  let label = provider.label();
  let error = match reason {
    Some(reason) => format!("{label} is not ready yet: {reason}"),
    None => format!("{label} is not ready yet."),
  };
  Err(WireError::new(ErrorCode::NotReady, error))
}

async fn declaration(State(served): State<Served>) -> Json<ChatDeclaration> {
  Json(served.provider.declaration())
}

async fn health(State(served): State<Served>) -> Json<ChatHealth> {
  Json(served.provider.health())
}

/// A send the provider performed is answered with what it came to; a refusal, in the error shape.
async fn send(State(served): State<Served>, body: Bytes) -> Answered<ChatSent> {
  let request: ChatSendRequest = read(&body)?;
  let provider = ready(&served)?;
  if let Some(refusal) = provider.undeclared(request.send.verb().into()) {
    return Err(refusal.into());
  }
  match provider.send(request).await? {
    ChatSendResult::Sent(sent) => Ok(Json(sent)),
    ChatSendResult::Refused(refusal) => Err(refusal.into()),
  }
}

/// A fetched file is answered as itself; a file the platform would not give, and a refusal, in the error
/// shape.
async fn media(State(served): State<Served>, body: Bytes) -> Answered<MessageFile> {
  let request: ChatMediaRequest = read(&body)?;
  let provider = ready(&served)?;
  if let Some(refusal) = provider.undeclared(ChatCapability::FetchMedia) {
    return Err(refusal.into());
  }
  match provider.fetch_media(request).await? {
    ChatMediaResult::Fetched(file) => Ok(Json(file)),
    ChatMediaResult::Unavailable { reason } => Err(WireError::new(ErrorCode::Unavailable, reason)),
    ChatMediaResult::Refused(refusal) => Err(refusal.into()),
  }
}

async fn dialect_read(State(served): State<Served>, body: Bytes) -> Answered<DialectReading> {
  let DialectRead { text } = read(&body)?;
  let dialect = served.provider.dialect();
  Ok(Json(DialectReading {
    markup: dialect.convert(&text),
    parts: dialect.parts(&text),
    fold: dialect.folded(&text),
  }))
}

async fn dialect_check(State(served): State<Served>, body: Bytes) -> Answered<DialectChecked> {
  let DialectCheck { markup } = read(&body)?;
  let dialect = served.provider.dialect();
  Ok(Json(DialectChecked {
    fault: dialect.fault(&markup),
    visible: dialect.visible(&markup),
  }))
}

async fn room_owns(State(served): State<Served>, body: Bytes) -> Answered<RoomOwns> {
  let RoomQuery { chat_id } = read(&body)?;
  let owns = ready(&served)?.owns_room(chat_id).await?;
  Ok(Json(RoomOwns { owns }))
}

async fn room_identity(State(served): State<Served>, body: Bytes) -> Answered<RoomIdentity> {
  let RoomQuery { chat_id } = read(&body)?;
  let identity = ready(&served)?.identity_of(chat_id).await?;
  Ok(Json(RoomIdentity { identity }))
}

async fn room_is_owner(State(served): State<Served>, body: Bytes) -> Answered<RoomKindAnswer> {
  let RoomQuery { chat_id } = read(&body)?;
  let kind = ready(&served)?.is_owner_room(chat_id).await?;
  Ok(Json(RoomKindAnswer { kind }))
}

async fn room_private_of(State(served): State<Served>, body: Bytes) -> Answered<RoomPerson> {
  let RoomQuery { chat_id } = read(&body)?;
  let person = ready(&served)?.private_room_of(chat_id).await?;
  Ok(Json(RoomPerson { person }))
}

async fn room_title(State(served): State<Served>, body: Bytes) -> Answered<RoomTitle> {
  let RoomQuery { chat_id } = read(&body)?;
  let title = ready(&served)?.room_title(chat_id).await?;
  Ok(Json(RoomTitle { title }))
}

async fn owner_room(State(served): State<Served>) -> Answered<OwnerRoom> {
  let chat_id = ready(&served)?.owner_room().await?;
  Ok(Json(OwnerRoom { chat_id }))
}

async fn avatars(
  State(served): State<Served>,
  query: Result<Query<AvatarsQuery>, QueryRejection>,
) -> Answered<AvatarList> {
  let Query(query) = query.map_err(|rejection| {
    WireError::new(
      ErrorCode::BadRequest,
      format!("The query is not the shape this route takes: {rejection}."),
    )
  })?;
  let provider = &*served.provider;
  if !provider.can().contains(&ChatCapability::Avatars) {
    return Ok(Json(AvatarList {
      avatars: Vec::new(),
    }));
  }
  Ok(Json(AvatarList {
    avatars: query.page(provider.avatars().await?),
  }))
}

/// A path whose kind is neither `room` nor `person`, or whose id is not a number, names no route.
async fn avatar(
  State(served): State<Served>,
  uri: Uri,
  subject: Result<Path<(ChatAvatarKind, i64)>, PathRejection>,
) -> Result<Response, WireError> {
  let Path((kind, id)) = subject.map_err(|_| no_route_at("GET", &uri))?;
  let provider = &*served.provider;
  let kept = if provider.can().contains(&ChatCapability::Avatars) {
    provider.avatar(kind, id).await?
  } else {
    None
  };
  let ChatPicture { mime, bytes } = kept.ok_or_else(|| {
    WireError::new(ErrorCode::NothingHeld, format!("No profile picture is kept for {kind} {id}."))
  })?;
  Ok(([(CONTENT_TYPE, mime)], bytes).into_response())
}

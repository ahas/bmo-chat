//! The server for the provider routes: every route `docs/protocol.md` lists for a provider, over any
//! provider, behind the token the serving service supplies.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{StatusCode, Uri};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use headers::authorization::Bearer;
use headers::{Authorization, HeaderMapExt};
use serde::de::DeserializeOwned;

use crate::avatar::{AvatarsQuery, ChatAvatar, ChatAvatarKind, ChatPicture};
use crate::bodies::{
  AccountRoomsQuery, ChatHealth, DialectCheck, DialectChecked, DialectRead, DialectReading, Page,
  ProviderOwner, ProviderRoom,
};
use crate::capability::ChatCapability;
use crate::declaration::ChatDeclaration;
use crate::error::{ErrorCode, WireError};
use crate::media::{ChatMediaRequest, ChatMediaResult, FilesQuery};
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
    .route("/sends", post(send))
    .route("/rooms", get(account_rooms))
    .route("/rooms/{room_id}", get(room))
    .route("/owner", get(owner))
    .route("/rooms/{room_id}/messages/{message_id}/files", get(files))
    .route("/dialect/readings", post(dialect_reading))
    .route("/dialect/checks", post(dialect_check))
    .route("/avatars", get(avatars))
    .route("/rooms/{room_id}/avatar", get(room_avatar))
    .route("/people/{person_id}/avatar", get(person_avatar))
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

/// The query read as the shape the route takes.
fn query<T>(query: Result<Query<T>, QueryRejection>) -> Result<T, WireError> {
  let Query(query) = query.map_err(|rejection| {
    WireError::new(
      ErrorCode::BadRequest,
      format!("The query is not the shape this route takes: {rejection}."),
    )
  })?;
  Ok(query)
}

/// The path's keys, where they are numbers; a key that is not names no route.
fn keys<T>(uri: &Uri, path: Result<Path<T>, PathRejection>) -> Result<T, WireError> {
  let Path(keys) = path.map_err(|_| no_route_at("GET", uri))?;
  Ok(keys)
}

/// The provider, where it is ready to answer the gated routes.
fn ready(served: &Served) -> Result<&dyn ChatProvider, WireError> {
  let provider = &*served.provider;
  let ChatHealth { ready, reason, .. } = provider.health();
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

/// A send the provider performed is answered 201 with what it came to; a refusal, in the error shape.
async fn send(
  State(served): State<Served>,
  body: Bytes,
) -> Result<(StatusCode, Json<ChatSent>), WireError> {
  let request: ChatSendRequest = read(&body)?;
  let provider = ready(&served)?;
  if let Some(refusal) = provider.undeclared(request.send.kind().into()) {
    return Err(refusal.into());
  }
  match provider.send(request).await? {
    ChatSendResult::Sent(sent) => Ok((StatusCode::CREATED, Json(sent))),
    ChatSendResult::Refused(refusal) => Err(refusal.into()),
  }
}

/// The rooms of the provider's account as one page; refused where the provider does not declare `roomList`.
async fn account_rooms(
  State(served): State<Served>,
  asked: Result<Query<AccountRoomsQuery>, QueryRejection>,
) -> Answered<Page<ProviderRoom>> {
  let asked = query(asked)?;
  let provider = ready(&served)?;
  if let Some(refusal) = provider.undeclared(ChatCapability::RoomList) {
    return Err(refusal.into());
  }
  Ok(Json(asked.page(provider.account_rooms().await?)))
}

async fn room(
  State(served): State<Served>,
  uri: Uri,
  path: Result<Path<i64>, PathRejection>,
) -> Answered<ProviderRoom> {
  let room_id = keys(&uri, path)?;
  let room = ready(&served)?.room(room_id).await?;
  let room = room
    .ok_or_else(|| WireError::new(ErrorCode::NotFound, format!("Nothing is known of room {room_id}.")))?;
  Ok(Json(room))
}

async fn owner(State(served): State<Served>) -> Answered<ProviderOwner> {
  let room_id = ready(&served)?.owner_room().await?;
  Ok(Json(ProviderOwner { room_id }))
}

/// The fetched files are answered as one page; files the platform would not give, and a refusal, in the
/// error shape.
async fn files(
  State(served): State<Served>,
  uri: Uri,
  path: Result<Path<(i64, i64)>, PathRejection>,
  asked: Result<Query<FilesQuery>, QueryRejection>,
) -> Answered<Page<MessageFile>> {
  let (room_id, message_id) = keys(&uri, path)?;
  let FilesQuery { sent_at } = query(asked)?;
  let provider = ready(&served)?;
  if let Some(refusal) = provider.undeclared(ChatCapability::FetchMedia) {
    return Err(refusal.into());
  }
  let request = ChatMediaRequest {
    room_id,
    message_id,
    sent_at,
  };
  match provider.fetch_media(request).await? {
    ChatMediaResult::Fetched(files) => Ok(Json(Page::whole(files))),
    ChatMediaResult::Unavailable { reason } => Err(WireError::new(ErrorCode::Unavailable, reason)),
    ChatMediaResult::Refused(refusal) => Err(refusal.into()),
  }
}

async fn dialect_reading(State(served): State<Served>, body: Bytes) -> Answered<DialectReading> {
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

async fn avatars(
  State(served): State<Served>,
  asked: Result<Query<AvatarsQuery>, QueryRejection>,
) -> Answered<Page<ChatAvatar>> {
  let asked = query(asked)?;
  let provider = &*served.provider;
  if !provider.can().contains(&ChatCapability::Avatars) {
    return Ok(Json(Page::whole(Vec::new())));
  }
  Ok(Json(asked.page(provider.avatars().await?)))
}

async fn room_avatar(
  State(served): State<Served>,
  uri: Uri,
  path: Result<Path<i64>, PathRejection>,
) -> Result<Response, WireError> {
  picture(&served, ChatAvatarKind::Room, keys(&uri, path)?).await
}

async fn person_avatar(
  State(served): State<Served>,
  uri: Uri,
  path: Result<Path<i64>, PathRejection>,
) -> Result<Response, WireError> {
  picture(&served, ChatAvatarKind::Person, keys(&uri, path)?).await
}

/// The picture kept for a room or a person, as its bytes; `not-found` where none is kept.
async fn picture(served: &Served, kind: ChatAvatarKind, id: i64) -> Result<Response, WireError> {
  let provider = &*served.provider;
  let kept = if provider.can().contains(&ChatCapability::Avatars) {
    provider.avatar(kind, id).await?
  } else {
    None
  };
  let ChatPicture { mime, bytes } = kept.ok_or_else(|| {
    WireError::new(ErrorCode::NotFound, format!("No profile picture is kept for {kind} {id}."))
  })?;
  Ok(([(CONTENT_TYPE, mime)], bytes).into_response())
}

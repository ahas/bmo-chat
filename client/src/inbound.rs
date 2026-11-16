//! The server for the receiving-side routes: the hand-overs and questions `docs/protocol.md` lists for the
//! receiving side, handed to the receiving side the consuming service supplies, behind the token it supplies.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::header::RETRY_AFTER;
use axum::http::{Method, StatusCode, Uri};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post, put};
use headers::authorization::Bearer;
use headers::{Authorization, HeaderMapExt};
use serde::de::DeserializeOwned;

use crate::bodies::Page;
use crate::error::{ErrorCode, RETRY_AFTER_SECONDS, WireError};
use crate::file_body::FileBody;
use crate::message::{GatheredPatch, HandedMessage, MessageFile};
use crate::parts::arrived;
use crate::presence::{PresenceChange, TypingMark};
use crate::receiver_bodies::{
  AvatarChange, Deletion, HeldMessage, ReactionChange, ReceiverRoom, RoomQuery, RoomsQuery,
};

/// What the receiving side does with what a provider hands over, and how it answers what a provider asks.
/// The consuming service implements it. A call it could not take or answer is answered with the error it
/// gives, in the error shape and at its code's status.
#[async_trait]
pub trait ChatReceiver: Send + Sync {
  /// A message of a room handed over, whatever its origin: the message as the receiving side now holds it,
  /// and whether it held it before. A `live` message already held is not taken again, an `edit` replaces
  /// the words and the sender held, and a `history` message already held is left as it is.
  async fn hand_over(
    &self,
    room_id: i64,
    message_id: i64,
    message: HandedMessage,
  ) -> Result<HeldMessage, WireError>;

  /// The files a message's gathering promised, or why they were not kept: the message as the receiving side
  /// now holds it, no longer gathering. `not-found` where it does not hold the message, `conflict` where the
  /// message promised no files.
  async fn gathered(
    &self,
    room_id: i64,
    message_id: i64,
    patch: GatheredPatch,
  ) -> Result<HandedMessage, WireError>;

  /// Word that the kept profile pictures of these rooms and people changed.
  async fn avatars_changed(&self, change: AvatarChange) -> Result<(), WireError>;

  /// One message's reactions as the platform shows them now: each person listed takes the emoji listed for them,
  /// and where the change is `whole`, a person not listed has none. `not-found` where it does not hold the message.
  async fn reactions_changed(&self, change: ReactionChange) -> Result<(), WireError>;

  /// Messages of one room deleted on the platform: each one held marked deleted, its words kept.
  async fn deleted(&self, deletion: Deletion) -> Result<(), WireError>;

  /// A person writing in a room now, or no longer; a mark about a room nothing is held of is passed over.
  /// The default passes every mark over, for a receiving side that shows no typing.
  async fn typing_marked(&self, _mark: TypingMark) -> Result<(), WireError> {
    Ok(())
  }

  /// Whether people are reachable now; a person held in no room is passed over. The default passes every
  /// change over, for a receiving side that shows no presence.
  async fn presence_changed(&self, _change: PresenceChange) -> Result<(), WireError> {
    Ok(())
  }

  /// The room as the receiving side holds it for the provider `via`, or null where it holds nothing of it.
  async fn room(&self, room_id: i64, query: RoomQuery) -> Result<Option<ReceiverRoom>, WireError>;

  /// The directory each file a multipart hand-over or patch carries is written into as it arrives
  /// (`contract.md`, "A file's bytes"): the system's temporary directory unless the receiving side names its
  /// own. A receiving side that keeps the files names a directory on the disk it keeps them on, so each is
  /// moved into place rather than copied.
  fn spool(&self) -> PathBuf {
    std::env::temp_dir()
  }

  /// The rooms of the provider `via` whose past it may read back, each with the oldest message held of it:
  /// in ascending order of room number, only those after `after`, at most `limit`. The router asks for one
  /// more than its caller's limit, to tell whether more stand past the page it answers.
  async fn rooms(&self, query: RoomsQuery) -> Result<Vec<ReceiverRoom>, WireError>;
}

/// The receiving-side routes over `receiver`, each answering only a request that carries `token` as its
/// bearer token. The serving service mounts the router where it likes and supplies the token from its own
/// configuration. No body limit is set, since a hand-over carries its files in its body, and a file part is
/// written to the receiving side's spool as it arrives rather than held in memory.
pub fn receiver_router(receiver: Arc<dyn ChatReceiver>, token: impl Into<String>) -> Router {
  let served = Served {
    receiver,
    token: token.into().into(),
  };
  Router::new()
    .route("/rooms", get(rooms))
    .route("/rooms/{room_id}", get(room))
    .route("/rooms/{room_id}/messages/{message_id}", put(hand_over).patch(gathered))
    .route("/avatar-changes", post(avatars_changed))
    .route("/reaction-changes", post(reactions_changed))
    .route("/deletions", post(deleted))
    .route("/typing-marks", post(typing_marked))
    .route("/presence-changes", post(presence_changed))
    .fallback(no_route)
    .method_not_allowed_fallback(no_route)
    .layer(DefaultBodyLimit::disable())
    .layer(from_fn_with_state(served.clone(), authorized))
    .with_state(served)
}

#[derive(Clone)]
struct Served {
  receiver: Arc<dyn ChatReceiver>,
  token: Arc<str>,
}

/// An answer in the error shape, with its code's status and, for `not-ready`, a `Retry-After`.
struct Answer(WireError);

impl IntoResponse for Answer {
  fn into_response(self) -> Response {
    let Answer(error) = self;
    let status = StatusCode::from_u16(error.code.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let not_ready = error.code == ErrorCode::NotReady;
    let mut response = (status, Json(error)).into_response();
    if not_ready {
      response.headers_mut().insert(RETRY_AFTER, RETRY_AFTER_SECONDS.into());
    }
    response
  }
}

impl From<WireError> for Answer {
  fn from(error: WireError) -> Self {
    Answer(error)
  }
}

type Answered<T> = Result<Json<T>, Answer>;

async fn authorized(State(served): State<Served>, request: Request, next: Next) -> Response {
  let bearer = request.headers().typed_get::<Authorization<Bearer>>();
  match bearer {
    Some(Authorization(bearer)) if bearer.token() == &*served.token => next.run(request).await,
    _ => Answer(WireError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong."))
      .into_response(),
  }
}

async fn no_route(request: Request) -> Answer {
  no_route_at(request.method().as_str(), request.uri().path())
}

fn no_route_at(method: &str, path: &str) -> Answer {
  Answer(WireError::new(ErrorCode::NoRoute, format!("There is no route at {method} {path}.")))
}

/// The body read as the shape the route takes.
fn read<T: DeserializeOwned>(body: &Bytes) -> Result<T, Answer> {
  serde_json::from_slice(body).map_err(|error| {
    Answer(WireError::new(
      ErrorCode::BadRequest,
      format!("The body is not the shape this route takes: {error}."),
    ))
  })
}

/// The query read as the shape the route takes.
fn query<T>(query: Result<Query<T>, QueryRejection>) -> Result<T, Answer> {
  let Query(query) = query.map_err(|rejection| {
    Answer(WireError::new(
      ErrorCode::BadRequest,
      format!("The query is not the shape this route takes: {rejection}."),
    ))
  })?;
  Ok(query)
}

/// The path's keys, where they are numbers; a key that is not names no route.
fn keys<T>(method: &Method, uri: &Uri, path: Result<Path<T>, PathRejection>) -> Result<T, Answer> {
  let Path(keys) = path.map_err(|_| no_route_at(method.as_str(), uri.path()))?;
  Ok(keys)
}

async fn rooms(
  State(served): State<Served>,
  asked: Result<Query<RoomsQuery>, QueryRejection>,
) -> Answered<Page<ReceiverRoom>> {
  let asked = query(asked)?;
  let limit = asked.limit;
  let mut items = served
    .receiver
    .rooms(RoomsQuery {
      limit: limit.map(|limit| limit.saturating_add(1)),
      ..asked
    })
    .await?;
  let more = limit.is_some_and(|limit| items.len() > limit as usize);
  if let Some(limit) = limit {
    items.truncate(limit as usize);
  }
  Ok(Json(Page { items, more }))
}

async fn room(
  State(served): State<Served>,
  method: Method,
  uri: Uri,
  path: Result<Path<i64>, PathRejection>,
  asked: Result<Query<RoomQuery>, QueryRejection>,
) -> Answered<ReceiverRoom> {
  let room_id = keys(&method, &uri, path)?;
  let room = served.receiver.room(room_id, query(asked)?).await?;
  let room = room.ok_or_else(|| {
    Answer(WireError::new(ErrorCode::NotFound, format!("Nothing is held of room {room_id}.")))
  })?;
  Ok(Json(room))
}

/// A message handed over, its body JSON or multipart; answered as held, its files withheld.
async fn hand_over(
  State(served): State<Served>,
  method: Method,
  uri: Uri,
  path: Result<Path<(i64, i64)>, PathRejection>,
  body: Request,
) -> Result<(StatusCode, Json<HandedMessage>), Answer> {
  let (room_id, message_id) = keys(&method, &uri, path)?;
  let message: HandedMessage = arrived(body, &served.receiver.spool()).await?;
  carried(&message.files)?;
  let HeldMessage { created, message } = served.receiver.hand_over(room_id, message_id, message).await?;
  let status = if created { StatusCode::CREATED } else { StatusCode::OK };
  Ok((status, Json(withheld(message))))
}

/// A gathering's patch, its body JSON or multipart; answered with the message as held, its files withheld.
async fn gathered(
  State(served): State<Served>,
  method: Method,
  uri: Uri,
  path: Result<Path<(i64, i64)>, PathRejection>,
  body: Request,
) -> Answered<HandedMessage> {
  let (room_id, message_id) = keys(&method, &uri, path)?;
  let patch: GatheredPatch = arrived(body, &served.receiver.spool()).await?;
  carried(&patch.files)?;
  let message = served.receiver.gathered(room_id, message_id, patch).await?;
  Ok(Json(withheld(message)))
}

/// Nothing where every file carries its bytes; a bad request naming the first that does not.
fn carried(files: &[MessageFile]) -> Result<(), Answer> {
  match files.iter().find(|file| file.bytes.is_withheld()) {
    Some(file) => Err(Answer(WireError::new(
      ErrorCode::BadRequest,
      format!("The file {} carries no bytes.", file.id),
    ))),
    None => Ok(()),
  }
}

/// The message as an answer names it: each file without its bytes, which the provider that sent them holds.
fn withheld(mut message: HandedMessage) -> HandedMessage {
  for file in &mut message.files {
    file.bytes = FileBody::Withheld;
  }
  message
}

async fn avatars_changed(
  State(served): State<Served>,
  body: Bytes,
) -> Result<(StatusCode, Json<AvatarChange>), Answer> {
  let change: AvatarChange = read(&body)?;
  served.receiver.avatars_changed(change.clone()).await?;
  Ok((StatusCode::CREATED, Json(change)))
}

async fn reactions_changed(
  State(served): State<Served>,
  body: Bytes,
) -> Result<(StatusCode, Json<ReactionChange>), Answer> {
  let change: ReactionChange = read(&body)?;
  served.receiver.reactions_changed(change.clone()).await?;
  Ok((StatusCode::CREATED, Json(change)))
}

async fn deleted(
  State(served): State<Served>,
  body: Bytes,
) -> Result<(StatusCode, Json<Deletion>), Answer> {
  let deletion: Deletion = read(&body)?;
  served.receiver.deleted(deletion.clone()).await?;
  Ok((StatusCode::CREATED, Json(deletion)))
}

async fn typing_marked(
  State(served): State<Served>,
  body: Bytes,
) -> Result<(StatusCode, Json<TypingMark>), Answer> {
  let mark: TypingMark = read(&body)?;
  served.receiver.typing_marked(mark.clone()).await?;
  Ok((StatusCode::CREATED, Json(mark)))
}

async fn presence_changed(
  State(served): State<Served>,
  body: Bytes,
) -> Result<(StatusCode, Json<PresenceChange>), Answer> {
  let change: PresenceChange = read(&body)?;
  served.receiver.presence_changed(change.clone()).await?;
  Ok((StatusCode::CREATED, Json(change)))
}

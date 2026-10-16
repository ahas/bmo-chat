//! The server for the receiver routes: the hand-overs `docs/protocol.md` lists for the receiver, handed to
//! the receiver the consuming service supplies, behind the token it supplies.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::StatusCode;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::post;
use headers::authorization::Bearer;
use headers::{Authorization, HeaderMapExt};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::error::{ErrorCode, WireError};
use crate::message::{ChatGathered, ChatHandover};

/// What the receiver does with what a provider hands over. The consuming service implements it. A hand-over
/// it could not take is answered with the error it gives, in the error shape and at its code's status.
#[async_trait]
pub trait ChatReceiver: Send + Sync {
  /// A message a room said.
  async fn message(&self, handover: ChatHandover) -> Result<(), WireError>;

  /// A message carrying a file, with the file or the promise of it.
  async fn attachment(&self, handover: ChatHandover) -> Result<(), WireError>;

  /// A message already handed over, said again with different words.
  async fn edited(&self, handover: ChatHandover) -> Result<(), WireError>;

  /// The file a message's `gathering` promised, or why it was not kept.
  async fn gathered(&self, gathered: ChatGathered) -> Result<(), WireError>;
}

/// The receiver routes over `receiver`, each answering only a request that carries `token` as its bearer
/// token. The serving service mounts the router where it likes and supplies the token from its own
/// configuration. No body limit is set, since a hand-over carries its files in its body.
pub fn receiver_router(receiver: Arc<dyn ChatReceiver>, token: impl Into<String>) -> Router {
  let served = Served {
    receiver,
    token: token.into().into(),
  };
  Router::new()
    .route("/message", post(message))
    .route("/attachment", post(attachment))
    .route("/edited", post(edited))
    .route("/gathered", post(gathered))
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

/// An answer in the error shape.
struct Answer(WireError);

impl IntoResponse for Answer {
  fn into_response(self) -> Response {
    let Answer(error) = self;
    let status = StatusCode::from_u16(error.code.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(error)).into_response()
  }
}

impl From<WireError> for Answer {
  fn from(error: WireError) -> Self {
    Answer(error)
  }
}

/// A hand-over taken: an empty JSON object.
type Taken = Result<Json<Map<String, Value>>, Answer>;

fn taken() -> Taken {
  Ok(Json(Map::new()))
}

async fn authorized(State(served): State<Served>, request: Request, next: Next) -> Response {
  let bearer = request.headers().typed_get::<Authorization<Bearer>>();
  match bearer {
    Some(Authorization(bearer)) if bearer.token() == &*served.token => next.run(request).await,
    _ => Answer(WireError::new(ErrorCode::Unauthorized, "The bearer token is missing or wrong."))
      .into_response(),
  }
}

async fn no_route(request: Request) -> Answer {
  Answer(WireError::new(
    ErrorCode::NoRoute,
    format!("There is no route at {} {}.", request.method(), request.uri().path()),
  ))
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

async fn message(State(served): State<Served>, body: Bytes) -> Taken {
  served.receiver.message(read(&body)?).await?;
  taken()
}

async fn attachment(State(served): State<Served>, body: Bytes) -> Taken {
  served.receiver.attachment(read(&body)?).await?;
  taken()
}

async fn edited(State(served): State<Served>, body: Bytes) -> Taken {
  served.receiver.edited(read(&body)?).await?;
  taken()
}

async fn gathered(State(served): State<Served>, body: Bytes) -> Taken {
  served.receiver.gathered(read(&body)?).await?;
  taken()
}

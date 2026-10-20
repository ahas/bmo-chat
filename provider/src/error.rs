//! The body of every answer that is not a success.

use axum::http::StatusCode;
use axum::http::header::RETRY_AFTER;
use axum::response::{IntoResponse, Json, Response};
use serde::{Deserialize, Serialize};
use strum::VariantArray;

/// How long a caller answered `not-ready` waits before it asks again, in seconds.
pub const RETRY_AFTER_SECONDS: u32 = 5;

/// Why a request was not answered with success: it could not be answered at all, or it was refused. Every
/// code of the common wire format is here, beside the protocol's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, VariantArray)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
  /// The body or the query is not the shape the route takes.
  BadRequest,
  /// The token is missing or wrong.
  Unauthorized,
  /// The token is right but the caller may not do this.
  Forbidden,
  /// No route answers this method and path.
  NoRoute,
  /// The route is right, but nothing stands at the key the path names.
  NotFound,
  /// The member's state does not allow the change, or the key is taken.
  Conflict,
  /// The member stood once and never will again, so the caller lets go of it. As a refusal: the message is
  /// one the platform no longer has, or will never let change again.
  Gone,
  /// The body or a file is over the route's ceiling.
  TooLarge,
  /// The service failed while answering.
  Failed,
  /// A service this one called failed.
  UpstreamFailed,
  /// The service is up but cannot answer yet.
  NotReady,
  /// The platform would not give the files asked for.
  Unavailable,
  /// Refused: the provider does not declare the capability the call needs.
  UnsupportedCapability,
  /// Refused: none of the pictures of the send went.
  NoPictureSent,
  /// Refused: the provider speaks as the room's own account alone, so it takes no other speaker.
  SpeakerNotAllowed,
  /// Refused: the provider has no number in the room for the speaker the send names.
  SpeakerUnnumbered,
  /// Refused by the platform, for the provider's own English reason.
  Platform,
}

impl ErrorCode {
  /// The HTTP status the code is answered with.
  pub fn status(self) -> u16 {
    match self {
      Self::BadRequest => 400,
      Self::Unauthorized => 401,
      Self::Forbidden | Self::SpeakerNotAllowed => 403,
      Self::NoRoute | Self::NotFound | Self::Unavailable => 404,
      Self::Conflict => 409,
      Self::Gone => 410,
      Self::TooLarge => 413,
      Self::UnsupportedCapability | Self::NoPictureSent | Self::SpeakerUnnumbered | Self::Platform => 422,
      Self::Failed => 500,
      Self::UpstreamFailed => 502,
      Self::NotReady => 503,
    }
  }
}

/// The error shape: a code to match on and an English sentence saying what went wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireError {
  pub code: ErrorCode,
  pub error: String,
}

impl WireError {
  pub fn new(code: ErrorCode, error: impl Into<String>) -> Self {
    Self {
      code,
      error: error.into(),
    }
  }
}

/// The error shape as an answer, with its code's status and, for `not-ready`, a `Retry-After`, for the base's
/// routes and for any route a provider serves of its own beside them.
impl IntoResponse for WireError {
  fn into_response(self) -> Response {
    let status = StatusCode::from_u16(self.code.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let not_ready = self.code == ErrorCode::NotReady;
    let mut response = (status, Json(self)).into_response();
    if not_ready {
      response.headers_mut().insert(RETRY_AFTER, RETRY_AFTER_SECONDS.into());
    }
    response
  }
}

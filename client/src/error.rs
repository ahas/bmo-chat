//! The body of every answer that is not a success.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::VariantArray;

/// How long a caller answered `not-ready` waits before it asks again, in seconds.
pub const RETRY_AFTER_SECONDS: u32 = 5;

/// Why a request was not answered with success: it could not be answered at all, or it was refused. Every
/// code of the common wire format is here, beside the protocol's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, VariantArray)]
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

  /// When the code is answered, as the sentence an OpenAPI document describes the answer with.
  pub fn meaning(self) -> &'static str {
    match self {
      Self::BadRequest => {
        "the body is not JSON, or the body or the query is not the shape the route takes; the sentence names \
         what is wrong."
      }
      Self::Unauthorized => "the bearer token is missing or wrong.",
      Self::Forbidden => "the token is right, but the caller may not do this.",
      Self::NoRoute => "no route answers this method and path, a path key that is not a number among them.",
      Self::NotFound => "the route is right, but nothing stands at the key the path names.",
      Self::Conflict => "the member's state does not allow the change, or the key is taken.",
      Self::Gone => {
        "the member stood once and never will again; as a refusal, the message is one the platform no longer \
         has, or will never let change again, so the caller lets go of it instead of asking again."
      }
      Self::TooLarge => "the body or a file is over the route's ceiling.",
      Self::Failed => "the service failed while answering; the sentence is the failure.",
      Self::UpstreamFailed => "a service this one called failed; the sentence names it.",
      Self::NotReady => {
        "the service is up but cannot answer yet, such as a provider whose platform connection is not open \
         yet; `Retry-After` says when to ask again."
      }
      Self::Unavailable => "the platform would not give the files asked for; the sentence says why.",
      Self::UnsupportedCapability => {
        "refused: the provider does not declare the capability the call needs."
      }
      Self::NoPictureSent => {
        "refused: none of the pictures of the send went; the sentence names each with why."
      }
      Self::SpeakerNotAllowed => {
        "refused: the provider speaks as the room's own account alone, so it takes no other speaker."
      }
      Self::SpeakerUnnumbered => {
        "refused: the provider has no number in the room for the speaker the send names."
      }
      Self::Platform => "refused by the platform; the sentence is the provider's own English reason.",
    }
  }
}

/// The error shape: a code to match on and an English sentence saying what went wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

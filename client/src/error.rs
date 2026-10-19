//! The body of every answer that is not a success.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::VariantArray;

/// Why a request was not answered with success: it could not be answered at all, or it was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, VariantArray)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
  /// The body is not JSON, or not the shape the route takes.
  BadRequest,
  /// The token is missing or wrong.
  Unauthorized,
  /// No route at this method and path.
  NoRoute,
  /// The route is right, but nothing is kept at the address the path names.
  NothingHeld,
  /// The service is up but not ready to answer this route.
  NotReady,
  /// The service failed while answering.
  Failed,
  /// The platform would not give the file asked for.
  Unavailable,
  /// Refused: the provider does not declare the capability the call needs.
  UnsupportedCapability,
  /// Refused: none of the pictures of the send went.
  NoPictureSent,
  /// Refused: the provider speaks as the room's own agent alone, so it takes no other speaker.
  SpeakerNotAllowed,
  /// Refused: the provider has no number in the room for the agent named as speaker.
  SpeakerUnnumbered,
  /// Refused by the platform, for the provider's own English reason.
  Platform,
  /// Refused: the message is one the platform no longer has, or will never let change again.
  Gone,
}

impl ErrorCode {
  /// The HTTP status the code is answered with.
  pub fn status(self) -> u16 {
    match self {
      Self::BadRequest => 400,
      Self::Unauthorized => 401,
      Self::SpeakerNotAllowed => 403,
      Self::NoRoute | Self::NothingHeld | Self::Unavailable => 404,
      Self::NotReady => 409,
      Self::Gone => 410,
      Self::UnsupportedCapability | Self::NoPictureSent | Self::SpeakerUnnumbered | Self::Platform => 422,
      Self::Failed => 500,
    }
  }

  /// When the code is answered, as the OpenAPI document describes the answer.
  pub(crate) fn meaning(self) -> &'static str {
    match self {
      Self::BadRequest => {
        "the body is not JSON, or the body or the query is not the shape the route takes; the sentence names \
         what is missing."
      }
      Self::Unauthorized => "the bearer token is missing or wrong.",
      Self::NoRoute => "there is no route at this method and path.",
      Self::NothingHeld => {
        "the route is right, but nothing is kept at the address the path names; a path whose parameters name \
         no address the route can hold is answered `no-route`."
      }
      Self::NotReady => {
        "the service is up but not ready to answer this route, such as a provider whose platform connection \
         is not open yet."
      }
      Self::Failed => "the service failed while answering; the sentence is the failure.",
      Self::Unavailable => "the platform would not give the file asked for; the sentence says why.",
      Self::UnsupportedCapability => {
        "refused: the provider does not declare the capability the call needs."
      }
      Self::NoPictureSent => {
        "refused: none of the pictures of the send went; the sentence names each with why."
      }
      Self::SpeakerNotAllowed => {
        "refused: the provider speaks as the room's own agent alone, so it takes no other speaker."
      }
      Self::SpeakerUnnumbered => {
        "refused: the provider has no number in the room for the agent named as speaker."
      }
      Self::Platform => "refused by the platform; the sentence is the provider's own English reason.",
      Self::Gone => {
        "refused: the message is one the platform no longer has, or will never let change again, so the \
         caller lets go of it instead of asking again."
      }
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

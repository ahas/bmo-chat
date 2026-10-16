//! The body of every answer that is not a success.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Why a request could not be answered at all. A refusal is an answer and never takes this shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
  /// The body is not JSON, or not the shape the route takes.
  BadRequest,
  /// The token is missing or wrong.
  Unauthorized,
  /// No route at this method and path.
  NoRoute,
  /// The service is up but not ready to answer this route.
  NotReady,
  /// The service failed while answering.
  Failed,
}

impl ErrorCode {
  /// The HTTP status the code is answered with.
  pub fn status(self) -> u16 {
    match self {
      Self::BadRequest => 400,
      Self::Unauthorized => 401,
      Self::NoRoute => 404,
      Self::NotReady => 409,
      Self::Failed => 500,
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

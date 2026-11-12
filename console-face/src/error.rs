//! Why a console face could not be built, and the refusals its routes answer in the common wire format.

use std::fmt;
use std::path::PathBuf;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Why a console face was not built from a declaration and a bundle folder.
#[derive(Debug)]
pub enum ConsoleFaceError {
  /// A file of the bundle folder, or the folder itself, could not be read.
  Unreadable { path: PathBuf, why: String },
  /// A file that must hold JSON of a given shape does not.
  Malformed { what: String, why: String },
  /// The declaration names a field the face fills itself.
  FilledField(String),
  /// Two of the bundle's files would answer at one path.
  SamePath(String),
  /// The assembled manifest does not keep the contract's schema: each field at fault, with why.
  Contract(Vec<String>),
}

impl fmt::Display for ConsoleFaceError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Unreadable { path, why } => write!(formatter, "{} could not be read: {why}", path.display()),
      Self::Malformed { what, why } => write!(formatter, "{what} {why}"),
      Self::FilledField(field) => {
        write!(formatter, "the declaration names `{field}`, which the console face fills itself")
      }
      Self::SamePath(path) => write!(formatter, "two of the bundle's files would answer at {path}"),
      Self::Contract(faults) => {
        write!(formatter, "the console manifest does not keep the contract: {}", faults.join("; "))
      }
    }
  }
}

impl std::error::Error for ConsoleFaceError {}

/// A refusal one of the face's routes, or the screens' guard, answers.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Refusal {
  Unauthorized,
  Forbidden,
  NotFound,
}

impl IntoResponse for Refusal {
  fn into_response(self) -> Response {
    let (status, code, error) = match self {
      Self::Unauthorized => {
        (StatusCode::UNAUTHORIZED, "unauthorized", "The console credential is missing or wrong.")
      }
      Self::Forbidden => (
        StatusCode::FORBIDDEN,
        "forbidden",
        "The reader's role does not allow the screen, or the owner key it needs is missing.",
      ),
      Self::NotFound => {
        (StatusCode::NOT_FOUND, "not-found", "No file of the console bundle stands at this path.")
      }
    };
    (status, Json(json!({ "code": code, "error": error }))).into_response()
  }
}

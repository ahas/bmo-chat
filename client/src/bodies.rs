//! The bodies of the provider routes that carry no piece of their own: health, the dialect routes and the
//! room routes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dialect::ChatMarkup;
use crate::message::RoomKind;

/// Whether a provider is ready, and where it is not, why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChatHealth {
  pub ready: bool,
  pub reason: Option<String>,
}

impl ChatHealth {
  pub fn ready() -> Self {
    Self {
      ready: true,
      reason: None,
    }
  }

  pub fn not_ready(reason: impl Into<String>) -> Self {
    Self {
      ready: false,
      reason: Some(reason.into()),
    }
  }
}

/// The body of every room route: the room asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomQuery {
  pub chat_id: i64,
}

/// The answer of `room/owns`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomOwns {
  pub owns: bool,
}

/// The answer of `room/identity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomIdentity {
  pub identity: Option<i64>,
}

/// The answer of `room/is-owner`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomKindAnswer {
  pub kind: RoomKind,
}

/// The answer of `room/private-of`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomPerson {
  pub person: Option<i64>,
}

/// The answer of `room/title`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomTitle {
  pub title: Option<String>,
}

/// The answer of `owner-room`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRoom {
  pub chat_id: Option<i64>,
}

/// The body of `dialect/read`: the markdown to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectRead {
  pub text: String,
}

/// The answer of `dialect/read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectReading {
  #[serde(flatten)]
  pub markup: ChatMarkup,
  /// The markdown divided into pieces that each fit one message.
  pub parts: Vec<String>,
  /// The folded markup, or null where the dialect does not fold or there is nothing worth folding.
  pub fold: Option<String>,
}

/// The body of `dialect/check`: markup the dialect built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectCheck {
  pub markup: String,
}

/// The answer of `dialect/check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectChecked {
  /// Null when the markup is sound, else the reason.
  pub fault: Option<String>,
  /// What a reader sees of it.
  pub visible: String,
}

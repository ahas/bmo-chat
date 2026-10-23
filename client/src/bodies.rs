//! The bodies of the provider routes that carry no piece of their own: health, a room, the rooms of the
//! account, the owner, the dialect routes, and the page every list is answered in.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dialect::ChatMarkup;
use crate::message::RoomKind;
use crate::modes::ChatVoice;

/// Whether a provider is ready, where it is not, why, and the provider service's own version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChatHealth {
  pub ready: bool,
  pub reason: Option<String>,
  pub version: String,
}

impl ChatHealth {
  pub fn ready(version: impl Into<String>) -> Self {
    Self {
      ready: true,
      reason: None,
      version: version.into(),
    }
  }

  pub fn not_ready(version: impl Into<String>, reason: impl Into<String>) -> Self {
    Self {
      ready: false,
      reason: Some(reason.into()),
      version: version.into(),
    }
  }
}

/// What a provider knows of one room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRoom {
  pub id: i64,
  /// Whether the room is one of this provider's.
  pub owns: bool,
  /// The sender number the account's own words carry in the room.
  pub identity: Option<i64>,
  /// `owner` when the room is the owner's private chat on this platform, else `group`.
  pub kind: RoomKind,
  /// The sender number of the one person in a one-to-one with the account.
  pub private_to: Option<i64>,
  /// What the platform calls the room, or null where it cannot or will not say.
  pub title: Option<String>,
  /// The voices the room is read by and may be sent in, each one of the declaration's modes; null where the
  /// provider does not say, which stands for every mode the declaration lists. `human-reader` alone makes the
  /// room read-only.
  #[serde(default)]
  pub modes: Option<Vec<ChatVoice>>,
}

/// The query of `GET /rooms`: only the rooms numbered after `after`, and at most `limit` of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct AccountRoomsQuery {
  /// A room number.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u32>,
}

/// The answer of `GET /owner`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOwner {
  /// The room to speak into to reach the owner unprompted, or null.
  pub room_id: Option<i64>,
}

/// The body of `POST /dialect/readings`: the markdown to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectRead {
  pub text: String,
}

/// The answer of `POST /dialect/readings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectReading {
  #[serde(flatten)]
  pub markup: ChatMarkup,
  /// The markdown divided into pieces that each fit one message.
  pub parts: Vec<String>,
  /// The folded markup, or null where the dialect does not fold or there is nothing worth folding.
  pub fold: Option<String>,
}

/// The body of `POST /dialect/checks`: markup the dialect built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectCheck {
  pub markup: String,
}

/// The answer of `POST /dialect/checks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DialectChecked {
  /// Null when the markup is sound, else the reason.
  pub fault: Option<String>,
  /// What a reader sees of it.
  pub visible: String,
}

/// One page of a list: its items, and whether anything stands past the last of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "{T}Page")]
pub struct Page<T> {
  pub items: Vec<T>,
  pub more: bool,
}

impl<T> Page<T> {
  /// A page holding everything there is.
  pub fn whole(items: Vec<T>) -> Self {
    Self { items, more: false }
  }
}

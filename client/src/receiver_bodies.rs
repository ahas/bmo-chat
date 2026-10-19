//! The bodies of the receiver routes that carry no piece of their own: the questions a provider asks the
//! receiver, the answer to a history hand-over, and word of changed profile pictures.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The body of `rooms/listened`: the room asked about, and the provider asking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListenedQuery {
  pub chat_id: i64,
  pub via: String,
}

/// The answer of `rooms/listened`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Listened {
  pub listened: bool,
}

/// The body of `rooms/of`: the provider asking, and only the rooms after the room `after`, at most `limit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomsOfQuery {
  pub via: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u32>,
}

/// A room whose past its provider may read back, with the oldest message the receiver holds of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRoom {
  pub chat_id: i64,
  /// Null where the receiver holds no message of the room.
  pub oldest_message_id: Option<i64>,
  /// In epoch milliseconds; null where the receiver holds no message of the room.
  pub oldest_sent_at: Option<i64>,
}

/// The answer of `rooms/of`, in ascending order of room number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomsOf {
  pub rooms: Vec<HistoryRoom>,
}

/// The answer of `history`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryKept {
  /// True where the receiver archived the message, false where it already held it.
  pub kept: bool,
}

/// The body of `avatars/updated`: the rooms and people whose kept profile picture changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AvatarsUpdated {
  pub via: String,
  pub rooms: Vec<i64>,
  pub people: Vec<i64>,
}

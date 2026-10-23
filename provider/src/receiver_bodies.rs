//! The bodies of the receiving-side routes that carry no piece of their own: a room as the receiving side
//! holds it, the queries a provider asks with, a message as held after a hand-over, and word of changed
//! profile pictures.

use serde::{Deserialize, Serialize};

use crate::message::HandedMessage;

/// One room as the receiving side holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverRoom {
  pub id: i64,
  /// Whether the receiving side listens to the room.
  pub listened: bool,
  /// Whether the receiving side asked for the room to be read through the asking provider's account; false
  /// where it does not say.
  #[serde(default)]
  pub read: bool,
  /// Null where the receiving side holds no message of the room.
  pub oldest_message_id: Option<i64>,
  /// In epoch milliseconds; null where the receiving side holds no message of the room.
  pub oldest_sent_at: Option<i64>,
}

/// The query of `GET /rooms/{roomId}`: the provider asking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomQuery {
  pub via: String,
}

/// The query of `GET /rooms`: the provider asking, and only the rooms after the room `after`, at most
/// `limit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomsQuery {
  pub via: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u32>,
}

/// A message as the receiving side holds it after a hand-over, and whether it held it before.
#[derive(Debug, Clone, PartialEq)]
pub struct HeldMessage {
  /// True where the receiving side did not hold the message before, so the hand-over is answered 201.
  pub created: bool,
  pub message: HandedMessage,
}

/// The body of `POST /avatar-changes`: the rooms and people whose kept profile picture changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarChange {
  pub via: String,
  pub room_ids: Vec<i64>,
  pub person_ids: Vec<i64>,
}

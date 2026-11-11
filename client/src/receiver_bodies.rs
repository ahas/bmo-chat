//! The bodies of the receiving-side routes that carry no piece of their own: a room as the receiving side
//! holds it, the queries a provider asks with, a message as held after a hand-over, and word of changed
//! profile pictures.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::message::HandedMessage;

/// One room as the receiving side holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RoomQuery {
  pub via: String,
}

/// The query of `GET /rooms`: the provider asking, and only the rooms after the room `after`, at most
/// `limit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AvatarChange {
  pub via: String,
  pub room_ids: Vec<i64>,
  pub person_ids: Vec<i64>,
}

/// The body of `POST /reaction-changes`: one message's reactions as the platform shows them now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReactionChange {
  pub via: String,
  pub room_id: i64,
  pub message_id: i64,
  /// Each person listed with every emoji they have on the message now.
  pub reactions: Vec<PersonReactions>,
  /// Whether the list is every reaction the message carries, so a person not listed has none.
  #[serde(default)]
  pub whole: bool,
}

/// One person's reactions on a message: their number, or none for the account itself, and their emoji, empty
/// where they took theirs off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonReactions {
  pub person_id: Option<i64>,
  pub emojis: Vec<String>,
}

/// The body of `POST /deletions`: messages of one room deleted on the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Deletion {
  pub via: String,
  pub room_id: i64,
  pub message_ids: Vec<i64>,
}

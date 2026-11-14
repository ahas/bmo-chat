//! The members of a room, as a provider lists them.

use serde::{Deserialize, Serialize};

use crate::refusal::ChatRefusal;

/// One person in a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMember {
  /// The person, as a number of the provider's `people` space, the sender number the room's messages carry;
  /// null where the provider has given the person none.
  pub person: Option<i64>,
  /// What the room calls the person.
  pub name: String,
  /// The person's status line, or null where the platform gave none.
  pub status_message: Option<String>,
}

/// The answer of `GET /rooms/{roomId}/members`: the room's members, whole.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChatMembers {
  pub members: Vec<ChatMember>,
}

/// What a provider asked for a room's members came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatMembersResult {
  /// The room's members, in the order the provider keeps them.
  Listed(Vec<ChatMember>),
  /// The provider holds no members of the room, with its own English reason: it knows nothing of the room, or
  /// has not read its members yet.
  Unheld { reason: String },
  /// What the platform will not do.
  Refused(ChatRefusal),
}

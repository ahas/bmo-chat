//! The members of a room, as a provider lists them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One person in a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct ChatMembers {
  pub members: Vec<ChatMember>,
}

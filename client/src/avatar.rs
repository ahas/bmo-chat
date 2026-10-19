//! Profile pictures: the pictures a provider that declares `avatars` keeps of rooms and people.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whose picture one is: a room's or a person's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatAvatarKind {
  Room,
  Person,
}

impl fmt::Display for ChatAvatarKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::Room => "room",
      Self::Person => "person",
    })
  }
}

/// One picture a provider keeps: its subject, and when the picture kept now was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatAvatar {
  pub kind: ChatAvatarKind,
  /// The room's or the person's number.
  pub id: i64,
  /// When the picture was taken, in epoch milliseconds.
  pub taken_at: i64,
}

/// A picture itself. On the wire it is the answer's bytes, with `mime` as its `Content-Type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPicture {
  pub mime: String,
  pub bytes: Vec<u8>,
}

/// The query of `GET /avatars`: only the pictures taken after `after`, and at most `limit` of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct AvatarsQuery {
  /// A time, in epoch milliseconds.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u32>,
}

/// The answer of `GET /avatars`: oldest taken first, never ending between two pictures taken at the same
/// moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AvatarList {
  pub avatars: Vec<ChatAvatar>,
}

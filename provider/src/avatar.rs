//! Profile pictures: the pictures a provider that declares `avatars` keeps of rooms and people.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::bodies::Page;

/// Whose picture one is: a room's or a person's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AvatarsQuery {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u32>,
}

impl AvatarsQuery {
  /// The page of `kept` this query asks for: oldest taken first, only those taken after the cursor, and at
  /// most `limit`, except that a page never ends between two pictures taken at the same moment. `more` says
  /// whether pictures stand past the page's last.
  pub fn page(&self, mut kept: Vec<ChatAvatar>) -> Page<ChatAvatar> {
    kept.sort_by_key(|avatar| (avatar.taken_at, avatar.kind, avatar.id));
    if let Some(after) = self.after {
      kept.retain(|avatar| avatar.taken_at > after);
    }
    let mut more = false;
    if let Some(limit) = self.limit.and_then(|limit| usize::try_from(limit).ok())
      && limit < kept.len()
    {
      let end = match limit.checked_sub(1) {
        Some(last) => {
          let moment = kept[last].taken_at;
          last + kept[last..].iter().take_while(|avatar| avatar.taken_at == moment).count()
        }
        None => 0,
      };
      more = end < kept.len();
      kept.truncate(end);
    }
    Page { items: kept, more }
  }
}

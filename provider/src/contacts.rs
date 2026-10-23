//! The contacts of a provider's account, and a conversation started with them.

use serde::{Deserialize, Serialize};

use crate::bodies::{Page, ProviderRoom};
use crate::capability::ChatCapability;
use crate::refusal::ChatRefusal;

/// What kind of account a contact is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatContactKind {
  /// A person's account.
  Person,
  /// A platform's bot account.
  Bot,
}

/// One contact of the provider's account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContact {
  /// The contact, as a number of the provider's `people` space.
  pub id: i64,
  /// What the account calls the contact.
  pub name: String,
  pub kind: ChatContactKind,
  /// The one-to-one the account already has with the contact, or null where it has none.
  pub room_id: Option<i64>,
}

/// The query of `GET /contacts`: only the contacts numbered after `after`, and at most `limit` of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContactsQuery {
  /// A contact number.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub after: Option<i64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub limit: Option<u32>,
}

impl ContactsQuery {
  /// The page of `listed` this query asks for: in ascending order of contact number, each contact once, only
  /// those after the cursor, and at most `limit`. `more` says whether contacts stand past the page's last.
  pub fn page(&self, listed: Vec<ChatContact>) -> Page<ChatContact> {
    Page::numbered(listed, |contact| contact.id, self.after, self.limit)
  }
}

/// The body of `POST /rooms`: a conversation to start, with one contact or with several under a title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ChatStart {
  /// A one-to-one with one contact of the account.
  #[serde(rename_all = "camelCase")]
  OneToOne { contact_id: i64 },
  /// A room of several contacts, under a title.
  #[serde(rename_all = "camelCase")]
  Group { title: String, contact_ids: Vec<i64> },
}

impl ChatStart {
  /// The capability this shape of start needs.
  pub fn capability(&self) -> ChatCapability {
    match self {
      Self::OneToOne { .. } => ChatCapability::StartChat,
      Self::Group { .. } => ChatCapability::StartGroup,
    }
  }

  /// Why the start is no start of its shape, where it is not: a group naming no contact, naming one twice, or
  /// carrying a blank title.
  pub fn fault(&self) -> Option<String> {
    let Self::Group { title, contact_ids } = self else {
      return None;
    };
    if title.trim().is_empty() {
      return Some("A group's title is blank.".to_owned());
    }
    if contact_ids.is_empty() {
      return Some("A group names no contact.".to_owned());
    }
    let mut seen = contact_ids.clone();
    seen.sort_unstable();
    seen
      .windows(2)
      .find(|pair| pair[0] == pair[1])
      .map(|pair| format!("A group names contact {} twice.", pair[0]))
  }
}

/// What a start came to. Only the room is an answer: no contact and a refusal cross the wire in the error shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatStartResult {
  /// The room started, or the one-to-one the account already had with the contact.
  Started(ProviderRoom),
  /// A contact the start names is none the provider knows.
  NoContact {
    contact_id: i64,
  },
  Refused(ChatRefusal),
}

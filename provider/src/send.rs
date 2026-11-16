//! A send, one kind at a time, and what it came to.

use serde::{Deserialize, Serialize};

use crate::capability::ChatVerb;
use crate::file_body::FileBody;
use crate::message::FileItem;
use crate::modes::ChatVoice;
use crate::refusal::{ChatRefusal, MissedPicture};

/// One picture of an album: an address the provider can read, or the file itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AlbumItem {
  /// A web address or a file path the provider can read.
  Address {
    address: String,
  },
  File(FileItem),
}

/// A sticker to send: an emoji the provider picks a sticker for, or one item of the platform's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatSticker {
  Emoji { emoji: String },
  Item { item: StickerItem },
}

/// One sticker item of the platform's own, which the provider maps onto its platform's wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerItem {
  /// The item's own key on the platform.
  pub id: String,
  /// The key of the pack, set or product the item belongs to.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub pack: Option<String>,
  /// The item's title, or the title of its pack.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  /// The address the item's picture stands at.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub image_url: Option<String>,
  /// The address of the item's small picture.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thumbnail_url: Option<String>,
}

/// The kind of a send with its own fields, which cross as `kind` and `fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
  tag = "kind",
  content = "fields",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
pub enum ChatSend {
  Text {
    text: String,
    /// A notice that must not buzz a phone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    silent: bool,
    /// Fold where the dialect can; whether to fold is the sender's decision.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    fold: bool,
  },
  Photo {
    image_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
  },
  Album {
    items: Vec<AlbumItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
  },
  Document {
    bytes: FileBody,
    file_name: String,
  },
  Audio {
    audio: FileBody,
    file_name: String,
  },
  Voice {
    audio: FileBody,
    /// What the line said.
    said: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    /// What was actually read out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transcript: Option<String>,
  },
  Sticker(ChatSticker),
  /// An empty emoji takes the account's own reaction off.
  Reaction {
    message_id: i64,
    emoji: String,
  },
  Pin {
    message_id: i64,
    on: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    silent: bool,
  },
  Typing {
    on: bool,
  },
  Edit {
    message_id: i64,
    text: String,
  },
  Delete {
    message_id: i64,
  },
}

impl ChatSend {
  /// The kind of the send, as the capability it needs names it.
  pub fn kind(&self) -> ChatVerb {
    match self {
      Self::Text { .. } => ChatVerb::Text,
      Self::Photo { .. } => ChatVerb::Photo,
      Self::Album { .. } => ChatVerb::Album,
      Self::Document { .. } => ChatVerb::Document,
      Self::Audio { .. } => ChatVerb::Audio,
      Self::Voice { .. } => ChatVerb::Voice,
      Self::Sticker(_) => ChatVerb::Sticker,
      Self::Reaction { .. } => ChatVerb::Reaction,
      Self::Pin { .. } => ChatVerb::Pin,
      Self::Typing { .. } => ChatVerb::Typing,
      Self::Edit { .. } => ChatVerb::Edit,
      Self::Delete { .. } => ChatVerb::Delete,
    }
  }
}

/// A send into one room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendRequest {
  pub room_id: i64,
  #[serde(flatten)]
  pub send: ChatSend,
  /// The voice to speak with; unnamed, the provider speaks with the voice it answers the room by.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub mode: Option<ChatVoice>,
  /// Who speaks, where it is not the one the room is answered by, as the caller's opaque string. Taken only
  /// by a provider that declares `speakers`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub speaker: Option<String>,
  /// Returned on the inbound message that is the platform's echo of this send.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tag: Option<String>,
  /// The words the answers carry before them in the `human-avatar` voice; ignored in any other voice.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prefix: Option<String>,
  /// The message the send answers. Ignored by a provider that does not declare `replyTo`, and by every kind
  /// that answers no message.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reply_to_message_id: Option<i64>,
  /// The account the send goes out in; unnamed, the provider's own.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub account_id: Option<i64>,
}

impl ChatSendRequest {
  /// A send in the room's own voice and the provider's own account, with no speaker, no tag, no prefix and
  /// answering no message.
  pub fn new(room_id: i64, send: ChatSend) -> Self {
    Self {
      room_id,
      send,
      mode: None,
      speaker: None,
      tag: None,
      prefix: None,
      reply_to_message_id: None,
      account_id: None,
    }
  }
}

/// A send the provider performed: the answer of `POST /sends`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSent {
  /// Null where the platform hands none back.
  pub message_id: Option<i64>,
  /// The pictures of an album that did not go; empty where everything went.
  #[serde(default)]
  pub missed: Vec<MissedPicture>,
}

/// What a send came to. A refusal crosses the wire in the error shape, so only `Sent` is an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatSendResult {
  Sent(ChatSent),
  Refused(ChatRefusal),
}

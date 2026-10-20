//! A send, one kind at a time, and what it came to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::capability::ChatVerb;
use crate::message::FileItem;
use crate::missed::MissedPicture;
use crate::modes::ChatVoice;

/// One picture of an album: an address the provider can read, or the file itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AlbumItem {
  /// A web address or a file path the provider can read.
  Address {
    address: String,
  },
  File(FileItem),
}

/// The kind of a send with its own fields, which cross as `kind` and `fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    #[serde(with = "crate::bytes")]
    #[schemars(with = "String", extend("contentEncoding" = "base64"))]
    bytes: Vec<u8>,
    file_name: String,
  },
  Audio {
    #[serde(with = "crate::bytes")]
    #[schemars(with = "String", extend("contentEncoding" = "base64"))]
    audio: Vec<u8>,
    file_name: String,
  },
  Voice {
    #[serde(with = "crate::bytes")]
    #[schemars(with = "String", extend("contentEncoding" = "base64"))]
    audio: Vec<u8>,
    /// What the line said.
    said: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    /// What was actually read out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transcript: Option<String>,
  },
  Sticker {
    emoji: String,
  },
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
      Self::Sticker { .. } => ChatVerb::Sticker,
      Self::Reaction { .. } => ChatVerb::Reaction,
      Self::Pin { .. } => ChatVerb::Pin,
      Self::Typing { .. } => ChatVerb::Typing,
      Self::Edit { .. } => ChatVerb::Edit,
      Self::Delete { .. } => ChatVerb::Delete,
    }
  }
}

/// A send into one room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
}

impl ChatSendRequest {
  /// A send in the room's own voice, with no speaker, no tag, no prefix and answering no message.
  pub fn new(room_id: i64, send: ChatSend) -> Self {
    Self {
      room_id,
      send,
      mode: None,
      speaker: None,
      tag: None,
      prefix: None,
      reply_to_message_id: None,
    }
  }
}

/// A send the provider performed: the answer of `POST /sends`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatSent {
  /// Null where the platform hands none back.
  pub message_id: Option<i64>,
  /// The pictures of an album that did not go; empty where everything went.
  #[serde(default)]
  pub missed: Vec<MissedPicture>,
}

//! A send, one verb at a time, and what it came to.

use serde::{Deserialize, Serialize};

use crate::capability::ChatVerb;
use crate::message::FileItem;
use crate::modes::ChatVoice;
use crate::refusal::{ChatRefusal, MissedPicture};

/// An agent other than the one the room is answered by, by its registry id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatSpeaker {
  pub agent: String,
}

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

/// The verb of a send with its own fields. `reply_to` is a modifier a provider that does not declare
/// `replyTo` ignores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ChatSend {
  Text {
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<i64>,
    /// A notice that must not buzz a phone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    silent: bool,
    /// Fold where the dialect can; whether to fold is decided upstream.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    fold: bool,
  },
  Photo {
    image_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<i64>,
  },
  Album {
    items: Vec<AlbumItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<i64>,
  },
  Document {
    #[serde(with = "crate::bytes")]
    bytes: Vec<u8>,
    file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<i64>,
  },
  Audio {
    #[serde(with = "crate::bytes")]
    audio: Vec<u8>,
    file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<i64>,
  },
  Voice {
    #[serde(with = "crate::bytes")]
    audio: Vec<u8>,
    /// What the line said.
    said: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    /// What was actually read out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transcript: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<i64>,
  },
  Sticker {
    emoji: String,
  },
  /// An empty emoji takes the agent's own reaction off.
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
  pub fn verb(&self) -> ChatVerb {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendRequest {
  pub chat_id: i64,
  #[serde(flatten)]
  pub send: ChatSend,
  /// The voice to speak with; unnamed, the provider speaks with the voice it answers the room by.
  #[serde(rename = "as", default, skip_serializing_if = "Option::is_none")]
  pub voice: Option<ChatVoice>,
  /// Taken only by a provider that declares `speakers`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub speaker: Option<ChatSpeaker>,
  /// Returned on the inbound message that is the platform's echo of this send.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tag: Option<String>,
  /// The words the agent's lines carry before them in the `human-avatar` voice; ignored in any other voice.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prefix: Option<String>,
}

impl ChatSendRequest {
  /// A send in the room's own voice, with no speaker, no tag and no prefix.
  pub fn new(chat_id: i64, send: ChatSend) -> Self {
    Self {
      chat_id,
      send,
      voice: None,
      speaker: None,
      tag: None,
      prefix: None,
    }
  }
}

/// A send the provider performed: the answer of `send`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSent {
  /// Null where the platform hands none back.
  pub message_id: Option<i64>,
  /// The pictures of an album that did not go; empty where everything went, and then left out.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub missed: Vec<MissedPicture>,
}

/// What a send came to. A refusal crosses the wire in the error shape, so only `Sent` is an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatSendResult {
  Sent(ChatSent),
  Refused(ChatRefusal),
}

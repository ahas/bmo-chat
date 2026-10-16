//! A send, one verb at a time, and what it came to.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
}

impl ChatSendRequest {
  /// A send in the room's own voice, with no speaker and no tag.
  pub fn new(chat_id: i64, send: ChatSend) -> Self {
    Self {
      chat_id,
      send,
      voice: None,
      speaker: None,
      tag: None,
    }
  }
}

/// What a send came to. On the wire the two are told apart by `sent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatSendResult {
  Sent {
    /// Null where the platform hands none back.
    message_id: Option<i64>,
    /// The pictures of an album that did not go; empty where everything went.
    missed: Vec<MissedPicture>,
  },
  Refused {
    refusal: ChatRefusal,
    /// The refusal is about a message the platform no longer has, or will never let change again.
    gone: bool,
  },
}

#[derive(Serialize)]
#[serde(untagged, rename_all_fields = "camelCase")]
enum ResultOut<'a> {
  Sent {
    sent: bool,
    message_id: Option<i64>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    missed: &'a [MissedPicture],
  },
  Refused {
    sent: bool,
    refusal: &'a ChatRefusal,
    gone: bool,
  },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultIn {
  sent: bool,
  #[serde(default)]
  message_id: Option<i64>,
  #[serde(default)]
  missed: Vec<MissedPicture>,
  #[serde(default)]
  refusal: Option<ChatRefusal>,
  #[serde(default)]
  gone: bool,
}

impl Serialize for ChatSendResult {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    match self {
      Self::Sent { message_id, missed } => ResultOut::Sent {
        sent: true,
        message_id: *message_id,
        missed,
      },
      Self::Refused { refusal, gone } => ResultOut::Refused {
        sent: false,
        refusal,
        gone: *gone,
      },
    }
    .serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for ChatSendResult {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    let fields = ResultIn::deserialize(deserializer)?;
    if fields.sent {
      return Ok(Self::Sent {
        message_id: fields.message_id,
        missed: fields.missed,
      });
    }
    let refusal = fields
      .refusal
      .ok_or_else(|| serde::de::Error::custom("a send result that was not sent carries a refusal"))?;
    Ok(Self::Refused {
      refusal,
      gone: fields.gone,
    })
  }
}

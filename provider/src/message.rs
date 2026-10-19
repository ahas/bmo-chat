//! A message as it crosses from a provider to the receiver, and what crosses beside it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::modes::ChatVoice;

/// Which kind of room one chat is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomKind {
  /// The owner's private chat on this platform.
  Owner,
  /// Any other room.
  Group,
}

/// A message a room said. Nothing platform-shaped crosses, and no file path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
  pub chat_id: i64,
  pub message_id: i64,
  pub sender_id: Option<i64>,
  /// The name the platform shows for who said it.
  pub sender_name: String,
  /// When, in epoch milliseconds.
  pub sent_at: i64,
  /// Its words, or a placeholder the provider wrote for what was sent.
  pub text: String,
  pub reply_to_message_id: Option<i64>,
  /// The key of the provider that took it in.
  pub via: String,
  /// The number of the first message of the send of several this one came in.
  pub album: Option<i64>,
  /// The mode that delivered it.
  pub mode: ChatVoice,
}

/// What the platform said about a message beyond the message itself.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundChatContext {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub room_title: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sender_username: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reply_to_sender_id: Option<i64>,
  /// The words the person actually typed, where `text` carries a placeholder.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub spoken_text: Option<String>,
  /// The platform's own payload, opaque; nothing outside the provider reads into it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub metadata: Option<Value>,
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub carries_file: bool,
  /// The provider is still gathering the message's file and hands it over later as `gathered`.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub gathering: bool,
  /// The opaque tag of the send this message is the platform's echo of.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tag: Option<String>,
}

/// What the provider already knows of the room a hand-over came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomFacts {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<RoomKind>,
  /// The sender number the agent's own words carry in the room.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub identity: Option<i64>,
  /// The sender number of the one person in a one-to-one with the agent.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub private_to: Option<i64>,
}

/// A file as it crosses the wire with no identifier of its own, as a send carries it inline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileItem {
  pub name: String,
  pub mime: String,
  #[serde(with = "crate::bytes")]
  pub bytes: Vec<u8>,
}

/// What a file a message carried is, as the platform showed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChatFileKind {
  Photo,
  Video,
  /// A short video the platform shows in a circle.
  RoundVideo,
  /// A short looping video without sound, or an animated image.
  Animation,
  /// A sound file.
  Audio,
  /// A recording made in the room to be heard as speech.
  VoiceNote,
  Sticker,
  /// Any other file.
  Document,
}

/// A file a message carried, as a provider hands it over or fetches it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageFile {
  /// The provider's identifier of the file among its message's files: the same each time the same file
  /// crosses, and never shared by two files of one message.
  pub id: String,
  pub name: String,
  pub mime: String,
  #[serde(with = "crate::bytes")]
  pub bytes: Vec<u8>,
  /// What the file is, as far as the platform tells; left out where the provider cannot tell.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<ChatFileKind>,
}

/// The body of the `message`, `attachment`, `edited` and `history` hand-overs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatHandover {
  pub message: ChatMessage,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context: Option<InboundChatContext>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub room: Option<RoomFacts>,
  /// The files an `attachment` carries when they are already gathered.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub files: Vec<MessageFile>,
}

/// What came of gathering a message's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum GatheredOutcome {
  Kept {
    files: Vec<MessageFile>,
  },
  /// The reason is an English sentence saying why the file could not be kept.
  NotKept {
    reason: String,
  },
}

/// The body of the `gathered` hand-over: the file a message's `gathering` promised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGathered {
  pub chat_id: i64,
  pub message_id: i64,
  pub via: String,
  #[serde(flatten)]
  pub outcome: GatheredOutcome,
}

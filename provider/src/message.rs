//! A message as it crosses from a provider to the receiving side, and what crosses with it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::file_body::FileBody;
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

/// Where a handed-over message comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageOrigin {
  /// A message a room said.
  Live,
  /// A message already handed over, changed: its words, its sender, or both.
  Edit,
  /// A past message read back from a room, kept where the receiving side lacks it and waking nobody.
  History,
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
  /// The sender number the account's own words carry in the room.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub identity: Option<i64>,
  /// The sender number of the one person in a one-to-one with the account.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub private_to: Option<i64>,
}

/// A file as it crosses the wire with no identifier of its own, as a send carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileItem {
  pub name: String,
  pub mime: String,
  pub bytes: FileBody,
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
  /// Left out, and read as withheld, where an answer names the file without its bytes.
  #[serde(default = "FileBody::withheld", skip_serializing_if = "FileBody::is_withheld")]
  pub bytes: FileBody,
  /// What the file is, as far as the platform tells; left out where the provider cannot tell.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<ChatFileKind>,
}

/// A message as a provider hands it over, under the room and the number the route's path names. Nothing
/// platform-shaped crosses, and no file path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandedMessage {
  pub origin: MessageOrigin,
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
  #[serde(default)]
  pub context: InboundChatContext,
  #[serde(default)]
  pub room: RoomFacts,
  /// The files the message carries, where they are already gathered.
  #[serde(default)]
  pub files: Vec<MessageFile>,
  /// The provider is still gathering the message's files and sends them later in a `GatheredPatch`.
  #[serde(default)]
  pub gathering: bool,
  /// Why the files gathering promised could not be kept.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub gather_failure: Option<String>,
}

/// The files a message's gathering promised, or why they could not be kept: a merge patch of the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatheredPatch {
  pub files: Vec<MessageFile>,
  /// An English sentence saying why the files could not be kept.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub gather_failure: Option<String>,
}

impl GatheredPatch {
  /// The files, kept.
  pub fn kept(files: Vec<MessageFile>) -> Self {
    Self {
      files,
      gather_failure: None,
    }
  }

  /// No files, for this reason.
  pub fn not_kept(reason: impl Into<String>) -> Self {
    Self {
      files: Vec::new(),
      gather_failure: Some(reason.into()),
    }
  }
}

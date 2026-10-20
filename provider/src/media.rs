//! The pictures and files a message carried, fetched from the platform again.

use serde::{Deserialize, Serialize};

use crate::message::MessageFile;
use crate::refusal::ChatRefusal;

/// The message whose pictures and files are wanted again: the route's path, and its query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatMediaRequest {
  pub room_id: i64,
  pub message_id: i64,
  /// When the message was sent, in epoch milliseconds, for a platform that finds an old message by its time.
  pub sent_at: Option<i64>,
}

/// The query of `GET /rooms/{roomId}/messages/{messageId}/files`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesQuery {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sent_at: Option<i64>,
}

/// What a fetch came to. Only the files are an answer: `unavailable` and a refusal cross the wire in the error
/// shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatMediaResult {
  /// The message's files, in the order it carried them.
  Fetched(Vec<MessageFile>),
  /// The platform would not give them; the reason is an English sentence.
  Unavailable {
    reason: String,
  },
  Refused(ChatRefusal),
}

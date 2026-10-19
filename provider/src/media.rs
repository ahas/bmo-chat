//! A picture or a file a message carried, fetched from the platform again.

use serde::{Deserialize, Serialize};

use crate::message::MessageFile;
use crate::refusal::ChatRefusal;

/// The message whose picture or file is wanted again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMediaRequest {
  pub chat_id: i64,
  pub message_id: i64,
  /// When the message was sent, in epoch milliseconds, for a platform that finds an old message by its time.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sent_at: Option<i64>,
}

/// What a fetch came to. Only the file is an answer: `unavailable` and a refusal cross the wire in the error
/// shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatMediaResult {
  Fetched(MessageFile),
  /// The platform would not give it; the reason is an English sentence.
  Unavailable {
    reason: String,
  },
  Refused(ChatRefusal),
}

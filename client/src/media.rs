//! A picture or a file a message carried, fetched from the platform again.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The message whose picture or file is wanted again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatMediaRequest {
  pub chat_id: i64,
  pub message_id: i64,
  /// When the message was sent, in epoch milliseconds, for a platform that finds an old message by its time.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sent_at: Option<i64>,
}

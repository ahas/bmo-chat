//! The pictures and files a message carried, fetched from the platform again.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The message whose pictures and files are wanted again: the route's path, and its query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatMediaRequest {
  pub room_id: i64,
  pub message_id: i64,
  /// When the message was sent, in epoch milliseconds, for a platform that finds an old message by its time.
  pub sent_at: Option<i64>,
}

/// The query of `GET /rooms/{roomId}/messages/{messageId}/files`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilesQuery {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sent_at: Option<i64>,
}

//! Whose numbers a provider's rooms, people and messages carry.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whose numbers one kind of thing carries on a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatNumberSpace {
  /// The platform's own numbers, used as they are.
  Native,
  /// Numbers the provider gave out because the platform's cannot be used.
  Assigned,
  /// None of its own: the provider speaks into rooms and people another provider numbers.
  None,
}

/// One number space for each of rooms, people and messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ChatNumbering {
  pub rooms: ChatNumberSpace,
  pub people: ChatNumberSpace,
  pub messages: ChatNumberSpace,
}

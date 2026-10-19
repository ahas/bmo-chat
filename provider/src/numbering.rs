//! Whose numbers a provider's rooms, people and messages carry, and the ranges it assigns them in.

use serde::{Deserialize, Serialize};

/// Whose numbers one kind of thing carries on a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatNumberSpace {
  /// The platform's own numbers, used as they are.
  Native,
  /// Numbers the provider gave out because the platform's cannot be used.
  Assigned,
  /// None of its own: the provider speaks into rooms and people another provider numbers.
  None,
}

/// The numbers a provider assigns in one space: from `first` to `last`, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatNumberRange {
  pub first: i64,
  pub last: i64,
}

/// The range of each space a provider assigns numbers in; null for a space it does not assign. Messages carry
/// none, since a message is numbered within its room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ChatNumberRanges {
  pub rooms: Option<ChatNumberRange>,
  pub people: Option<ChatNumberRange>,
}

/// One number space for each of rooms, people and messages, and the ranges of the spaces assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatNumbering {
  pub rooms: ChatNumberSpace,
  pub people: ChatNumberSpace,
  pub messages: ChatNumberSpace,
  pub ranges: ChatNumberRanges,
}

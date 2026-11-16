//! Typing and presence: who is writing in a room now, and who is reachable now, as a provider tells them to the
//! receiving side (`docs/contract.md`, "Typing and presence").

use serde::{Deserialize, Serialize};

/// How long a typing mark stands, in milliseconds, where its provider names no life of its own.
pub const TYPING_MARK_LASTS_MS: u64 = 6_000;

/// The body of `POST /typing-marks`: a person writing in a room now, or no longer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypingMark {
  pub via: String,
  pub room_id: i64,
  /// A number of the provider's `people` space.
  pub person_id: i64,
  pub on: bool,
  /// How long the mark stands unless told again, in milliseconds; null takes the six-second default.
  pub lasts_ms: Option<u64>,
}

/// Whether a person is reachable now, as the provider maps its platform's own states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChatPresenceState {
  /// At the platform now, busy or not.
  Online,
  /// Signed in but idle, or stepped away.
  Away,
  /// Not at the platform, or hiding being there.
  Offline,
}

/// A person's presence: their state, and the moment the platform says they were last at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPresence {
  pub state: ChatPresenceState,
  /// Epoch milliseconds; null where the platform names no moment.
  pub last_seen_at: Option<i64>,
}

/// One person's presence, by their number in the provider's `people` space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonPresence {
  pub person_id: i64,
  #[serde(flatten)]
  pub presence: ChatPresence,
}

/// The body of `POST /presence-changes`: whether people are reachable now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceChange {
  pub via: String,
  pub people: Vec<PersonPresence>,
  /// Whether the list is every person the provider tells presence of, so a person not listed is unknown.
  #[serde(default)]
  pub whole: bool,
}

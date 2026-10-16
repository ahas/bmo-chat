//! Read modes: whose account speaks, what it receives, and where the mode stands.

use serde::{Deserialize, Serialize};

/// Whose account speaks on a provider. One axis of a read mode; `ChatReach` is the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChatVoice {
  /// The platform's official bot interface.
  BotAccount,
  /// A person's account, read only; a send in the owner's own words goes out in it.
  HumanReader,
  /// A person's account, with the agent answering in it behind a prefix the caller supplies.
  HumanAvatar,
  /// An account that belongs to the agent itself.
  HumanAsAgent,
  /// No platform: the owner typing into the machine.
  Direct,
}

/// What a mode's account receives. A property of the mode on that platform, not of the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChatReach {
  AllMessages,
  MentionsOnly,
}

/// Where a mode stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatModeStanding {
  /// Running.
  Active,
  /// Supported, waiting on something it is missing.
  Inactive,
  /// Refused by the platform's own terms; no configuration makes it active.
  Unsupported,
}

/// One way a provider's rooms are read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReadMode {
  pub voice: ChatVoice,
  pub reach: ChatReach,
  pub standing: ChatModeStanding,
  /// What is missing for an inactive mode, or the platform's own words for an unsupported one; null for
  /// an active one.
  pub reason: Option<String>,
  /// Whether an answer can be pinned to a message this mode delivered.
  pub can_reply_to: bool,
}

//! The provider's self-declaration.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::capability::ChatCapability;
use crate::dialect::ChatDialectDeclaration;
use crate::modes::ChatReadMode;
use crate::numbering::ChatNumbering;

/// What a provider declares of itself. The base holds no record of a provider anywhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatDeclaration {
  /// Short, ASCII and stable; written onto every message the provider hands over.
  pub key: String,
  /// What the platform is called where a person reads it.
  pub label: String,
  /// Null when the provider can run, else the reason it cannot.
  pub configured: Option<String>,
  /// Every mode it supports, each with its standing.
  pub modes: Vec<ChatReadMode>,
  /// Its capabilities.
  pub can: BTreeSet<ChatCapability>,
  pub dialect: ChatDialectDeclaration,
  /// What a person reading this provider's rooms sees beyond what the provider sends; null where it shows
  /// only what it carries.
  pub shows: Option<String>,
  pub numbering: ChatNumbering,
  /// Whether a send may name a speaker other than the one the room is answered by.
  pub speakers: bool,
  /// The app registration a person's own account needs before it reads on this provider, where it needs one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub registration: Option<ChatRegistration>,
  /// How a person's node is asked whether it holds what the provider reads a person's account through there, where
  /// the provider reads one through the node at all.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub node_probe: Option<ChatNodeProbe>,
}

/// The commands a person's node is asked with whether it holds what the provider reads a person's account through
/// there, one for each shell a node takes a command in, each exiting 0 where it does; `missing` says what the person
/// installs or signs into where it does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatNodeProbe {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub powershell: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub linux: Option<String>,
  pub missing: String,
}

/// An app registration a provider needs of a person: where it is made, and the values it gives, which the person
/// keeps among their secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRegistration {
  pub url: String,
  pub fields: Vec<String>,
}

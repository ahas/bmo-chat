//! What a provider's dialect declares, and what its reading of markdown gives back.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The agent's markdown once a dialect has read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChatMarkup {
  /// What goes out.
  pub markup: String,
  /// The same words with the marks stripped.
  pub plain: String,
  /// Whether the markup says anything the plain text does not.
  pub formatted: bool,
}

/// What one message of a dialect holds. Null where the platform sets no limit of that kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatDialectLimits {
  /// The longest message, in the units the platform counts by.
  pub text: Option<u64>,
  /// The longest caption under a picture or a file.
  pub caption: Option<u64>,
  /// The most pictures one album carries.
  pub album_photos: Option<u64>,
  /// The largest file, in bytes.
  pub file_bytes: Option<u64>,
}

/// A dialect as a provider's declaration states it. Its readings are asked of the provider over the
/// dialect routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChatDialectDeclaration {
  pub name: String,
  pub limits: ChatDialectLimits,
  /// The sentence telling a writer in such a room what its marks become.
  pub writing: String,
  /// Whether this dialect can fold a long answer.
  pub foldable: bool,
}

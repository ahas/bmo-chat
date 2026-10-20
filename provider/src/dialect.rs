//! What a dialect declares and what its reading of markdown gives back.

use serde::{Deserialize, Serialize};

/// Markdown once a dialect has read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMarkup {
  /// What goes out.
  pub markup: String,
  /// The same words with the marks stripped.
  pub plain: String,
  /// Whether the markup says anything the plain text does not.
  pub formatted: bool,
}

/// What one message of a dialect holds. Null where the platform sets no limit of that kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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

/// A dialect as a provider's declaration states it: the parts of a dialect that are facts rather than
/// readings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatDialectDeclaration {
  pub name: String,
  pub limits: ChatDialectLimits,
  /// The sentence telling a writer in such a room what its marks become.
  pub writing: String,
  /// Whether this dialect can fold a long answer.
  pub foldable: bool,
}

/// What markdown written for a provider's rooms becomes on them. A dialect names no provider, so one dialect
/// may serve several.
pub trait ChatDialect: Send + Sync {
  fn name(&self) -> &str;

  fn limits(&self) -> ChatDialectLimits;

  /// The sentence telling a writer in such a room what its marks become.
  fn writing(&self) -> &str;

  /// Whether this dialect can fold a long answer. A dialect that cannot is never asked for a fold.
  fn foldable(&self) -> bool;

  /// The markdown read into what goes out, with the same words stripped of their marks beside it.
  fn convert(&self, markdown: &str) -> ChatMarkup;

  /// The markdown divided into pieces that each fit one message. A dialect that does not split answers with
  /// the one piece it was given.
  fn parts(&self, markdown: &str) -> Vec<String>;

  /// Null when markup this dialect built is sound, else the reason.
  fn fault(&self, markup: &str) -> Option<String>;

  /// The folded markup, or null when there is nothing worth folding.
  fn fold(&self, markdown: &str) -> Option<String>;

  /// What a reader actually sees of the markup.
  fn visible(&self, markup: &str) -> String;

  /// The dialect as a declaration states it.
  fn declaration(&self) -> ChatDialectDeclaration {
    ChatDialectDeclaration {
      name: self.name().to_owned(),
      limits: self.limits(),
      writing: self.writing().to_owned(),
      foldable: self.foldable(),
    }
  }

  /// The fold, asked of a foldable dialect only; null for any other.
  fn folded(&self, markdown: &str) -> Option<String> {
    if self.foldable() { self.fold(markdown) } else { None }
  }

  /// The markup a text goes out as: the fold where the send asks for one and the dialect gives one, the
  /// conversion otherwise.
  fn outgoing(&self, markdown: &str, fold: bool) -> String {
    fold
      .then(|| self.folded(markdown))
      .flatten()
      .unwrap_or_else(|| self.convert(markdown).markup)
  }
}

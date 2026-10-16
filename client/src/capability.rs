//! The send verbs, and the capabilities a provider declares.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::VariantArray;

/// The twelve send verbs.
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, VariantArray,
)]
#[serde(rename_all = "camelCase")]
pub enum ChatVerb {
  Text,
  Photo,
  Album,
  Document,
  Audio,
  Voice,
  Sticker,
  Reaction,
  Pin,
  Typing,
  Edit,
  Delete,
}

/// What a provider can do: the twelve verbs, and four answers that are not verbs.
#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, VariantArray,
)]
#[serde(rename_all = "camelCase")]
pub enum ChatCapability {
  Text,
  Photo,
  Album,
  Document,
  Audio,
  Voice,
  Sticker,
  Reaction,
  Pin,
  Typing,
  Edit,
  Delete,
  /// An answer can be aimed at a particular message.
  ReplyTo,
  /// A picture or a file can still be fetched after its message arrived.
  FetchMedia,
  /// The provider supplies profile pictures.
  Avatars,
  /// The provider can read a room's past messages back.
  History,
}

impl ChatVerb {
  /// Every verb, in declaration order, taken from the type itself.
  pub const ALL: &'static [Self] = <Self as VariantArray>::VARIANTS;
}

impl ChatCapability {
  /// The full capability set, in declaration order, taken from the type itself.
  pub const ALL: &'static [Self] = <Self as VariantArray>::VARIANTS;

  /// What the capability is in the words a person reads, as the middle of a sentence.
  pub fn words(self) -> &'static str {
    match self {
      Self::Text => "send a message",
      Self::Photo => "send a photo",
      Self::Album => "send an album of photos",
      Self::Document => "send a file",
      Self::Audio => "send an audio file",
      Self::Voice => "send a voice message",
      Self::Sticker => "send a sticker",
      Self::Reaction => "react to a message",
      Self::Pin => "pin a message",
      Self::Typing => "show a typing indicator",
      Self::Edit => "edit a sent message",
      Self::Delete => "delete a sent message",
      Self::ReplyTo => "reply to a particular message",
      Self::FetchMedia => "fetch a picture or a file later",
      Self::Avatars => "fetch profile pictures",
      Self::History => "read past messages back",
    }
  }
}

impl From<ChatVerb> for ChatCapability {
  fn from(verb: ChatVerb) -> Self {
    match verb {
      ChatVerb::Text => Self::Text,
      ChatVerb::Photo => Self::Photo,
      ChatVerb::Album => Self::Album,
      ChatVerb::Document => Self::Document,
      ChatVerb::Audio => Self::Audio,
      ChatVerb::Voice => Self::Voice,
      ChatVerb::Sticker => Self::Sticker,
      ChatVerb::Reaction => Self::Reaction,
      ChatVerb::Pin => Self::Pin,
      ChatVerb::Typing => Self::Typing,
      ChatVerb::Edit => Self::Edit,
      ChatVerb::Delete => Self::Delete,
    }
  }
}

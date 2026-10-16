//! Refusals: a fixed English sentence and a code a caller can match on.
//!
//! On the wire a refusal and a missed picture's reason carry their sentence as `message` beside their code
//! and fields. The sentence is rendered from the fields, so it is written when one is encoded and passed
//! over when one is decoded.

use std::fmt;

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};

use crate::capability::ChatCapability;

/// Why a send was not performed. Every variant carries the provider's label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
  remote = "Self",
  tag = "code",
  rename_all = "kebab-case",
  rename_all_fields = "camelCase"
)]
#[schemars(rename = "ChatRefusal", transform = with_message)]
pub enum ChatRefusal {
  /// The provider does not declare the capability the send needs.
  UnsupportedCapability {
    label: String,
    capability: ChatCapability,
  },
  /// None of the pictures went, each with why.
  NoPictureSent {
    label: String,
    missed: Vec<MissedPicture>,
  },
  /// The provider speaks as the room's own agent alone, so it takes no other speaker.
  SpeakerNotAllowed { label: String, agent: String },
  /// The provider has no number in the room for the agent named as speaker.
  SpeakerUnnumbered { label: String, agent: String },
  /// The provider is not ready yet.
  NotReady { label: String },
  /// The provider's own English reason.
  Platform { label: String, detail: String },
}

impl ChatRefusal {
  /// The label of the provider that refused.
  pub fn label(&self) -> &str {
    match self {
      Self::UnsupportedCapability { label, .. }
      | Self::NoPictureSent { label, .. }
      | Self::SpeakerNotAllowed { label, .. }
      | Self::SpeakerUnnumbered { label, .. }
      | Self::NotReady { label }
      | Self::Platform { label, .. } => label,
    }
  }

  /// The code a caller matches on, as it crosses the wire.
  pub fn code(&self) -> &'static str {
    match self {
      Self::UnsupportedCapability { .. } => "unsupported-capability",
      Self::NoPictureSent { .. } => "no-picture-sent",
      Self::SpeakerNotAllowed { .. } => "speaker-not-allowed",
      Self::SpeakerUnnumbered { .. } => "speaker-unnumbered",
      Self::NotReady { .. } => "not-ready",
      Self::Platform { .. } => "platform",
    }
  }
}

impl fmt::Display for ChatRefusal {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::UnsupportedCapability { label, capability } => {
        write!(f, "{label} cannot {}.", capability.words())
      }
      Self::NoPictureSent { label, missed } => {
        write!(f, "{label}: no picture was sent.")?;
        for picture in missed {
          write!(f, "\n{}: {}", picture.address, picture.reason)?;
        }
        Ok(())
      }
      Self::SpeakerNotAllowed { label, agent } => {
        write!(f, "{label} cannot send in the name of agent {agent}.")
      }
      Self::SpeakerUnnumbered { label, agent } => {
        write!(f, "{label} has no number for agent {agent}, so it cannot send in that agent's name.")
      }
      Self::NotReady { label } => write!(f, "{label} is not ready yet."),
      Self::Platform { label, detail } => write!(f, "{label}: {detail}"),
    }
  }
}

/// A picture a send did not carry, by the address it was given as, with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MissedPicture {
  pub address: String,
  pub reason: MissedPictureReason,
}

/// Why one picture did not go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(remote = "Self", tag = "code", rename_all = "kebab-case")]
#[schemars(rename = "MissedPictureReason", transform = with_message)]
pub enum MissedPictureReason {
  NotAnAddress,
  OutsideDataRoot,
  MissingFile,
  Unfetched,
  /// The provider's own English reason.
  Platform {
    detail: String,
  },
}

impl MissedPictureReason {
  /// The code a caller matches on, as it crosses the wire.
  pub fn code(&self) -> &'static str {
    match self {
      Self::NotAnAddress => "not-an-address",
      Self::OutsideDataRoot => "outside-data-root",
      Self::MissingFile => "missing-file",
      Self::Unfetched => "unfetched",
      Self::Platform { .. } => "platform",
    }
  }
}

impl fmt::Display for MissedPictureReason {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::NotAnAddress => f.write_str("It is neither a web address nor a file path on this machine."),
      Self::OutsideDataRoot => f.write_str("The file is outside the data folder, so it was not sent."),
      Self::MissingFile => f.write_str("There is no such file on this machine."),
      Self::Unfetched => f.write_str("The picture could not be fetched."),
      Self::Platform { detail } => f.write_str(detail),
    }
  }
}

/// A refusal encoded as its code and fields, with its English sentence beside them as `message`.
#[derive(Serialize)]
struct RefusalOut<'a> {
  #[serde(flatten, serialize_with = "refusal_fields")]
  refusal: &'a ChatRefusal,
  message: String,
}

fn refusal_fields<S: Serializer>(refusal: &&ChatRefusal, serializer: S) -> Result<S::Ok, S::Error> {
  ChatRefusal::serialize(refusal, serializer)
}

impl Serialize for ChatRefusal {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    RefusalOut {
      refusal: self,
      message: self.to_string(),
    }
    .serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for ChatRefusal {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    ChatRefusal::deserialize(deserializer)
  }
}

/// A missed picture's reason encoded as its code and fields, with its English sentence as `message`.
#[derive(Serialize)]
struct ReasonOut<'a> {
  #[serde(flatten, serialize_with = "reason_fields")]
  reason: &'a MissedPictureReason,
  message: String,
}

fn reason_fields<S: Serializer>(reason: &&MissedPictureReason, serializer: S) -> Result<S::Ok, S::Error> {
  MissedPictureReason::serialize(reason, serializer)
}

impl Serialize for MissedPictureReason {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    ReasonOut {
      reason: self,
      message: self.to_string(),
    }
    .serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for MissedPictureReason {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    MissedPictureReason::deserialize(deserializer)
  }
}

/// Adds the English sentence every variant carries as `message` to the schema of each variant: the derived
/// schema has only the code and the fields.
fn with_message(schema: &mut Schema) {
  let message = json!({
    "type": "string",
    "description": "The English sentence, as the sender rendered it from the code and the fields.",
  });
  let Some(Value::Array(variants)) = schema.get_mut("oneOf") else {
    return;
  };
  for variant in variants.iter_mut().filter_map(Value::as_object_mut) {
    if let Some(Value::Object(properties)) = variant.get_mut("properties") {
      properties.insert("message".into(), message.clone());
    }
    if let Some(Value::Array(required)) = variant.get_mut("required") {
      required.push("message".into());
    }
  }
}

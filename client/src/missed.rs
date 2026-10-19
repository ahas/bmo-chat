//! Missed pictures: the pictures of an album that did not go, each with a fixed English sentence and a code
//! a caller can match on.
//!
//! On the wire a missed picture's reason carries its sentence as `message` beside its code and fields. The
//! sentence is rendered from the fields, so it is written when one is encoded and passed over when one is
//! decoded.

use std::fmt;

use schemars::{JsonSchema, Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};

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

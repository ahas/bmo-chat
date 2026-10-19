//! Refusals: a fixed English sentence and a code a caller can match on.
//!
//! A refusal crosses the wire in the error shape, as its code and its sentence; its fields stay with the
//! provider, which renders them into the sentence. A missed picture's reason crosses inside a send's answer,
//! with its sentence as `message` beside its code and fields. That sentence is rendered from the fields, so
//! it is written when one is encoded and passed over when one is decoded.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::capability::ChatCapability;
use crate::error::{ErrorCode, WireError};

/// Why a send or a fetch was not performed. Every variant carries the provider's label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRefusal {
  /// The provider does not declare the capability the call needs.
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
  /// The message is one the platform no longer has, or will never let change again, so the caller lets go
  /// of it instead of asking again; the provider's own English reason.
  Gone { label: String, detail: String },
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
      | Self::Platform { label, .. }
      | Self::Gone { label, .. } => label,
    }
  }

  /// The code a caller matches on, as it crosses the wire.
  pub fn code(&self) -> ErrorCode {
    match self {
      Self::UnsupportedCapability { .. } => ErrorCode::UnsupportedCapability,
      Self::NoPictureSent { .. } => ErrorCode::NoPictureSent,
      Self::SpeakerNotAllowed { .. } => ErrorCode::SpeakerNotAllowed,
      Self::SpeakerUnnumbered { .. } => ErrorCode::SpeakerUnnumbered,
      Self::NotReady { .. } => ErrorCode::NotReady,
      Self::Platform { .. } => ErrorCode::Platform,
      Self::Gone { .. } => ErrorCode::Gone,
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
      Self::Platform { label, detail } | Self::Gone { label, detail } => write!(f, "{label}: {detail}"),
    }
  }
}

/// A refusal as it crosses the wire: its code, with its sentence as the error.
impl From<ChatRefusal> for WireError {
  fn from(refusal: ChatRefusal) -> Self {
    WireError::new(refusal.code(), refusal.to_string())
  }
}

/// A picture a send did not carry, by the address it was given as, with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissedPicture {
  pub address: String,
  pub reason: MissedPictureReason,
}

/// Why one picture did not go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", tag = "code", rename_all = "kebab-case")]
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

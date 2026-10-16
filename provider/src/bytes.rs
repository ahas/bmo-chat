//! Bytes on the wire: a base64 string in the standard alphabet, with padding.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
  serializer.serialize_str(&STANDARD.encode(bytes))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
  let text = String::deserialize(deserializer)?;
  STANDARD
    .decode(text)
    .map_err(|why| serde::de::Error::custom(format!("the bytes are not base64: {why}")))
}

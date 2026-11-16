//! A file's bytes: in memory, in a file on this machine's disk, or withheld, and how each crosses the wire
//! (`docs/contract.md`, "A file's bytes").
//!
//! A body stored on disk crosses as `{"part": "<name>"}` naming a part of a multipart body. The derived
//! (de)serializers of the bodies that carry files have no way to hand a file body its request's parts, so the
//! parts stand in a per-thread context for the length of one synchronous (de)serialization, opened and
//! closed by `written` and `read_with_parts` alone.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, DeserializeOwned, MapAccess, Visitor};
use serde::ser::{self, SerializeMap};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::io::AsyncRead;

/// A file's bytes, held one of three ways.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBody {
  /// The bytes themselves, in memory; they cross inline, as base64.
  Inline(Vec<u8>),
  /// A file on this machine's disk, read only as it is sent; it crosses as a part of a multipart body.
  Stored(StoredFile),
  /// No bytes: an answer names the file without them, since its asker already holds them.
  Withheld,
}

impl FileBody {
  /// The file at `path` on this machine's disk, read where it lies and left there.
  pub fn stored(path: impl Into<PathBuf>) -> Self {
    Self::Stored(StoredFile::at(path))
  }

  /// The body an answer names a file by.
  pub fn withheld() -> Self {
    Self::Withheld
  }

  pub fn is_withheld(&self) -> bool {
    matches!(self, Self::Withheld)
  }

  /// The bytes, whole, in memory.
  pub async fn read(&self) -> io::Result<Vec<u8>> {
    match self {
      Self::Inline(bytes) => Ok(bytes.clone()),
      Self::Stored(file) => tokio::fs::read(file.path()).await,
      Self::Withheld => Err(withheld()),
    }
  }

  /// How many bytes the file holds.
  pub async fn size(&self) -> io::Result<u64> {
    match self {
      Self::Inline(bytes) => Ok(bytes.len() as u64),
      Self::Stored(file) => Ok(tokio::fs::metadata(file.path()).await?.len()),
      Self::Withheld => Err(withheld()),
    }
  }

  /// The bytes as a stream, read from the disk for a stored file.
  pub async fn reader(&self) -> io::Result<Pin<Box<dyn AsyncRead + Send + Sync>>> {
    match self {
      Self::Inline(bytes) => Ok(Box::pin(Cursor::new(bytes.clone()))),
      Self::Stored(file) => Ok(Box::pin(tokio::fs::File::open(file.path()).await?)),
      Self::Withheld => Err(withheld()),
    }
  }
}

impl From<Vec<u8>> for FileBody {
  fn from(bytes: Vec<u8>) -> Self {
    Self::Inline(bytes)
  }
}

fn withheld() -> io::Error {
  io::Error::other("The file was named without its bytes.")
}

/// A file on this machine's disk. One a part arrived as is the library's own and removed once the last clone
/// of it is dropped, unless it was moved away first; one a holder names is left where it lies.
#[derive(Debug, Clone)]
pub struct StoredFile(Arc<Stored>);

#[derive(Debug)]
struct Stored {
  path: PathBuf,
  spooled: bool,
}

impl Drop for Stored {
  fn drop(&mut self) {
    if self.spooled {
      // A spooled file moved away first is no longer there to remove.
      let _ = std::fs::remove_file(&self.path);
    }
  }
}

impl StoredFile {
  /// The file at `path`, which its holder keeps.
  pub fn at(path: impl Into<PathBuf>) -> Self {
    Self(Arc::new(Stored {
      path: path.into(),
      spooled: false,
    }))
  }

  /// A file a part was written to, removed once the last clone of it is dropped.
  pub(crate) fn spooled(path: PathBuf) -> Self {
    Self(Arc::new(Stored {
      path,
      spooled: true,
    }))
  }

  pub fn path(&self) -> &Path {
    &self.0.path
  }
}

impl PartialEq for StoredFile {
  fn eq(&self, other: &Self) -> bool {
    self.path() == other.path()
  }
}

impl Eq for StoredFile {}

/// The parts of the body being written or read on this thread.
enum Parts {
  /// Each stored file met becomes the next part, `file-0` first.
  Writing(Vec<StoredFile>),
  /// The parts the body arrived with, by name, and the names its files named.
  Reading {
    parts: BTreeMap<String, StoredFile>,
    named: BTreeSet<String>,
  },
}

thread_local! {
  static PARTS: RefCell<Option<Parts>> = const { RefCell::new(None) };
}

/// The context open for one (de)serialization, closed when dropped, a failed one included.
struct Open;

impl Open {
  fn new(parts: Parts) -> Self {
    PARTS.set(Some(parts));
    Self
  }

  fn close(self) -> Option<Parts> {
    PARTS.take()
  }
}

impl Drop for Open {
  fn drop(&mut self) {
    PARTS.set(None);
  }
}

/// The name of the part a body's `index`th stored file crosses as.
pub(crate) fn part_name(index: usize) -> String {
  format!("file-{index}")
}

/// The body as JSON, and the stored files it names, in the order of their parts' names.
pub(crate) fn written<T: Serialize>(body: &T) -> serde_json::Result<(Vec<u8>, Vec<StoredFile>)> {
  let open = Open::new(Parts::Writing(Vec::new()));
  let json = serde_json::to_vec(body)?;
  let files = match open.close() {
    Some(Parts::Writing(files)) => files,
    _ => Vec::new(),
  };
  Ok((json, files))
}

/// The body read from its JSON, each part it names taken from `parts`; the sentence says what is wrong with
/// a body that is not the shape, names a part it does not carry or carries a part no file names.
pub(crate) fn read_with_parts<T: DeserializeOwned>(
  json: &[u8],
  parts: BTreeMap<String, StoredFile>,
) -> Result<T, String> {
  let open = Open::new(Parts::Reading {
    parts,
    named: BTreeSet::new(),
  });
  let body = serde_json::from_slice(json).map_err(|error| error.to_string())?;
  if let Some(Parts::Reading { parts, named }) = open.close()
    && let Some(unnamed) = parts.keys().find(|name| !named.contains(*name))
  {
    return Err(format!("the part {unnamed} is named by no file"));
  }
  Ok(body)
}

impl Serialize for FileBody {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    match self {
      Self::Inline(bytes) => serializer.serialize_str(&STANDARD.encode(bytes)),
      Self::Stored(file) => {
        let name = PARTS.with_borrow_mut(|parts| match parts {
          Some(Parts::Writing(files)) => {
            files.push(file.clone());
            Some(part_name(files.len() - 1))
          }
          _ => None,
        });
        let name = name.ok_or_else(|| {
          ser::Error::custom("a file stored on disk crosses only as a part of a multipart body")
        })?;
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("part", &name)?;
        map.end()
      }
      Self::Withheld => Err(ser::Error::custom("a withheld file has no bytes to send")),
    }
  }
}

impl<'de> Deserialize<'de> for FileBody {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    deserializer.deserialize_any(BodyVisitor)
  }
}

struct BodyVisitor;

impl<'de> Visitor<'de> for BodyVisitor {
  type Value = FileBody;

  fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
    f.write_str("the bytes as base64, or the part of a multipart body that carries them")
  }

  fn visit_str<E: de::Error>(self, text: &str) -> Result<FileBody, E> {
    STANDARD
      .decode(text)
      .map(FileBody::Inline)
      .map_err(|why| E::custom(format!("the bytes are not base64: {why}")))
  }

  fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<FileBody, A::Error> {
    let mut part: Option<String> = None;
    while let Some(key) = map.next_key::<String>()? {
      if key != "part" {
        return Err(de::Error::unknown_field(&key, &["part"]));
      }
      part = Some(map.next_value()?);
    }
    let name = part.ok_or_else(|| de::Error::missing_field("part"))?;
    let taken = PARTS.with_borrow_mut(|parts| match parts {
      Some(Parts::Reading { parts, named }) => {
        let file = parts.get(&name).cloned();
        named.insert(name.clone());
        file
      }
      _ => None,
    });
    taken
      .map(FileBody::Stored)
      .ok_or_else(|| de::Error::custom(format!("the body names the part {name}, which it does not carry")))
  }
}

impl JsonSchema for FileBody {
  fn schema_name() -> Cow<'static, str> {
    "FileBody".into()
  }

  fn json_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
      "description": "A file's bytes: inline as base64, or the part of a multipart body that carries them. An \
        answer that names a file without its bytes leaves the field out.",
      "oneOf": [
        { "type": "string", "contentEncoding": "base64" },
        {
          "type": "object",
          "properties": {
            "part": {
              "type": "string",
              "description": "The name of the part of this multipart body that carries the bytes.",
            },
          },
          "required": ["part"],
          "additionalProperties": false,
        },
      ],
    })
  }
}

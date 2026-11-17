//! A body that carries files, as it is sent and as it arrives, a request's or an answer's: JSON, or multipart
//! with each stored file as a part of its own (`docs/protocol.md`, "Files carried as parts").
//!
//! The multipart body is written here rather than by reqwest's multipart form: the registry line every unit
//! shares for reqwest carries no multipart feature. Only the writing is the library's own; an arriving
//! multipart body, a request's and an answer's alike, is read by multer, the reader axum's own is built on.

use std::collections::BTreeMap;
use std::io::{self, Cursor};
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::header::CONTENT_TYPE;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

use crate::error::{ErrorCode, WireError};
use crate::file_body::{StoredFile, part_name, read_with_parts, written};

/// The part of a multipart body that carries its JSON.
const BODY_PART: &str = "body";

/// A body ready to be sent: its JSON, and the stored files that cross as its parts.
pub(crate) struct Outgoing {
  json: Vec<u8>,
  files: Vec<StoredFile>,
  boundary: String,
}

impl Outgoing {
  /// The body written for sending; it goes as multipart only where it names a file stored on disk.
  pub(crate) fn of(body: &impl Serialize) -> serde_json::Result<Self> {
    let (json, files) = written(body)?;
    Ok(Self {
      json,
      files,
      boundary: boundary(),
    })
  }

  /// The body's `Content-Type`.
  pub(crate) fn content_type(&self) -> String {
    if self.files.is_empty() {
      "application/json".to_owned()
    } else {
      format!("multipart/form-data; boundary={}", self.boundary)
    }
  }

  /// The body's bytes, its stored files read from the disk as they are sent. Each offer of a request takes
  /// the body anew, so a file is opened again for each.
  pub(crate) async fn body(&self) -> io::Result<reqwest::Body> {
    if self.files.is_empty() {
      return Ok(reqwest::Body::from(self.json.clone()));
    }
    let boundary = &self.boundary;
    let mut head = format!(
      "--{boundary}\r\nContent-Disposition: form-data; name=\"{BODY_PART}\"\r\n\
       Content-Type: application/json\r\n\r\n"
    )
    .into_bytes();
    head.extend_from_slice(&self.json);
    let mut stream: Pin<Box<dyn AsyncRead + Send + Sync>> = Box::pin(Cursor::new(head));
    for (index, file) in self.files.iter().enumerate() {
      let name = part_name(index);
      let opening = format!(
        "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
      );
      let bytes = tokio::fs::File::open(file.path()).await?;
      stream = Box::pin(stream.chain(Cursor::new(opening.into_bytes())).chain(bytes));
    }
    let closing = format!("\r\n--{boundary}--\r\n");
    let stream = stream.chain(Cursor::new(closing.into_bytes()));
    Ok(reqwest::Body::wrap_stream(ReaderStream::new(stream)))
  }
}

/// A boundary no file's bytes are expected to hold: the time, the process and a count, in hexadecimal.
fn boundary() -> String {
  static COUNT: AtomicU64 = AtomicU64::new(0);
  let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
  let count = COUNT.fetch_add(1, Ordering::Relaxed);
  format!("bmo-chat-{nanos:032x}-{:08x}-{count:016x}", std::process::id())
}

/// Why an arriving multipart body could not be taken.
pub(crate) enum Unread {
  /// The body is not a multipart body of the shape a body that carries files takes.
  Shape(String),
  /// The stream the body came on broke off.
  Broken(String),
  /// A part could not be written into the spool.
  Unkept(io::Error),
}

impl From<multer::Error> for Unread {
  fn from(error: multer::Error) -> Self {
    match error {
      multer::Error::StreamReadFailed(why) => Self::Broken(why.to_string()),
      error => Self::Shape(error.to_string()),
    }
  }
}

/// The boundary of a multipart body of this `Content-Type`, or none where the body is not multipart.
pub(crate) fn multipart_boundary(content_type: Option<&str>) -> Result<Option<String>, Unread> {
  match content_type {
    Some(value) if value.trim_start().to_ascii_lowercase().starts_with("multipart/form-data") => {
      multer::parse_boundary(value).map(Some).map_err(Unread::from)
    }
    _ => Ok(None),
  }
}

/// A multipart body's JSON, and each of its other parts written into `spool` by its name as it arrives.
pub(crate) async fn spooled(
  mut form: multer::Multipart<'_>,
  spool: &Path,
) -> Result<(Bytes, BTreeMap<String, StoredFile>), Unread> {
  let mut json = None;
  let mut parts = BTreeMap::new();
  while let Some(mut field) = form.next_field().await? {
    let name = field.name().unwrap_or_default().to_owned();
    if (name == BODY_PART && json.is_some()) || parts.contains_key(&name) {
      return Err(Unread::Shape(format!("it carries two parts named {name}")));
    }
    if name == BODY_PART {
      json = Some(field.bytes().await?);
      continue;
    }
    let (file, mut out) = spool_file(spool).await.map_err(Unread::Unkept)?;
    while let Some(chunk) = field.chunk().await? {
      out.write_all(&chunk).await.map_err(Unread::Unkept)?;
    }
    out.flush().await.map_err(Unread::Unkept)?;
    parts.insert(name, file);
  }
  let json = json.ok_or_else(|| Unread::Shape(format!("it has no part named {BODY_PART}")))?;
  Ok((json, parts))
}

/// The body of `request` read as `T`: its JSON, or its multipart body with each file part written into
/// `spool` as it arrives.
pub(crate) async fn arrived<T: DeserializeOwned>(request: Request, spool: &Path) -> Result<T, WireError> {
  let content_type = request.headers().get(CONTENT_TYPE).and_then(|value| value.to_str().ok());
  let (json, parts) = match multipart_boundary(content_type).map_err(refused)? {
    Some(boundary) => {
      let form = multer::Multipart::new(request.into_body().into_data_stream(), boundary);
      spooled(form, spool).await.map_err(refused)?
    }
    None => {
      let body = Bytes::from_request(request, &()).await.map_err(|rejection| unreadable(&rejection))?;
      (body, BTreeMap::new())
    }
  };
  read_with_parts(&json, parts).map_err(|why| {
    WireError::new(ErrorCode::BadRequest, format!("The body is not the shape this route takes: {why}."))
  })
}

/// A new file in `spool`, removed once the part written to it is let go.
async fn spool_file(spool: &Path) -> io::Result<(StoredFile, tokio::fs::File)> {
  static COUNT: AtomicU64 = AtomicU64::new(0);
  tokio::fs::create_dir_all(spool).await?;
  loop {
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    let path = spool.join(format!("bmo-chat-part-{}-{count}", std::process::id()));
    match tokio::fs::OpenOptions::new().write(true).create_new(true).open(&path).await {
      Ok(out) => return Ok((StoredFile::spooled(path), out)),
      Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
      Err(error) => return Err(error),
    }
  }
}

/// A request body that could not be taken, in the error shape.
fn refused(unread: Unread) -> WireError {
  match unread {
    Unread::Shape(why) | Unread::Broken(why) => unreadable(&why),
    Unread::Unkept(error) => WireError::new(
      ErrorCode::Failed,
      format!("A part of the body could not be written to disk: {error}."),
    ),
  }
}

fn unreadable(why: &impl std::fmt::Display) -> WireError {
  WireError::new(ErrorCode::BadRequest, format!("The body could not be read: {why}."))
}

//! A body that carries files, as it is sent, as it is answered and as it arrives: JSON, or multipart with each
//! stored file as a part of its own (`docs/protocol.md`, "Files carried as parts").
//!
//! The multipart body is written here rather than by reqwest's multipart form: the registry line every unit
//! shares for reqwest carries no multipart feature, and an answer is no reqwest body. Only the writing is the
//! library's own; an arriving multipart body is read by axum's reader.

use std::collections::BTreeMap;
use std::io::{self, Cursor};
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes};
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

use crate::error::{ErrorCode, WireError};
use crate::file_body::{StoredFile, part_name, read_with_parts, written};

/// The part of a multipart body that carries its JSON.
const BODY_PART: &str = "body";

/// A body ready to be sent or answered: its JSON, and the stored files that cross as its parts.
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
    Ok(reqwest::Body::wrap_stream(ReaderStream::new(self.parts().await?)))
  }

  /// The body as an answer, its stored files read from the disk as it is sent; a file that cannot be opened
  /// fails the answer.
  pub(crate) async fn answer(self) -> Result<Response, WireError> {
    let content_type = self.content_type();
    let body = if self.files.is_empty() {
      Body::from(self.json)
    } else {
      let parts = self.parts().await.map_err(|error| {
        WireError::new(ErrorCode::Failed, format!("A file the answer carries could not be read: {error}."))
      })?;
      Body::from_stream(ReaderStream::new(parts))
    };
    Ok(([(CONTENT_TYPE, content_type)], body).into_response())
  }

  /// The multipart body as a stream, each stored file opened now and read as the stream reaches it.
  async fn parts(&self) -> io::Result<Pin<Box<dyn AsyncRead + Send + Sync>>> {
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
    Ok(Box::pin(stream.chain(Cursor::new(closing.into_bytes()))))
  }
}

/// A boundary no file's bytes are expected to hold: the time, the process and a count, in hexadecimal.
fn boundary() -> String {
  static COUNT: AtomicU64 = AtomicU64::new(0);
  let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
  let count = COUNT.fetch_add(1, Ordering::Relaxed);
  format!("bmo-chat-{nanos:032x}-{:08x}-{count:016x}", std::process::id())
}

/// The body of `request` read as `T`: its JSON, or its multipart body with each file part written into
/// `spool` as it arrives.
pub(crate) async fn arrived<T: DeserializeOwned>(request: Request, spool: &Path) -> Result<T, WireError> {
  let multipart = request
    .headers()
    .get(CONTENT_TYPE)
    .and_then(|value| value.to_str().ok())
    .is_some_and(|value| value.trim_start().to_ascii_lowercase().starts_with("multipart/form-data"));
  let (json, parts) = if multipart {
    spooled(request, spool).await?
  } else {
    let body = Bytes::from_request(request, &()).await.map_err(|rejection| unreadable(&rejection))?;
    (body, BTreeMap::new())
  };
  read_with_parts(&json, parts).map_err(|why| {
    WireError::new(ErrorCode::BadRequest, format!("The body is not the shape this route takes: {why}."))
  })
}

/// A multipart body's JSON, and each of its other parts written into `spool` by its name.
async fn spooled(
  request: Request,
  spool: &Path,
) -> Result<(Bytes, BTreeMap<String, StoredFile>), WireError> {
  let mut form = Multipart::from_request(request, &())
    .await
    .map_err(|rejection| unreadable(&rejection))?;
  let mut json = None;
  let mut parts = BTreeMap::new();
  while let Some(mut field) = form.next_field().await.map_err(|error| unreadable(&error))? {
    let name = field.name().unwrap_or_default().to_owned();
    if (name == BODY_PART && json.is_some()) || parts.contains_key(&name) {
      return Err(bad(format!("The body carries two parts named {name}.")));
    }
    if name == BODY_PART {
      json = Some(field.bytes().await.map_err(|error| unreadable(&error))?);
      continue;
    }
    let (file, mut out) = spool_file(spool).await.map_err(unkept)?;
    while let Some(chunk) = field.chunk().await.map_err(|error| unreadable(&error))? {
      out.write_all(&chunk).await.map_err(unkept)?;
    }
    out.flush().await.map_err(unkept)?;
    parts.insert(name, file);
  }
  let json = json.ok_or_else(|| bad(format!("The multipart body has no part named {BODY_PART}.")))?;
  Ok((json, parts))
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

fn bad(error: String) -> WireError {
  WireError::new(ErrorCode::BadRequest, error)
}

fn unreadable(why: &impl std::fmt::Display) -> WireError {
  bad(format!("The body could not be read: {why}."))
}

fn unkept(error: io::Error) -> WireError {
  WireError::new(ErrorCode::Failed, format!("A part of the body could not be written to disk: {error}."))
}

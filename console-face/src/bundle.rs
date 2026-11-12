//! The bundle build's folder and the files the face answers from it: the module, the stylesheet and the words
//! under paths made from their bytes, every other file under the name the build gave it, and the layer the
//! build wrote beside the module.

use axum::body::Bytes;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::ConsoleFaceError;

/// The file the bundle build writes beside the module, naming the layer it was built on.
const LAYER_FILE: &str = "layer.json";

/// How many hexadecimal digits of a file's sha256 stand in its path.
const DIGEST_DIGITS: usize = 16;

/// The bundle build's folder, and the names of the files in it that the manifest names.
#[derive(Clone, Debug)]
pub struct BundleFolder {
  folder: PathBuf,
  module: String,
  stylesheet: Option<String>,
  words: String,
}

impl BundleFolder {
  /// The folder the bundle build wrote, the module's file name in it and the words' file name in it.
  pub fn new(folder: impl Into<PathBuf>, module: impl Into<String>, words: impl Into<String>) -> Self {
    Self {
      folder: folder.into(),
      module: module.into(),
      stylesheet: None,
      words: words.into(),
    }
  }

  /// The stylesheet's file name in the folder, where the bundle has one.
  pub fn with_stylesheet(mut self, stylesheet: impl Into<String>) -> Self {
    self.stylesheet = Some(stylesheet.into());
    self
  }

  /// Every file of the folder as the face answers it.
  pub(crate) fn read(&self) -> Result<Bundle, ConsoleFaceError> {
    let layer = self.layer()?;
    let module = self.named("module", &self.module)?;
    let stylesheet = self.stylesheet.as_deref().map(|file| self.named("stylesheet", file)).transpose()?;
    let words_file = self.named("words", &self.words)?;
    let words: Value =
      serde_json::from_slice(&words_file.served.bytes).map_err(|why| ConsoleFaceError::Malformed {
        what: self.words.clone(),
        why: format!("is not JSON: {why}"),
      })?;
    if !words.is_object() {
      return Err(ConsoleFaceError::Malformed {
        what: self.words.clone(),
        why: "is not one object keyed by locale".to_owned(),
      });
    }

    let mut files = BTreeMap::new();
    let mut taken: Vec<&str> = vec![LAYER_FILE, &self.module, &self.words];
    taken.extend(self.stylesheet.as_deref());
    for relative in self.loaded_files()? {
      if taken.contains(&relative.as_str()) {
        continue;
      }
      let bytes = read_bytes(&self.folder.join(&relative))?;
      files.insert(relative.clone(), Served::new(&relative, bytes));
    }
    for named in [Some(&module), stylesheet.as_ref(), Some(&words_file)].into_iter().flatten() {
      if files.insert(named.name.clone(), named.served.clone()).is_some() {
        return Err(ConsoleFaceError::SamePath(named.name.clone()));
      }
    }

    Ok(Bundle {
      layer,
      module: module.name,
      stylesheet: stylesheet.map(|named| named.name),
      words_name: words_file.name,
      words,
      files,
    })
  }

  fn layer(&self) -> Result<String, ConsoleFaceError> {
    let bytes = read_bytes(&self.folder.join(LAYER_FILE))?;
    let read: Value = serde_json::from_slice(&bytes).map_err(|why| ConsoleFaceError::Malformed {
      what: LAYER_FILE.to_owned(),
      why: format!("is not JSON: {why}"),
    })?;
    read.get("layer").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| {
      ConsoleFaceError::Malformed {
        what: LAYER_FILE.to_owned(),
        why: "names no layer".to_owned(),
      }
    })
  }

  // A named file, answered in the folder it stands in under its kind and its bytes' digest, with the extension
  // it was written with, so what the module loads by a relative name stands where the build put it.
  fn named(&self, kind: &str, file: &str) -> Result<Named, ConsoleFaceError> {
    let bytes = read_bytes(&self.folder.join(file))?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let written = Path::new(file);
    let folder = written
      .parent()
      .map(|parent| parent.to_string_lossy().into_owned())
      .filter(|parent| !parent.is_empty())
      .map(|parent| format!("{parent}/"))
      .unwrap_or_default();
    let extension = written.extension().map(|extension| format!(".{}", extension.to_string_lossy()));
    let name = format!("{folder}{kind}-{}{}", &digest[..DIGEST_DIGITS], extension.unwrap_or_default());
    Ok(Named {
      served: Served::new(file, bytes),
      name,
    })
  }

  // Every file under the folder, by its path from the folder with `/` between segments; a hidden entry, which
  // a build keeps for itself, is left out.
  fn loaded_files(&self) -> Result<Vec<String>, ConsoleFaceError> {
    let mut found = Vec::new();
    let mut folders = vec![(self.folder.clone(), String::new())];
    while let Some((folder, prefix)) = folders.pop() {
      let entries = std::fs::read_dir(&folder).map_err(|why| ConsoleFaceError::Unreadable {
        path: folder.clone(),
        why: why.to_string(),
      })?;
      for entry in entries {
        let entry = entry.map_err(|why| ConsoleFaceError::Unreadable {
          path: folder.clone(),
          why: why.to_string(),
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
          continue;
        }
        let relative = format!("{prefix}{name}");
        if entry.path().is_dir() {
          folders.push((entry.path(), format!("{relative}/")));
        } else {
          found.push(relative);
        }
      }
    }
    Ok(found)
  }
}

/// The folder as the face answers it.
pub(crate) struct Bundle {
  pub layer: String,
  pub module: String,
  pub stylesheet: Option<String>,
  pub words_name: String,
  pub words: Value,
  /// Every file, by its name under the face's files prefix.
  pub files: BTreeMap<String, Served>,
}

/// One file's bytes and the type it is answered as.
#[derive(Clone, Debug)]
pub(crate) struct Served {
  pub content_type: String,
  pub bytes: Bytes,
}

impl Served {
  fn new(file: &str, bytes: Vec<u8>) -> Self {
    let content_type = mime_guess::from_path(file).first_or_octet_stream().essence_str().to_owned();
    Self {
      content_type,
      bytes: Bytes::from(bytes),
    }
  }
}

struct Named {
  served: Served,
  name: String,
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, ConsoleFaceError> {
  std::fs::read(path).map_err(|why| ConsoleFaceError::Unreadable {
    path: path.to_owned(),
    why: why.to_string(),
  })
}

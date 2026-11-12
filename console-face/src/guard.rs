//! Who a call to the face, or to a screen's route, comes from: the console's credential, and the role and owner
//! key the console relays with it (bmo-ui's `docs/console-bundles.md`, 「Roles and the owner key」).

use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, Method};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::Refusal;

/// The header the console relays the reader's owner key in.
pub const OWNER_KEY_HEADER: &str = "Owner-Key";
/// The header the console relays the reader's role in.
pub const OWNER_ROLE_HEADER: &str = "Owner-Role";

const BEARER: &str = "Bearer ";

/// The contract's roles, from the one holding the most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
  Member,
  Administrator,
}

impl Role {
  fn read(word: &str) -> Option<Self> {
    match word {
      "administrator" => Some(Self::Administrator),
      "member" => Some(Self::Member),
      _ => None,
    }
  }
}

/// What the manifest asks of a reader, and the console's credential, as its digest.
#[derive(Clone, Debug)]
pub(crate) struct Guard {
  credential: [u8; 32],
  role: Role,
  owner_key: bool,
  /// Whether the manifest names a share read, so a reader with no sign-in loads the bundle's files.
  share_page: bool,
  /// The patterns of the share reads and of the share head, each as its segments.
  shared: Vec<Vec<String>>,
}

impl Guard {
  /// The guard of a manifest that already keeps the contract's schema.
  pub fn new(credential: &str, manifest: &Value) -> Self {
    let role = manifest
      .get("role")
      .and_then(Value::as_str)
      .and_then(Role::read)
      .unwrap_or(Role::Administrator);
    let owner_key = manifest.get("ownerKey").and_then(Value::as_bool).unwrap_or(false);
    let reads: Vec<&Value> =
      manifest.get("shareReads").and_then(Value::as_array).into_iter().flatten().collect();
    let head = manifest.get("shareHead");
    let share_page = !reads.is_empty();
    let shared = reads.into_iter().chain(head).filter_map(Value::as_str).map(segments).collect();
    Self {
      credential: digest(credential),
      role,
      owner_key,
      share_page,
      shared,
    }
  }

  /// Whether a call is a share read or the share head's read, which the contract holds to its token instead of
  /// the role and the owner key (bmo-ui's `docs/console-bundles.md`, 「Share pages」): a `GET` or a `HEAD` whose
  /// path a pattern matches segment by segment, a placeholder matching any one segment but an empty one and one
  /// that reads as a dot or two. A segment holding an escaped dot is matched by no placeholder, so such a call is
  /// held to the role.
  pub fn shared(&self, method: &Method, path: &str) -> bool {
    if method != Method::GET && method != Method::HEAD {
      return false;
    }
    let asked = segments(path);
    self.shared.iter().any(|pattern| {
      pattern.len() == asked.len()
        && pattern.iter().zip(&asked).all(|(written, segment)| {
          if written.starts_with('{') && written.ends_with('}') {
            !segment.is_empty()
              && segment != "."
              && segment != ".."
              && !segment.to_ascii_lowercase().contains("%2e")
          } else {
            written == segment
          }
        })
    })
  }

  /// Whether the call carries the console's credential.
  pub fn carries_console_credential(&self, headers: &HeaderMap) -> bool {
    let Some(token) = headers.get(AUTHORIZATION).and_then(|value| value.to_str().ok()) else {
      return false;
    };
    // Compared by digest, so the time taken says nothing of how much of the token was right.
    token.strip_prefix(BEARER).is_some_and(|token| digest(token) == self.credential)
  }

  /// The console's credential, refused 401 where the call lacks it.
  pub fn console(&self, headers: &HeaderMap) -> Result<(), Refusal> {
    if self.carries_console_credential(headers) {
      Ok(())
    } else {
      Err(Refusal::Unauthorized)
    }
  }

  /// The manifest's role and owner key, refused 403 where the call does not carry what they ask.
  pub fn reader(&self, headers: &HeaderMap) -> Result<(), Refusal> {
    let held = headers
      .get(OWNER_ROLE_HEADER)
      .and_then(|value| value.to_str().ok())
      .and_then(Role::read);
    let key = headers
      .get(OWNER_KEY_HEADER)
      .and_then(|value| value.to_str().ok())
      .filter(|key| !key.is_empty());
    let allowed = held.is_some_and(|held| held >= self.role);
    if allowed && (!self.owner_key || key.is_some()) {
      Ok(())
    } else {
      Err(Refusal::Forbidden)
    }
  }

  /// What a file of the bundle asks: the reader's role and owner key, but where the manifest names a share read,
  /// whose page a reader with no sign-in loads.
  pub fn file(&self, headers: &HeaderMap) -> Result<(), Refusal> {
    if self.share_page { Ok(()) } else { self.reader(headers) }
  }
}

// A path's segments after its leading slash.
fn segments(path: &str) -> Vec<String> {
  path.strip_prefix('/').unwrap_or(path).split('/').map(str::to_owned).collect()
}

fn digest(text: &str) -> [u8; 32] {
  Sha256::digest(text.as_bytes()).into()
}

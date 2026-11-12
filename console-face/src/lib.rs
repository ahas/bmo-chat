//! A service's console face, as bmo-ui's console contract (`docs/console-bundles.md` in bmo-ui) states it and
//! this repository's `docs/console-face.md` states what the crate takes and answers: the manifest assembled
//! from the service's one declaration and its bundle build's folder, the bundle's files under paths that follow
//! their bytes, the guard around them, the guard a service lays over its screens' own routes, the check of the
//! console's credential a service makes on a route it guards itself, and the reading of a manifest through
//! bmo-ui's own release for tests.

mod bundle;
mod error;
mod face;
mod guard;
mod reading;
mod schema;

pub use bundle::BundleFolder;
pub use error::ConsoleFaceError;
pub use face::{CONSOLE_FILES_PREFIX, CONSOLE_MANIFEST_ROUTE, ConsoleFace};
pub use guard::{ConsoleCredential, OWNER_KEY_HEADER, OWNER_ROLE_HEADER};
pub use reading::{ReadingError, contract_schema, contract_terms, read_manifest, share_reads};

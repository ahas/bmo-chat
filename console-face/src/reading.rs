//! A manifest read through bmo-ui's own reading of the contract, and the contract's JSON Schema as it answers
//! it, each run by Bun from bmo-ui's packed release file (`docs/console-face.md`, 「The reading through bmo-ui's
//! release」). Tests call these; a service's running code never does.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The contract's shared module inside the release, from the packed file's root folder.
const SHARED_MODULE: &str = "./shared/console-manifest.ts";

/// The folder a packed file holds the package under.
const PACKAGE_FOLDER: &str = "package";

/// The folder Bun resolves a package's imports from.
const MODULES_FOLDER: &str = "node_modules";

/// The packages the shared module imports when it runs.
const IMPORTED: &[&str] = &["zod"];

/// Why a reading did not answer a manifest.
#[derive(Debug)]
pub enum ReadingError {
  /// bmo-ui's reading refused the manifest; the words are its own, naming each field at fault.
  Refused(String),
  /// The reading could not be run: the release would not unpack, or Bun did not run.
  Unrun(String),
}

impl fmt::Display for ReadingError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Refused(why) => write!(formatter, "{why}"),
      Self::Unrun(why) => write!(formatter, "the reading could not be run: {why}"),
    }
  }
}

impl std::error::Error for ReadingError {}

/// The manifest as bmo-ui's `consoleManifest` in the release file reads it, every optional field that states a
/// default filled, or the reading's refusal.
pub fn read_manifest(manifest: &Value, release: &Path) -> Result<Value, ReadingError> {
  let script = format!(
    "import {{ consoleManifest }} from \"{SHARED_MODULE}\";\n\
     let given = \"\";\n\
     for await (const part of process.stdin) given += String(part);\n\
     try {{\n\
       process.stdout.write(JSON.stringify(consoleManifest(JSON.parse(given))));\n\
     }} catch (fault) {{\n\
       process.stderr.write(fault instanceof Error ? fault.message : String(fault));\n\
       process.exitCode = 1;\n\
     }}\n"
  );
  run(release, &script, &manifest.to_string())
}

/// The contract's JSON Schema of a manifest as the release file's shared module answers it.
pub fn contract_schema(release: &Path) -> Result<Value, ReadingError> {
  let script = format!(
    "import {{ CONSOLE_MANIFEST_SCHEMA }} from \"{SHARED_MODULE}\";\n\
     process.stdout.write(JSON.stringify(CONSOLE_MANIFEST_SCHEMA));\n"
  );
  run(release, &script, "")
}

/// Whether each method and path asked is a share read of the manifest, as bmo-ui's `shareRead` in the release file
/// answers it, in the order asked.
pub fn share_reads(
  manifest: &Value,
  asked: &[(&str, &str)],
  release: &Path,
) -> Result<Vec<bool>, ReadingError> {
  let script = format!(
    "import {{ consoleManifest, shareRead }} from \"{SHARED_MODULE}\";\n\
     let given = \"\";\n\
     for await (const part of process.stdin) given += String(part);\n\
     const {{ manifest, asked }} = JSON.parse(given);\n\
     const read = consoleManifest(manifest);\n\
     process.stdout.write(JSON.stringify(asked.map(([method, path]) => shareRead(read, method, path))));\n"
  );
  let input = serde_json::json!({ "manifest": manifest, "asked": asked });
  let answer = run(release, &script, &input.to_string())?;
  serde_json::from_value(answer)
    .map_err(|why| ReadingError::Unrun(format!("the reading answered no list: {why}")))
}

/// The contract's own terms as the release file's shared module names them: `route`, the manifest's route;
/// `ownerKeyHeader` and `ownerRoleHeader`, the headers the console relays the owner key and the role in; and
/// `roles`, each role held and each needed with whether the one held allows the one needed.
pub fn contract_terms(release: &Path) -> Result<Value, ReadingError> {
  let script = format!(
    "import {{ CONSOLE_MANIFEST_ROUTE, OWNER_KEY_HEADER, OWNER_ROLE_HEADER, CONSOLE_ROLES, roleAllows }} \
     from \"{SHARED_MODULE}\";\n\
     const roles = CONSOLE_ROLES.flatMap((held) => CONSOLE_ROLES.map((needed) => \
       ({{ held, needed, allows: roleAllows(held, needed) }})));\n\
     process.stdout.write(JSON.stringify({{ route: CONSOLE_MANIFEST_ROUTE, ownerKeyHeader: OWNER_KEY_HEADER, \
       ownerRoleHeader: OWNER_ROLE_HEADER, roles }}));\n"
  );
  run(release, &script, "")
}

// The script run by Bun in the release unpacked, the input handed on its standard input.
fn run(release: &Path, script: &str, input: &str) -> Result<Value, ReadingError> {
  let unpacked = Unpacked::new(release)?;
  unpacked.link_imports()?;
  let mut reading = Command::new("bun")
    .arg("--eval")
    .arg(script)
    .current_dir(unpacked.package())
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|why| ReadingError::Unrun(format!("Bun did not start: {why}")))?;
  if let Some(mut handed) = reading.stdin.take() {
    handed
      .write_all(input.as_bytes())
      .map_err(|why| ReadingError::Unrun(format!("the input was not handed over: {why}")))?;
  }
  let done = reading
    .wait_with_output()
    .map_err(|why| ReadingError::Unrun(format!("Bun did not end: {why}")))?;
  if !done.status.success() {
    return Err(ReadingError::Refused(String::from_utf8_lossy(&done.stderr).trim().to_owned()));
  }
  serde_json::from_slice(&done.stdout)
    .map_err(|why| ReadingError::Unrun(format!("the reading answered no JSON: {why}")))
}

/// The release file unpacked into a scratch folder of its own, removed when it is let go of.
struct Unpacked {
  folder: PathBuf,
}

impl Unpacked {
  fn new(release: &Path) -> Result<Self, ReadingError> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let folder = std::env::temp_dir().join(format!(
      "bmo-console-face-reading-{}-{}",
      std::process::id(),
      COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&folder)
      .map_err(|why| ReadingError::Unrun(format!("{} was not made: {why}", folder.display())))?;
    let unpacked = Self { folder };
    let done = Command::new("tar")
      .arg("--extract")
      .arg("--gzip")
      .arg("--file")
      .arg(release)
      .arg("--directory")
      .arg(&unpacked.folder)
      .stdin(Stdio::null())
      .output()
      .map_err(|why| ReadingError::Unrun(format!("tar did not start: {why}")))?;
    if !done.status.success() {
      return Err(ReadingError::Unrun(format!(
        "{} did not unpack: {}",
        release.display(),
        String::from_utf8_lossy(&done.stderr).trim()
      )));
    }
    Ok(unpacked)
  }

  fn package(&self) -> PathBuf {
    self.folder.join(PACKAGE_FOLDER)
  }

  /// Links the modules the shared module imports, installed once at the ranges the release declares, into the
  /// unpacked package, so Bun resolves them there and installs nothing on the fly.
  fn link_imports(&self) -> Result<(), ReadingError> {
    let package = self.package();
    let modules = Imports::declared(&package)?.installed()?;
    std::os::unix::fs::symlink(&modules, package.join(MODULES_FOLDER)).map_err(|why| {
      ReadingError::Unrun(format!("{} was not linked into the release: {why}", modules.display()))
    })
  }
}

impl Drop for Unpacked {
  fn drop(&mut self) {
    // A scratch folder left behind costs disk alone, so a failure to remove it is not raised.
    let _ = std::fs::remove_dir_all(&self.folder);
  }
}

/// The installing package's manifest: the packages the shared module imports, each at the range the release
/// declares.
struct Imports {
  manifest: String,
}

impl Imports {
  fn declared(package: &Path) -> Result<Self, ReadingError> {
    let path = package.join("package.json");
    let declared: Value = std::fs::read(&path)
      .map_err(|why| ReadingError::Unrun(format!("{} is not readable: {why}", path.display())))
      .and_then(|bytes| {
        serde_json::from_slice(&bytes)
          .map_err(|why| ReadingError::Unrun(format!("{} is no JSON: {why}", path.display())))
      })?;
    let mut dependencies = serde_json::Map::new();
    for name in IMPORTED {
      let range = declared["dependencies"][name]
        .as_str()
        .ok_or_else(|| ReadingError::Unrun(format!("the release declares no range of {name}")))?;
      dependencies.insert((*name).to_owned(), Value::from(range));
    }
    let manifest = serde_json::json!({
      "name": "bmo-console-face-reading",
      "private": true,
      "dependencies": dependencies,
    });
    Ok(Self {
      manifest: manifest.to_string(),
    })
  }

  /// The installed modules' folder, installed now where no reading of the same ranges installed it before.
  fn installed(&self) -> Result<PathBuf, ReadingError> {
    let digest = format!("{:x}", Sha256::digest(self.manifest.as_bytes()));
    let installed = std::env::temp_dir().join(format!("bmo-console-face-imports-{}", &digest[..16]));
    let modules = installed.join(MODULES_FOLDER);
    if modules.is_dir() {
      return Ok(modules);
    }
    // Installed beside the folder and renamed into it, so a reading running at once never sees it half made.
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let staging =
      installed.with_extension(format!("{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
    let done = Self::install(&staging, &self.manifest).and_then(|()| {
      std::fs::rename(&staging, &installed).or_else(|why| {
        if modules.is_dir() {
          Ok(())
        } else {
          Err(ReadingError::Unrun(format!("{} was not made: {why}", installed.display())))
        }
      })
    });
    // Still there only where another reading's install was renamed in first, or this one failed.
    let _ = std::fs::remove_dir_all(&staging);
    done.map(|()| modules)
  }

  /// Installs from Bun's cache alone first, and reaches the registry only for what the cache lacks, so a
  /// machine whose cache holds the packages installs them however its registry answers.
  fn install(folder: &Path, manifest: &str) -> Result<(), ReadingError> {
    std::fs::create_dir_all(folder)
      .and_then(|()| std::fs::write(folder.join("package.json"), manifest))
      .map_err(|why| ReadingError::Unrun(format!("{} was not made: {why}", folder.display())))?;
    let mut refusals = Vec::new();
    for reach in ["--offline", "--prefer-offline"] {
      let done = Command::new("bun")
        .args(["install", reach, "--production", "--ignore-scripts", "--no-progress"])
        .current_dir(folder)
        .stdin(Stdio::null())
        .output()
        .map_err(|why| ReadingError::Unrun(format!("Bun did not start: {why}")))?;
      if done.status.success() {
        return Ok(());
      }
      refusals.push(format!("{reach}: {}", String::from_utf8_lossy(&done.stderr).trim()));
    }
    Err(ReadingError::Unrun(format!(
      "the reading's imports did not install: {}",
      refusals.join("; ")
    )))
  }
}

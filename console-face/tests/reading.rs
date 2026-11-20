//! The reading holds on a machine whose temporary folder stands below a `node_modules` folder, where Bun drops
//! its install on the fly (`docs/console-face.md`, 「The reading through bmo-ui's release」).

use std::process::{Command, Stdio};

/// A test that runs one reading, run again by this one in a process of its own.
const READING_TEST: &str = "schema::the_carried_schema_is_the_pinned_release_s";

#[test]
fn the_reading_holds_with_a_node_modules_folder_above_its_scratch_folder() {
  let above = std::env::temp_dir().join(format!("bmo-console-face-above-{}", std::process::id()));
  let temporary = above.join("temporary");
  std::fs::create_dir_all(above.join("node_modules")).expect("the node_modules folder is made");
  std::fs::create_dir_all(&temporary).expect("the temporary folder is made");

  let done = Command::new(std::env::current_exe().expect("the test binary is known"))
    .args(["--exact", READING_TEST, "--test-threads=1"])
    .env("TMPDIR", &temporary)
    .env_remove("BMO_CONSOLE_FACE_WRITE_SCHEMA")
    .stdin(Stdio::null())
    .output()
    .expect("the test binary runs");
  let _ = std::fs::remove_dir_all(&above);

  let printed = String::from_utf8_lossy(&done.stdout);
  assert!(
    done.status.success() && printed.contains("1 passed"),
    "{READING_TEST} failed below a node_modules folder:\n{printed}\n{}",
    String::from_utf8_lossy(&done.stderr)
  );
}

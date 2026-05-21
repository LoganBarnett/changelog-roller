use std::{
  fs,
  path::PathBuf,
  process::{Command, Stdio},
  time::{SystemTime, UNIX_EPOCH},
};

fn get_binary_path() -> PathBuf {
  let mut path =
    std::env::current_exe().expect("Failed to get current executable path");

  // Navigate from the test executable to the binary.
  path.pop(); // remove test executable name
  path.pop(); // remove deps dir
  path.push("changelog-roller");

  if !path.exists() {
    path.pop();
    path.pop();
    path.push("debug");
    path.push("changelog-roller");
  }

  path
}

/// Builds a unique scratch path under the system temp directory.  Avoids
/// pulling in a dev-dependency for the integration tests since each test
/// only needs one file's worth of isolation.
fn unique_temp_path(prefix: &str) -> PathBuf {
  let nanos = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_nanos())
    .unwrap_or(0);
  let pid = std::process::id();
  std::env::temp_dir()
    .join(format!("changelog-roller-{}-{}-{}.org", prefix, pid, nanos))
}

fn write_fixture(path: &PathBuf, content: &str) {
  fs::write(path, content).expect("Failed to write fixture file");
}

#[test]
fn test_help_flag() {
  let output = Command::new(get_binary_path())
    .arg("--help")
    .output()
    .expect("Failed to execute binary");

  assert!(
    output.status.success(),
    "Expected success exit code, got: {:?}",
    output.status.code()
  );
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(
    stdout.contains("Usage:"),
    "Expected help text to contain 'Usage:', got: {}",
    stdout
  );
  assert!(
    stdout.contains("insert-item"),
    "Expected help text to advertise insert-item subcommand, got: {}",
    stdout
  );
}

#[test]
fn test_version_flag() {
  let output = Command::new(get_binary_path())
    .arg("--version")
    .output()
    .expect("Failed to execute binary");

  assert!(
    output.status.success(),
    "Expected success exit code, got: {:?}",
    output.status.code()
  );
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(
    stdout.contains("changelog-roller"),
    "Expected version text to contain 'changelog-roller', got: {}",
    stdout
  );
}

#[test]
fn test_no_subcommand_fails() {
  // The binary requires a subcommand; running it bare should fail.
  let output = Command::new(get_binary_path())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .expect("Failed to execute binary");

  assert!(
    !output.success(),
    "Expected non-zero exit code when no subcommand is given"
  );
}

#[test]
fn test_ready_to_roll_succeeds_when_upcoming_has_content() {
  let path = unique_temp_path("ready-yes");
  write_fixture(
    &path,
    "* changelog\n** Upcoming\n*** Additions\n1. New thing\n",
  );

  let output = Command::new(get_binary_path())
    .args(["--input-file", path.to_str().unwrap()])
    .arg("ready-to-roll")
    .output()
    .expect("Failed to execute binary");

  let _ = fs::remove_file(&path);

  assert!(
    output.status.success(),
    "Expected success, got status {:?}, stderr: {}",
    output.status.code(),
    String::from_utf8_lossy(&output.stderr)
  );
}

#[test]
fn test_ready_to_roll_fails_when_upcoming_empty() {
  let path = unique_temp_path("ready-no");
  write_fixture(&path, "* changelog\n** Upcoming\n*** Additions\n");

  let output = Command::new(get_binary_path())
    .args(["--input-file", path.to_str().unwrap()])
    .arg("ready-to-roll")
    .output()
    .expect("Failed to execute binary");

  let _ = fs::remove_file(&path);

  assert!(
    !output.status.success(),
    "Expected non-zero exit when upcoming has no content"
  );
}

#[test]
fn test_insert_item_writes_to_stdout() {
  let path = unique_temp_path("insert-stdout");
  let original = "* changelog\n** Upcoming\n*** Additions\n1. First thing\n";
  write_fixture(&path, original);

  let output = Command::new(get_binary_path())
    .args(["--input-file", path.to_str().unwrap()])
    .args([
      "insert-item",
      "--heading",
      "Additions",
      "--body",
      "Second thing",
    ])
    .output()
    .expect("Failed to execute binary");

  let on_disk =
    fs::read_to_string(&path).expect("Failed to read fixture after run");
  let _ = fs::remove_file(&path);

  assert!(
    output.status.success(),
    "Expected success, stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(
    stdout.contains("1. First thing"),
    "stdout missing original item, got: {}",
    stdout
  );
  assert!(
    stdout.contains("2. Second thing"),
    "stdout missing new item, got: {}",
    stdout
  );
  assert_eq!(
    on_disk, original,
    "input file must be untouched when --in-place is not given"
  );
}

#[test]
fn test_insert_item_in_place_modifies_file() {
  let path = unique_temp_path("insert-inplace");
  write_fixture(&path, "* changelog\n** Upcoming\n*** Additions\n");

  let output = Command::new(get_binary_path())
    .args(["--input-file", path.to_str().unwrap()])
    .arg("--in-place")
    .args([
      "insert-item",
      "--heading",
      "Additions",
      "--body",
      "Fresh item",
    ])
    .output()
    .expect("Failed to execute binary");

  let on_disk =
    fs::read_to_string(&path).expect("Failed to read fixture after run");
  let _ = fs::remove_file(&path);

  assert!(
    output.status.success(),
    "Expected success, stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(
    on_disk.contains("1. Fresh item"),
    "file should contain inserted item, got: {}",
    on_disk
  );
}

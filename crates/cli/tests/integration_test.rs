use std::{path::PathBuf, process::Command};

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

#[test]
fn test_help_flag() {
  let output = Command::new(get_binary_path()).arg("--help").output();

  match output {
    Ok(output) => {
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
    }
    Err(e) => {
      if e.kind() == std::io::ErrorKind::NotFound {
        eprintln!(
          "Binary not found. Build the project first with: cargo build -p changelog-roller"
        );
      }
      panic!("Failed to execute binary: {}", e);
    }
  }
}

#[test]
fn test_version_flag() {
  let output = Command::new(get_binary_path()).arg("--version").output();

  match output {
    Ok(output) => {
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
    Err(e) => {
      if e.kind() == std::io::ErrorKind::NotFound {
        eprintln!(
          "Binary not found. Build the project first with: cargo build -p changelog-roller"
        );
      }
      panic!("Failed to execute binary: {}", e);
    }
  }
}

#[test]
fn test_basic_execution() {
  // Running with no arguments should succeed (no --add-version means no-op).
  let output = Command::new(get_binary_path()).output();

  match output {
    Ok(output) => {
      assert!(
        output.status.success(),
        "Expected success exit code, got: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
      );
    }
    Err(e) => {
      if e.kind() == std::io::ErrorKind::NotFound {
        eprintln!(
          "Binary not found. Build the project first with: cargo build -p changelog-roller"
        );
      }
      panic!("Failed to execute binary: {}", e);
    }
  }
}

#[test]
fn test_ready_to_roll_requires_input_file() {
  // --ready-to-roll without --input-file should fail with a non-zero exit.
  let output = Command::new(get_binary_path())
    .arg("--ready-to-roll")
    .output();

  match output {
    Ok(output) => {
      assert!(
        !output.status.success(),
        "Expected non-zero exit code when --input-file is missing"
      );
    }
    Err(e) => {
      if e.kind() == std::io::ErrorKind::NotFound {
        eprintln!(
          "Binary not found. Build the project first with: cargo build -p changelog-roller"
        );
      }
      panic!("Failed to execute binary: {}", e);
    }
  }
}

//! Verify configuration errors are reported before terminal/device initialization.

use std::{fs, process::Command};

#[test]
fn invalid_override_exits_without_entering_the_terminal() {
    let directory = std::env::temp_dir().join(format!("tiny-wb-startup-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("custom.toml");
    fs::write(&path, "points = 1").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_tiny-wb"))
        .current_dir(&directory)
        .env("TINY_WB_CONFIG", &path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("custom.toml"));
    assert!(error.contains("points must be within"));
    assert!(!result.stdout.windows(2).any(|bytes| bytes == b"\x1b["));
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn malformed_csv_exits_without_entering_the_terminal_or_enumerating_ports() {
    let directory =
        std::env::temp_dir().join(format!("tiny-wb-csv-startup-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("broken.csv");
    fs::write(&path, "frequency_MHz,level_dBm\n240,not-a-level\n").unwrap();
    let config_path = directory.join("viewer.toml");
    fs::write(&config_path, "[logging]\ndirectory = 'not-a-directory'\n").unwrap();
    fs::write(directory.join("not-a-directory"), "ordinary file").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_tiny-wb"))
        .current_dir(&directory)
        .env("TINY_WB_CONFIG", config_path)
        .arg("--view")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("broken.csv"), "unexpected stderr: {error}");
    assert!(error.contains("line 2"), "unexpected stderr: {error}");
    assert!(!result.stdout.windows(2).any(|bytes| bytes == b"\x1b["));
    fs::remove_dir_all(directory).unwrap();
}

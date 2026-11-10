//! 端到端测试：真实二进制、真实文件系统、真实退出码。

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bakon"))
        .env_remove("BAKON_STORE")
        .env_remove("BAKON_HOME")
        .args(args)
        .output()
        .expect("failed to run bakon")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn version_reports_the_build_target() {
    let output = run(&["version"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).starts_with("bakon 1.0.0 ("),
        "stdout: {}",
        stdout(&output)
    );
}

#[test]
fn version_supports_json() {
    let output = run(&["--format", "json", "version"]);
    let text = stdout(&output);

    assert_eq!(output.status.code(), Some(0));
    assert!(text.contains("\"version\":\"1.0.0\""), "stdout: {text}");
    assert!(text.contains("\"target\":\""), "stdout: {text}");
}

#[test]
fn init_creates_a_store_and_is_idempotent() {
    let temporary = TempDir::new().unwrap();
    let store = temporary.path().join("store");
    let store_arg = store.to_str().unwrap();

    let first = run(&["init", "--store", store_arg]);
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    assert!(stdout(&first).starts_with("initialized "));
    assert!(store.join("BAKON_STORE").is_file());
    assert!(store.join("objects/tmp").is_dir());

    let second = run(&["init", "--store", store_arg]);
    assert_eq!(second.status.code(), Some(0), "stderr: {}", stderr(&second));
    assert!(stdout(&second).starts_with("already initialized: "));
}

#[test]
fn init_refuses_a_foreign_non_empty_directory() {
    let temporary = TempDir::new().unwrap();
    let store = temporary.path().join("store");
    std::fs::create_dir(&store).unwrap();
    std::fs::write(store.join("notes.txt"), b"someone else's data").unwrap();

    let output = run(&["init", "--store", store.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(6), "stdout: {}", stdout(&output));
    assert!(
        stderr(&output).contains("not a bakon store"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(!Path::new(&store).join("BAKON_STORE").exists());
}

#[test]
fn unknown_subcommands_exit_with_usage_code() {
    let output = run(&["frobnicate"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(!stderr(&output).is_empty());
}

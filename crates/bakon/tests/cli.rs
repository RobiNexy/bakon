//! 端到端测试：真实二进制、真实文件系统、真实退出码。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn run(args: &[&str]) -> Output {
    run_with_env(args, &[])
}

fn run_with_env(args: &[&str], environment: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bakon"));
    command
        .env_remove("BAKON_STORE")
        .env_remove("BAKON_HOME")
        .env_remove("VISUAL")
        .args(args);
    for (key, value) in environment {
        command.env(key, value);
    }
    command.output().expect("failed to run bakon")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// 写一个把内容替换为 `body` 的编辑器脚本；路径以 `$1` 传入。
fn editor_script(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("editor.sh");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

struct Fixture {
    _temporary: TempDir,
    store: PathBuf,
    file: PathBuf,
}

impl Fixture {
    fn new(content: &[u8]) -> Self {
        let temporary = TempDir::new().unwrap();
        let store = temporary.path().join("store");
        let file = temporary.path().join("note.txt");
        fs::write(&file, content).unwrap();
        Self {
            _temporary: temporary,
            store,
            file,
        }
    }

    fn store(&self) -> &str {
        self.store.to_str().unwrap()
    }

    fn file(&self) -> &str {
        self.file.to_str().unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut full = vec!["--store", self.store()];
        full.extend_from_slice(args);
        run(&full)
    }

    fn edit(&self, editor: &Path) -> Output {
        let mut full = vec!["--store", self.store(), "edit", self.file()];
        let output = run_with_env(&full, &[("EDITOR", editor)]);
        full.clear();
        output
    }
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
    fs::create_dir(&store).unwrap();
    fs::write(store.join("notes.txt"), b"someone else's data").unwrap();

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

#[test]
fn edit_tracks_then_records_the_editor_result() {
    let fixture = Fixture::new(b"first");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'second' > \"$1\"",
    );

    let output = fixture.edit(&editor);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("tracked"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("saved version 1"),
        "stderr: {}",
        stderr(&output)
    );

    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(log.status.code(), Some(0), "stderr: {}", stderr(&log));
    let log_text = stdout(&log);
    let lines: Vec<&str> = log_text.lines().collect();
    assert_eq!(lines.len(), 2, "stdout: {log_text}");
    assert!(
        lines[0].starts_with("1\t") && lines[0].ends_with("\tedit"),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("0\t") && lines[1].ends_with("\tbaseline"),
        "{}",
        lines[1]
    );

    let baseline = fixture.run(&["show", fixture.file(), "0"]);
    assert_eq!(baseline.stdout, b"first");

    let latest = fixture.run(&["show", fixture.file(), "1"]);
    assert_eq!(latest.stdout, b"second");
}

#[test]
fn edit_without_changes_creates_no_version() {
    let fixture = Fixture::new(b"unchanged");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");

    let output = fixture.edit(&editor);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("no changes"),
        "stderr: {}",
        stderr(&output)
    );
    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&log).lines().count(), 1);
}

#[test]
fn diff_exits_one_on_text_changes() {
    let fixture = Fixture::new(b"alpha\n");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'beta\\n' > \"$1\"",
    );
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file()]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("-alpha"), "stdout: {text}");
    assert!(text.contains("+beta"), "stdout: {text}");
}

#[test]
fn diff_with_identical_content_exits_zero() {
    let fixture = Fixture::new(b"same\n");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file(), "0", "0"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stdout(&output).is_empty());
}

#[test]
fn diff_reports_binary_files_without_printing_bytes() {
    let fixture = Fixture::new(b"a\0b");
    let editor = editor_script(
        Path::new(fixture.store()).parent().unwrap(),
        "printf 'c\\0d' > \"$1\"",
    );
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file()]);

    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("binary files differ"));
    assert!(stdout(&output).is_empty());
}

#[test]
fn dump_matches_show() {
    let fixture = Fixture::new(b"payload");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let shown = fixture.run(&["show", fixture.file(), "0"]);
    let dumped = fixture.run(&["dump", fixture.file(), "0"]);

    assert_eq!(shown.stdout, b"payload");
    assert_eq!(dumped.stdout, shown.stdout);
}

#[test]
fn log_on_an_untracked_file_exits_three() {
    let fixture = Fixture::new(b"never tracked");

    let output = fixture.run(&["log", fixture.file()]);

    assert_eq!(output.status.code(), Some(3), "stdout: {}", stdout(&output));
    assert!(
        stderr(&output).contains("not managed"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn showing_a_missing_version_exits_three() {
    let fixture = Fixture::new(b"content");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["show", fixture.file(), "9"]);

    assert_eq!(output.status.code(), Some(3), "stdout: {}", stdout(&output));
    assert!(
        stderr(&output).contains("does not exist"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn diff_with_a_single_version_exits_three() {
    let fixture = Fixture::new(b"only");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    assert_eq!(fixture.edit(&editor).status.code(), Some(0));

    let output = fixture.run(&["diff", fixture.file()]);

    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("no previous version"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn a_failing_editor_exits_five_and_keeps_the_baseline() {
    let fixture = Fixture::new(b"kept");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 7");

    let output = fixture.edit(&editor);

    assert_eq!(output.status.code(), Some(5), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("editor failed"),
        "stderr: {}",
        stderr(&output)
    );
    let log = fixture.run(&["--format", "plain", "log", fixture.file()]);
    assert_eq!(stdout(&log).lines().count(), 1);
}

#[test]
fn editing_a_directory_exits_three() {
    let fixture = Fixture::new(b"x");
    let editor = editor_script(Path::new(fixture.store()).parent().unwrap(), "exit 0");
    let directory = fixture.file.parent().unwrap().join("folder");
    fs::create_dir(&directory).unwrap();

    let directory = directory.to_str().unwrap();
    let output = run_with_env(
        &["--store", fixture.store(), "edit", directory],
        &[("EDITOR", &editor)],
    );

    assert_eq!(output.status.code(), Some(3), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("not a regular file"),
        "stderr: {}",
        stderr(&output)
    );
}

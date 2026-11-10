//! 崩溃注入：在任意持久化动作后终止进程，重开后断言存储可读、可修复、恢复幂等。
//!
//! `BAKON_TEST_CRASH_AFTER=n` 只在 debug 构建中生效；见 `bakon-core/src/fsutil.rs`。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const CRASH_EXIT: i32 = 137;

struct Case {
    temporary: TempDir,
    store: PathBuf,
    file: PathBuf,
    noop: PathBuf,
}

impl Case {
    fn new(content: &[u8]) -> Self {
        let temporary = TempDir::new().unwrap();
        let store = temporary.path().join("store");
        let file = temporary.path().join("note.txt");
        fs::write(&file, content).unwrap();
        let noop = write_editor(&temporary, "exit 0");
        Self {
            temporary,
            store,
            file,
            noop,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bakon"));
        command
            .env_remove("BAKON_STORE")
            .env_remove("BAKON_HOME")
            .env_remove("VISUAL")
            .env_remove("BAKON_TEST_CRASH_AFTER")
            .env(
                "BAKON_CONFIG",
                std::env::temp_dir().join("bakon-tests-no-config.toml"),
            )
            .args(["--store", self.store.to_str().unwrap()])
            .args(args);
        command
    }

    fn edit(&self, editor: &Path) -> Output {
        self.command(&["edit", self.file.to_str().unwrap()])
            .env("EDITOR", editor)
            .output()
            .unwrap()
    }

    fn crash_edit(&self, editor: &Path, limit: u64) -> Output {
        self.command(&["edit", self.file.to_str().unwrap()])
            .env("EDITOR", editor)
            .env("BAKON_TEST_CRASH_AFTER", limit.to_string())
            .output()
            .unwrap()
    }

    fn crash_revert(&self, version: u32, limit: u64) -> Output {
        self.command(&["revert", self.file.to_str().unwrap(), &version.to_string()])
            .env("BAKON_TEST_CRASH_AFTER", limit.to_string())
            .output()
            .unwrap()
    }

    fn track(&self) {
        let output = self.edit(&self.noop);
        assert_eq!(output.status.code(), Some(0), "track failed");
    }

    fn log(&self) -> String {
        let output = self.run(&["--format", "plain", "log", self.file.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(0), "store must stay readable");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn recover(&self) {
        let output = self.edit(&self.noop);
        assert_eq!(output.status.code(), Some(0), "recovery edit failed");

        let fixed = self.run(&["verify", "--fix"]);
        assert_eq!(fixed.status.code(), Some(0), "verify --fix failed");

        let clean = self.run(&["verify"]);
        assert_eq!(
            clean.status.code(),
            Some(0),
            "store not consistent after recovery"
        );

        let again = self.run(&["verify", "--fix"]);
        assert_eq!(again.status.code(), Some(0), "recovery must be idempotent");
    }

    fn editor(&self, body: &str) -> PathBuf {
        write_editor(&self.temporary, body)
    }
}

fn write_editor(temporary: &TempDir, body: &str) -> PathBuf {
    let path = temporary.path().join(format!("editor-{}.sh", body.len()));
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn edit_survives_a_crash_at_every_durability_point() {
    let mut observed = 0;
    for limit in 1..=20 {
        let case = Case::new(b"base");
        case.track();
        let changed = case.editor("printf 'changed' > \"$1\"");

        let crashed = case.crash_edit(&changed, limit);
        if crashed.status.code() != Some(CRASH_EXIT) {
            break;
        }
        observed += 1;
        case.log();
        case.recover();
    }
    assert!(
        observed >= 4,
        "expected several durability points, saw {observed}"
    );
}

#[test]
fn revert_survives_a_crash_at_every_durability_point() {
    let mut observed = 0;
    for limit in 1..=30 {
        let case = Case::new(b"one");
        case.track();
        let changed = case.editor("printf 'two' > \"$1\"");
        assert_eq!(case.edit(&changed).status.code(), Some(0));

        let crashed = case.crash_revert(0, limit);
        if crashed.status.code() != Some(CRASH_EXIT) {
            break;
        }
        observed += 1;

        let log = case.log();
        case.recover();

        let content = fs::read(&case.file).unwrap();
        assert!(content == b"one" || content == b"two", "unexpected content");
        if log.lines().any(|line| line.ends_with("\trevert")) {
            assert_eq!(
                content, b"one",
                "a recorded revert must be applied on recovery"
            );
        }
    }
    assert!(
        observed >= 8,
        "expected many durability points, saw {observed}"
    );
}

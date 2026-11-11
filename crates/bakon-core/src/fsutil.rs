//! 持久化的最小原语：目录 fsync 与原子替换。
//!
//! 只依赖两个平台事实：`rename` 原子、目录 `fsync` 在崩溃后让目录项可见。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::AbsPath;
use crate::{Error, Result};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// 创建目录；Unix 上新建目录权限为 0700。已存在的目录不改动权限。
pub(crate) fn create_private_dir_all(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;

        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path).map_err(Error::Io)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path).map_err(Error::Io)
    }
}

/// 私有文件的打开选项：Unix 上新建文件权限为 0600。调用方补充读写标志。
pub(crate) fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    options
}

/// 保证目录项本身已落盘；只 fsync 文件不足以让 `rename` 结果持久。
#[cfg(unix)]
fn fsync_dir_raw(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

/// Windows 没有等价的目录 fsync；崩溃恢复退化为可能丢失最后一次提交。
#[cfg(not(unix))]
fn fsync_dir_raw(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub(crate) fn fsync_dir(path: &Path) -> Result<()> {
    fsync_dir_raw(path).map_err(Error::Io)
}

/// 原子替换的结果。`rename` 成功即已提交且可见；目录 `fsync` 只决定持久性。
#[must_use]
#[derive(Debug)]
pub(crate) enum WriteOutcome {
    Durable,
    VisibleButNotDurable(io::Error),
}

impl WriteOutcome {
    /// 需要“未提交即失败”的调用方使用；已提交但持久性未确认也视为失败。
    pub(crate) fn require_durable(self) -> Result<()> {
        match self {
            Self::Durable => Ok(()),
            Self::VisibleButNotDurable(source) => Err(Error::Io(source)),
        }
    }
}

/// 原子替换：临时文件 → 写入 → fsync → rename → 目录 fsync。
///
/// 临时文件必须与最终路径同一文件系统；`tmp_dir` 通常就是最终文件的父目录，
/// 对象层例外地使用 `objects/tmp/`。`rename` 前失败返回 `Err` 并尽力删除临时文件；
/// `rename` 成功后返回 [`WriteOutcome`]，目录 `fsync` 失败只影响持久性，不撤销可见提交。
pub(crate) fn write_atomic(
    path: &AbsPath,
    tmp_dir: &AbsPath,
    bytes: &[u8],
) -> Result<WriteOutcome> {
    create_private_dir_all(tmp_dir.as_path())?;
    let temporary = temporary_path(tmp_dir, path);

    let mut file = private_file_options()
        .write(true)
        .create_new(true)
        .open(temporary.as_path())
        .map_err(Error::Io)?;

    if let Err(error) = write_and_sync(&mut file, bytes) {
        let _ = fs::remove_file(temporary.as_path());
        return Err(error);
    }
    drop(file);

    if let Err(error) = fs::rename(temporary.as_path(), path.as_path()).map_err(Error::Io) {
        let _ = fs::remove_file(temporary.as_path());
        return Err(error);
    }
    durability_point();

    if let Some(parent) = path.as_path().parent() {
        if let Some(error) = injected_fsync_failure(parent) {
            return Ok(WriteOutcome::VisibleButNotDurable(error));
        }
        if let Err(error) = fsync_dir_raw(parent) {
            return Ok(WriteOutcome::VisibleButNotDurable(error));
        }
        durability_point();
    }
    // 不 fsync 临时目录：崩溃残留的 tmp 文件没有引用，启动时会被清空；
    // 目标目录的 fsync 才决定 rename 结果是否可见。
    Ok(WriteOutcome::Durable)
}

/// 测试用故障注入：debug 构建下按目录名模拟父目录 `fsync` 失败。
/// 失败发生在 `rename` 之后，用于覆盖“已可见但持久性未确认”的分支。
#[cfg(debug_assertions)]
fn injected_fsync_failure(parent: &Path) -> Option<io::Error> {
    let requested = std::env::var("BAKON_TEST_FAIL_FSYNC_DIR").ok()?;
    let name = parent.file_name()?.to_str()?;
    (name == requested).then(|| io::Error::other("injected directory fsync failure"))
}

#[cfg(not(debug_assertions))]
fn injected_fsync_failure(_parent: &Path) -> Option<io::Error> {
    None
}

fn write_and_sync(file: &mut File, bytes: &[u8]) -> Result<()> {
    file.write_all(bytes).map_err(Error::Io)?;
    file.sync_all().map_err(Error::Io)?;
    durability_point();
    Ok(())
}

/// 测试用崩溃点：debug 构建下，`BAKON_TEST_CRASH_AFTER=n` 会在第 n 次持久化动作后
/// 终止进程，用于枚举崩溃位置。release 构建不编译这段逻辑。
#[cfg(debug_assertions)]
pub(crate) fn durability_point() {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNT: AtomicU64 = AtomicU64::new(0);
    static LIMIT: OnceLock<Option<u64>> = OnceLock::new();

    let limit = *LIMIT.get_or_init(|| {
        std::env::var("BAKON_TEST_CRASH_AFTER")
            .ok()
            .and_then(|value| value.parse().ok())
    });
    let Some(limit) = limit else {
        return;
    };
    let count = COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    if count == limit {
        // 模拟进程崩溃：不运行析构，但此前的 fsync 已生效。
        std::process::exit(137);
    }
}

#[cfg(not(debug_assertions))]
pub(crate) fn durability_point() {}

/// 临时文件名由目标文件名、pid、单调序号与随机数组成，避免并发写者互撞。
fn temporary_path(tmp_dir: &AbsPath, path: &AbsPath) -> AbsPath {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = path
        .as_path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("entry");
    tmp_dir.join(format!(
        "{name}.{}.{sequence}.{}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ))
}

/// 清空目录内容但保留目录本身；不存在的目录视为已清空。
pub(crate) fn remove_dir_contents(path: &Path) -> Result<()> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::Io(error)),
    };
    for entry in entries {
        let entry = entry.map_err(Error::Io)?;
        let file_type = entry.file_type().map_err(Error::Io)?;
        if file_type.is_dir() {
            fs::remove_dir_all(entry.path()).map_err(Error::Io)?;
        } else {
            fs::remove_file(entry.path()).map_err(Error::Io)?;
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn atomic_write_creates_private_directories_and_files() {
        let temporary = TempDir::new().unwrap();
        let root = AbsPath::new(temporary.path().join("store")).unwrap();
        let path = root.join("data.json");

        write_atomic(&path, &root, b"secret")
            .unwrap()
            .require_durable()
            .unwrap();

        assert_eq!(mode(root.as_path()), 0o700);
        assert_eq!(mode(path.as_path()), 0o600);
    }
}

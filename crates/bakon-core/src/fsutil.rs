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

/// 保证目录项本身已落盘；只 fsync 文件不足以让 `rename` 结果持久。
#[cfg(unix)]
pub(crate) fn fsync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(Error::Io)
}

/// Windows 没有等价的目录 fsync；崩溃恢复退化为可能丢失最后一次提交。
#[cfg(not(unix))]
pub(crate) fn fsync_dir(_path: &Path) -> Result<()> {
    Ok(())
}

/// 原子替换：临时文件 → 写入 → fsync → rename → 目录 fsync。
///
/// 临时文件必须与最终路径同一文件系统；`tmp_dir` 通常就是最终文件的父目录，
/// 对象层例外地使用 `objects/tmp/`。失败时临时文件会被尽力删除。
pub(crate) fn write_atomic(path: &AbsPath, tmp_dir: &AbsPath, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(tmp_dir.as_path()).map_err(Error::Io)?;
    let temporary = temporary_path(tmp_dir, path);

    let mut file = OpenOptions::new()
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
        fsync_dir(parent)?;
        durability_point();
    }
    // 不 fsync 临时目录：崩溃残留的 tmp 文件没有引用，启动时会被清空；
    // 目标目录的 fsync 才决定 rename 结果是否可见。
    Ok(())
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

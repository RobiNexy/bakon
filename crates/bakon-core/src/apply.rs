//! 目标文件写回策略。
//!
//! 0.x 用「同目录临时文件 + rename」，会把符号链接替换成普通文件并更换 inode。
//! v1 按目标类型分派，保留链接、属主、权限与扩展属性。策略见
//! `design/SEQUENCE__write-protocol-and-failure-paths.md` §4。

use std::fs::{self, OpenOptions};
use std::io::Write;

use crate::fsutil::fsync_dir;
use crate::types::AbsPath;
use crate::{Error, Result};

/// 把内容写回目标；只修改内容，不替换文件本身。
pub fn write_target(path: &AbsPath, content: &[u8]) -> Result<()> {
    match fs::metadata(path.as_path()) {
        Ok(metadata) if metadata.is_file() => write_existing(path, content),
        Ok(_) => Err(Error::NotAFile { path: path.clone() }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if is_dangling_symlink(path)? {
                return Err(Error::NotAFile { path: path.clone() });
            }
            create_new(path, content)
        }
        Err(error) => Err(Error::Io(error)),
    }
}

fn write_existing(path: &AbsPath, content: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path.as_path())
        .map_err(Error::Io)?;
    file.write_all(content).map_err(Error::Io)?;
    file.sync_all().map_err(Error::Io)?;
    crate::fsutil::durability_point();
    Ok(())
}

fn create_new(path: &AbsPath, content: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
    }

    let mut file = options.open(path.as_path()).map_err(Error::Io)?;
    file.write_all(content).map_err(Error::Io)?;
    file.sync_all().map_err(Error::Io)?;
    crate::fsutil::durability_point();
    drop(file);

    if let Some(parent) = path.as_path().parent() {
        fsync_dir(parent)?;
        crate::fsutil::durability_point();
    }
    Ok(())
}

/// 悬空符号链接不能按「不存在」创建，否则会覆盖用户的链接意图。
fn is_dangling_symlink(path: &AbsPath) -> Result<bool> {
    match fs::symlink_metadata(path.as_path()) {
        Ok(metadata) => Ok(metadata.file_type().is_symlink()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(Error::Io(error)),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn rewrites_an_existing_file_in_place() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("file");
        fs::write(&path, b"old").unwrap();
        #[cfg(unix)]
        let inode_before = std::os::unix::fs::MetadataExt::ino(&fs::metadata(&path).unwrap());

        write_target(&AbsPath::new(&path).unwrap(), b"new").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let inode_after = fs::metadata(&path).unwrap().ino();
            assert_eq!(inode_before, inode_after, "write-back must keep the inode");
        }
    }

    #[test]
    fn creates_a_missing_file() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("new");

        write_target(&AbsPath::new(&path).unwrap(), b"content").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"content");
    }

    #[cfg(unix)]
    #[test]
    fn follows_a_symlink_and_keeps_it() {
        let temporary = TempDir::new().unwrap();
        let target = temporary.path().join("target");
        let link = temporary.path().join("link");
        fs::write(&target, b"old").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_target(&AbsPath::new(&link).unwrap(), b"new").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_dangling_symlink() {
        let temporary = TempDir::new().unwrap();
        let link = temporary.path().join("dangling");
        std::os::unix::fs::symlink(temporary.path().join("missing"), &link).unwrap();

        let error = write_target(&AbsPath::new(&link).unwrap(), b"content").unwrap_err();

        assert!(matches!(error, Error::NotAFile { .. }));
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn refuses_a_directory() {
        let temporary = TempDir::new().unwrap();
        let error = write_target(&AbsPath::new(temporary.path()).unwrap(), b"x").unwrap_err();
        assert!(matches!(error, Error::NotAFile { .. }));
    }
}

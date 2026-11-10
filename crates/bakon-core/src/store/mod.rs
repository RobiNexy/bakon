//! 存储打开与初始化。
//!
//! 打开规则严格按 `design/OPS__store-identity-recovery-and-retention.md` §1 执行：
//! 不存在的目录会被创建；非空且无标记的目录被拒绝，避免写入他人的数据。

mod marker;

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use crate::types::AbsPath;
use crate::{Error, Result};

pub use marker::{MARKER_FILE, Marker, SUPPORTED_SCHEMA};

/// 存储创建时必须存在的子目录。
const LAYOUT_DIRS: [&str; 4] = ["files", "objects", "objects/tmp", "trash"];

/// 已打开并校验过的存储。
#[derive(Debug)]
pub struct Store {
    root: AbsPath,
    marker: Marker,
}

impl Store {
    /// 打开存储；不存在或为空时初始化。返回 `(store, created)`。
    pub fn init(root: impl AsRef<Path>) -> Result<(Self, bool)> {
        let root = AbsPath::new(root)?;
        let marker_path = root.as_path().join(MARKER_FILE);

        match fs::metadata(root.as_path()) {
            Ok(metadata) if !metadata.is_dir() => Err(Error::Io(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("not a directory: {}", root.as_path().display()),
            ))),
            Ok(_) if marker_path.is_file() => {
                let marker = read_marker(&marker_path)?;
                marker.validate()?;
                ensure_layout(&root)?;
                Ok((Self { root, marker }, false))
            }
            Ok(_) if is_empty_dir(&root)? => {
                let marker = initialize(&root)?;
                Ok((Self { root, marker }, true))
            }
            Ok(_) => Err(Error::NotAStore { root }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(root.as_path()).map_err(Error::Io)?;
                let marker = initialize(&root)?;
                Ok((Self { root, marker }, true))
            }
            Err(error) => Err(Error::Io(error)),
        }
    }

    /// 打开已存在的存储；不关心是否刚刚创建。
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::init(root).map(|(store, _)| store)
    }

    pub fn root(&self) -> &AbsPath {
        &self.root
    }

    pub fn marker(&self) -> &Marker {
        &self.marker
    }
}

/// 原子写入标记：临时文件 → fsync → rename → 目录 fsync。
fn initialize(root: &AbsPath) -> Result<Marker> {
    let marker = Marker::new()?;
    let encoded = serde_json::to_vec_pretty(&marker)
        .map_err(|error| Error::Internal(format!("cannot encode store marker: {error}")))?;

    let temporary = root.join(format!(".{MARKER_FILE}.tmp.{}", std::process::id()));
    let final_path = root.join(MARKER_FILE);

    let mut file = File::create(temporary.as_path()).map_err(Error::Io)?;
    file.write_all(&encoded).map_err(Error::Io)?;
    file.write_all(b"\n").map_err(Error::Io)?;
    file.sync_all().map_err(Error::Io)?;
    drop(file);
    fs::rename(temporary.as_path(), final_path.as_path()).map_err(Error::Io)?;
    fsync_dir(root.as_path())?;

    ensure_layout(root)?;
    Ok(marker)
}

/// 创建缺失的子目录；已有的目录保持不变。
fn ensure_layout(root: &AbsPath) -> Result<()> {
    for directory in LAYOUT_DIRS {
        fs::create_dir_all(root.join(directory).as_path()).map_err(Error::Io)?;
    }
    fsync_dir(root.as_path())
}

fn is_empty_dir(root: &AbsPath) -> Result<bool> {
    let mut entries = fs::read_dir(root.as_path()).map_err(Error::Io)?;
    Ok(entries.next().is_none())
}

fn read_marker(path: &Path) -> Result<Marker> {
    let bytes = fs::read(path).map_err(Error::Io)?;
    serde_json::from_slice(&bytes).map_err(|error| Error::Corrupt {
        what: "store marker",
        detail: error.to_string(),
    })
}

#[cfg(unix)]
fn fsync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(Error::Io)
}

/// Windows 没有等价的目录 fsync；崩溃恢复退化为可能丢失最后一次提交。
#[cfg(not(unix))]
fn fsync_dir(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    fn marker_json(schema: u32) -> String {
        serde_json::json!({
            "schema": schema,
            "uuid": "00000000-0000-4000-8000-000000000000",
            "created": "2026-11-10T12:00:00Z",
            "tool": "bakon/1.0"
        })
        .to_string()
    }

    fn rewrite_marker(store: &Path, schema: u32) {
        fs::write(store.join(MARKER_FILE), marker_json(schema)).unwrap();
    }

    #[test]
    fn init_creates_marker_and_layout() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");

        let (store, created) = Store::init(&root).unwrap();

        assert!(created);
        assert!(store.root().as_path().join(MARKER_FILE).is_file());
        for directory in LAYOUT_DIRS {
            assert!(
                store.root().as_path().join(directory).is_dir(),
                "missing {directory}"
            );
        }
        assert_eq!(store.marker().schema, SUPPORTED_SCHEMA);
        store.marker().validate().unwrap();
    }

    #[test]
    fn init_is_idempotent() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");

        let (_, created) = Store::init(&root).unwrap();
        let (_, created_again) = Store::init(&root).unwrap();

        assert!(created);
        assert!(!created_again);
    }

    #[test]
    fn empty_existing_directory_gets_a_marker() {
        let temporary = TempDir::new().unwrap();
        let (_, created) = Store::init(temporary.path()).unwrap();

        assert!(created);
        assert!(temporary.path().join(MARKER_FILE).is_file());
    }

    #[test]
    fn non_empty_directory_without_marker_is_refused() {
        let temporary = TempDir::new().unwrap();
        fs::write(temporary.path().join("notes.txt"), b"not a store").unwrap();

        let error = Store::init(temporary.path()).unwrap_err();

        assert!(matches!(error, Error::NotAStore { .. }));
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn newer_schema_is_refused() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        Store::init(&root).unwrap();
        rewrite_marker(&root, SUPPORTED_SCHEMA + 1);

        let error = Store::init(&root).unwrap_err();

        assert!(
            matches!(error, Error::SchemaTooNew { found, .. } if found == SUPPORTED_SCHEMA + 1)
        );
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn older_schema_is_refused() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        Store::init(&root).unwrap();
        rewrite_marker(&root, SUPPORTED_SCHEMA - 1);

        let error = Store::init(&root).unwrap_err();

        assert!(
            matches!(error, Error::SchemaTooOld { found, .. } if found == SUPPORTED_SCHEMA - 1)
        );
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn corrupt_marker_is_refused() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        Store::init(&root).unwrap();
        fs::write(root.join(MARKER_FILE), b"{").unwrap();

        let error = Store::init(&root).unwrap_err();

        assert!(matches!(error, Error::Corrupt { .. }));
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn a_file_in_place_of_the_store_is_an_io_error() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("store");
        fs::write(&root, b"file").unwrap();

        let error = Store::init(&root).unwrap_err();

        assert!(matches!(error, Error::Io(_)));
        assert_eq!(error.exit_code(), 8);
    }
}

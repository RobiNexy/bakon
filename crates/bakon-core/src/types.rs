//! 核心类型：绝对路径、文件身份、版本号与内容哈希。
//!
//! 四个 newtype 之间没有隐式转换（也刻意不提供 `FromStr`），编译器因此能阻止
//! 把路径、身份、版本号与哈希混用。约束见 `design/API__store-contract-and-cli.md` §1。

use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::{Error, Result};

/// 规范化绝对路径：构造时绝对化并做词法清理，不解析符号链接。
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AbsPath(PathBuf);

impl AbsPath {
    /// 把任意路径绝对化并清理 `.`/`..`；不接触文件系统（`current_dir` 除外）。
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_err(Error::Io)?.join(path)
        };
        let cleaned = clean(&absolute);
        if !cleaned.is_absolute() {
            return Err(Error::Internal(format!(
                "path is not absolute after cleaning: {}",
                cleaned.display()
            )));
        }
        Ok(Self(cleaned))
    }

    /// 以本路径为根拼接相对路径。
    pub fn join(&self, relative: impl AsRef<Path>) -> Self {
        Self(clean(&self.0.join(relative.as_ref())))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for AbsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl AsRef<Path> for AbsPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// 词法清理：去掉 `.`，折叠 `..`，不解析符号链接。
fn clean(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => cleaned.push(prefix.as_os_str()),
            Component::RootDir => cleaned.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                cleaned.pop();
            }
            Component::Normal(part) => cleaned.push(part),
        }
    }
    cleaned
}

/// 文件身份：纳管时随机生成，与路径无关。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId([u8; 16]);

impl FileId {
    /// 生成新的随机身份；调用方负责保证存储内唯一。
    pub fn generate() -> Self {
        Self(uuid::v4_bytes())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&uuid::format(&self.0))
    }
}

/// 单调版本号，只能由 [`Ver::next`] 递增产生。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ver(u32);

impl Ver {
    /// 构造一个已知版本号；0 不是合法版本。
    pub fn new(value: u32) -> Result<Self> {
        if value == 0 {
            return Err(Error::Internal("version numbers start at 1".into()));
        }
        Ok(Self(value))
    }

    /// 唯一的递增方式。
    pub fn next(head: Self) -> Result<Self> {
        head.0
            .checked_add(1)
            .map(Self)
            .ok_or_else(|| Error::Internal("version number overflow".into()))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Ver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 内容哈希（SHA-256）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlobHash([u8; 32]);

impl BlobHash {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

pub(crate) mod uuid {
    /// 生成 UUID v4 字节串（设置版本与变体位）。
    pub(crate) fn v4_bytes() -> [u8; 16] {
        let mut bytes: [u8; 16] = rand::random();
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        bytes
    }

    /// 格式化为 8-4-4-4-12 的小写十六进制。
    pub(crate) fn format(bytes: &[u8; 16]) -> String {
        let mut out = String::with_capacity(36);
        for (index, byte) in bytes.iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                out.push('-');
            }
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    /// 校验 8-4-4-4-12 的小写十六进制形式。
    pub(crate) fn is_valid(text: &str) -> bool {
        if text.len() != 36 {
            return false;
        }
        text.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_dot_and_parent_components() {
        let path = AbsPath::new("/tmp/a/./b/../c").unwrap();
        assert_eq!(path.as_path(), Path::new("/tmp/a/c"));
    }

    #[test]
    fn parent_at_root_stays_at_root() {
        let path = AbsPath::new("/../..").unwrap();
        assert_eq!(path.as_path(), Path::new("/"));
    }

    #[test]
    fn relative_paths_become_absolute() {
        let path = AbsPath::new("relative/dir").unwrap();
        assert!(path.as_path().is_absolute());
        assert!(path.as_path().ends_with("relative/dir"));
    }

    #[test]
    fn join_cleans_the_result() {
        let root = AbsPath::new("/tmp/store").unwrap();
        let joined = root.join("objects/../files/x.json");
        assert_eq!(joined.as_path(), Path::new("/tmp/store/files/x.json"));
    }

    #[test]
    fn file_ids_are_unique_and_formatted_as_uuid_v4() {
        let first = FileId::generate();
        let second = FileId::generate();
        assert_ne!(first, second);
        let text = first.to_string();
        assert!(uuid::is_valid(&text), "unexpected format: {text}");
        assert_eq!(text.as_bytes()[14], b'4');
    }

    #[test]
    fn versions_only_grow() {
        let head = Ver::new(7).unwrap();
        assert_eq!(Ver::next(head).unwrap().get(), 8);
        assert!(Ver::new(0).is_err());
        assert!(Ver::next(Ver::new(u32::MAX).unwrap()).is_err());
    }

    #[test]
    fn blob_hashes_are_lowercase_hex() {
        let hash = BlobHash::from_bytes([0xab; 32]);
        let text = hash.to_string();
        assert_eq!(text.len(), 64);
        assert!(
            text.bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
    }
}

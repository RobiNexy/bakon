//! 核心类型：绝对路径、文件身份、版本号与内容哈希。
//!
//! 四个 newtype 之间没有隐式转换，编译器因此能阻止把路径、身份、版本号与哈希
//! 混用。约束见 `design/API__store-contract-and-cli.md` §1。

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
        if cleaned.to_str().is_none() {
            return Err(Error::Usage(
                "path is not valid UTF-8; bakon stores paths as UTF-8 strings".into(),
            ));
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

impl Serialize for AbsPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let text = self
            .0
            .to_str()
            .ok_or_else(|| serde::ser::Error::custom("path is not valid UTF-8"))?;
        serializer.serialize_str(text)
    }
}

impl<'de> Deserialize<'de> for AbsPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if !Path::new(&text).is_absolute() {
            return Err(serde::de::Error::custom("stored path is not absolute"));
        }
        Self::new(&text).map_err(serde::de::Error::custom)
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

    /// 32 位小写十六进制；清单文件以它命名。
    pub fn to_hex(self) -> String {
        hex_encode(&self.0)
    }

    /// 从 32 位小写十六进制解析；仅用于清单文件名与反序列化。
    pub(crate) fn from_hex(text: &str) -> Option<Self> {
        hex_decode::<16>(text).map(Self)
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex_encode(&self.0))
    }
}

impl Serialize for FileId {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for FileId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_hex(&text).ok_or_else(|| serde::de::Error::custom("invalid file id"))
    }
}

/// 单调版本号。首个版本是 0；此后只能由 [`Ver::next`] 递增产生。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ver(u32);

impl Ver {
    /// 首个版本号。
    pub const ZERO: Self = Self(0);

    /// 由外部输入（命令行参数或反序列化）构造；不作为生成新版本的入口。
    pub fn new(value: u32) -> Result<Self> {
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

impl Serialize for Ver {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for Ver {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = u32::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
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

    pub fn to_hex(self) -> String {
        hex_encode(&self.0)
    }

    pub(crate) fn from_hex(text: &str) -> Option<Self> {
        hex_decode::<32>(text).map(Self)
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex_encode(&self.0))
    }
}

impl Serialize for BlobHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex_encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for BlobHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_hex(&text).ok_or_else(|| serde::de::Error::custom("invalid blob hash"))
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn hex_decode<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    let mut bytes = [0_u8; N];
    let (pairs, _remainder) = text.as_bytes().as_chunks::<2>();
    for (index, pair) in pairs.iter().enumerate() {
        let pair = std::str::from_utf8(pair).ok()?;
        bytes[index] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(bytes)
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
    fn file_ids_are_unique_and_formatted_as_hex() {
        let first = FileId::generate();
        let second = FileId::generate();
        assert_ne!(first, second);
        assert_eq!(first.to_string().len(), 32);
        assert!(
            first
                .to_string()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        assert_eq!(FileId::from_hex(&first.to_string()), Some(first));
    }

    #[test]
    fn versions_start_at_zero_and_only_grow() {
        let zero = Ver::new(0).unwrap();
        assert_eq!(zero.get(), 0);
        assert_eq!(Ver::next(zero).unwrap().get(), 1);
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

    #[test]
    fn ids_and_hashes_round_trip_through_json() {
        let id = FileId::generate();
        let text = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<FileId>(&text).unwrap(), id);

        let hash = BlobHash::from_bytes([7; 32]);
        let text = serde_json::to_string(&hash).unwrap();
        assert_eq!(serde_json::from_str::<BlobHash>(&text).unwrap(), hash);
    }
}

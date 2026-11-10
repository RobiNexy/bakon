//! 对象层：内容寻址的不可变字节串。
//!
//! 契约见 `design/DATA__blob-store-and-durability.md`。对外只有两个操作：
//! [`ObjectStore::put`] 与 [`ObjectStore::get`]，两者都幂等且不依赖历史。

use std::fs;
use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use sha2::{Digest, Sha256};

use crate::fsutil::{remove_dir_contents, write_atomic};
use crate::types::{AbsPath, BlobHash};
use crate::{Error, Result};

/// 压缩级别固定，保证同一输入产生同一字节串（去重依赖这一点）。
/// 取最快级别：1 MB 提交的预算由 fsync 与哈希决定，压缩不应成为瓶颈。
const COMPRESSION: Compression = Compression::fast();

/// 对象库的根：`<store>/objects/`。
#[derive(Debug)]
pub struct ObjectStore {
    root: AbsPath,
    temporary: AbsPath,
}

impl ObjectStore {
    pub fn new(store_root: &AbsPath) -> Self {
        let root = store_root.join("objects");
        let temporary = root.join("tmp");
        Self { root, temporary }
    }

    /// 写入对象并返回内容哈希；相同内容重复写入是零操作。
    pub fn put(&self, plain: &[u8]) -> Result<BlobHash> {
        let hash = hash_bytes(plain);
        let path = self.path(hash);
        if path.as_path().exists() {
            return Ok(hash);
        }

        let compressed = deflate(plain)?;
        if let Some(shard) = path.as_path().parent() {
            fs::create_dir_all(shard).map_err(Error::Io)?;
            crate::fsutil::fsync_dir(self.root.as_path())?;
        }
        write_atomic(&path, &self.temporary, &compressed)?;
        Ok(hash)
    }

    /// 读取并校验对象；缺失、解压失败或哈希不符都视为存储损坏。
    pub fn get(&self, hash: BlobHash) -> Result<Vec<u8>> {
        let path = self.path(hash);
        let compressed = fs::read(path.as_path()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::Corrupt {
                    what: "object",
                    detail: format!("{hash} is missing"),
                }
            } else {
                Error::Io(error)
            }
        })?;

        let mut decoder = DeflateDecoder::new(compressed.as_slice());
        let mut plain = Vec::new();
        decoder
            .read_to_end(&mut plain)
            .map_err(|error| Error::Corrupt {
                what: "object",
                detail: format!("{hash} cannot be inflated: {error}"),
            })?;

        if hash_bytes(&plain) != hash {
            return Err(Error::Corrupt {
                what: "object",
                detail: format!("{hash} does not match its content"),
            });
        }
        Ok(plain)
    }

    /// 对象路径：`objects/aa/<rest>`。
    pub fn path(&self, hash: BlobHash) -> AbsPath {
        let text = hash.to_hex();
        self.root.join(format!("{}/{}", &text[..2], &text[2..]))
    }

    /// 清空写入暂存区；只能在持排他锁时调用。
    pub fn cleanup_tmp(&self) -> Result<()> {
        remove_dir_contents(self.temporary.as_path())
    }
}

/// 计算内容哈希，不写对象。
pub fn hash_bytes(plain: &[u8]) -> BlobHash {
    let mut hasher = Sha256::new();
    hasher.update(plain);
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&digest);
    BlobHash::from_bytes(bytes)
}

fn deflate(plain: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = DeflateEncoder::new(Vec::new(), COMPRESSION);
    encoder.write_all(plain).map_err(Error::Io)?;
    encoder.finish().map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn store() -> (TempDir, ObjectStore) {
        let temporary = TempDir::new().unwrap();
        let root = AbsPath::new(temporary.path()).unwrap();
        let objects = ObjectStore::new(&root);
        (temporary, objects)
    }

    #[test]
    fn round_trips_binary_content() {
        let (_temporary, objects) = store();
        let content = b"\x00\x01\xff binary \n";
        let hash = objects.put(content).unwrap();
        assert_eq!(objects.get(hash).unwrap(), content);
    }

    #[test]
    fn empty_content_is_a_valid_object() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"").unwrap();
        assert_eq!(objects.get(hash).unwrap(), b"");
    }

    #[test]
    fn same_content_is_stored_once() {
        let (_temporary, objects) = store();
        let first = objects.put(b"duplicate").unwrap();
        let second = objects.put(b"duplicate").unwrap();
        assert_eq!(first, second);

        let path = objects.path(first);
        let shard = path.as_path().parent().unwrap();
        let entries = fs::read_dir(shard).unwrap().count();
        assert_eq!(entries, 1);
    }

    #[test]
    fn different_content_gets_different_hashes() {
        let (_temporary, objects) = store();
        assert_ne!(objects.put(b"one").unwrap(), objects.put(b"two").unwrap());
    }

    #[test]
    fn a_missing_object_is_corruption() {
        let (_temporary, objects) = store();
        let error = objects.get(hash_bytes(b"absent")).unwrap_err();
        assert!(matches!(error, Error::Corrupt { .. }));
        assert_eq!(error.exit_code(), 6);
    }

    #[test]
    fn a_truncated_object_is_corruption() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"truncate me").unwrap();
        let path = objects.path(hash);
        let bytes = fs::read(path.as_path()).unwrap();
        fs::write(path.as_path(), &bytes[..bytes.len() / 2]).unwrap();

        assert!(matches!(objects.get(hash), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn a_flipped_byte_is_corruption() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"the quick brown fox").unwrap();
        let path = objects.path(hash);
        let mut bytes = fs::read(path.as_path()).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(path.as_path(), bytes).unwrap();

        assert!(matches!(objects.get(hash), Err(Error::Corrupt { .. })));
    }

    #[test]
    fn cleanup_removes_only_temporary_entries() {
        let (_temporary, objects) = store();
        let hash = objects.put(b"keep me").unwrap();
        fs::create_dir_all(objects.temporary.as_path()).unwrap();
        fs::write(objects.temporary.as_path().join("leftover.tmp"), b"x").unwrap();

        objects.cleanup_tmp().unwrap();

        assert_eq!(objects.get(hash).unwrap(), b"keep me");
        assert_eq!(
            fs::read_dir(objects.temporary.as_path()).unwrap().count(),
            0
        );
    }
}

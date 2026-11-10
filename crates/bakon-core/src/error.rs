//! 错误类型与退出码映射。
//!
//! 退出码是永久契约：调用点只能处理 [`Error`]，不得依赖错误文本。映射规则见
//! `design/SEQUENCE__write-protocol-and-failure-paths.md` §8。

use std::fmt;
use std::io;
use std::time::Duration;

use crate::types::{AbsPath, Ver};

/// 核心库的统一错误类型。
#[derive(Debug)]
pub enum Error {
    /// 命令行用法错误；通常由 CLI 层在解析阶段产生。
    Usage(String),
    /// 路径未被纳管。
    NotManaged(AbsPath),
    /// 目标文件不存在。
    NotFound(AbsPath),
    /// 请求的版本号不存在。
    NoSuchVersion { ver: Ver, latest: Ver },
    /// 请求的版本已被裁剪。
    Pruned { ver: Ver, oldest: Ver },
    /// 等待存储锁超时。
    LockTimeout {
        holder: Option<u32>,
        waited: Duration,
    },
    /// 目标文件偏离记录版本。
    Diverged { path: AbsPath, hint: &'static str },
    /// 编辑期间 head 被其他写者推进。
    HeadChanged { expected: Ver, actual: Ver },
    /// 钩子执行失败。
    HookFailed { ver: Ver, status: i32 },
    /// 目标目录存在且非空，但不是 bakon 存储。
    NotAStore { root: AbsPath },
    /// 存储 schema 高于本程序支持的版本。
    SchemaTooNew { found: u32, supported: u32 },
    /// 存储 schema 低于本程序支持的版本，需要迁移。
    SchemaTooOld { found: u32, supported: u32 },
    /// 持久数据损坏。
    Corrupt { what: &'static str, detail: String },
    /// 版本已记录，但写回目标文件失败。
    ApplyFailed { ver: Ver, source: io::Error },
    /// 环境或权限导致的 I/O 失败。
    Io(io::Error),
    /// 不应发生的内部不一致。
    Internal(String),
}

impl Error {
    /// 唯一的错误到退出码映射入口。0（成功）与 1（diff 有差异）不属于错误。
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            Self::NotManaged(_)
            | Self::NotFound(_)
            | Self::NoSuchVersion { .. }
            | Self::Pruned { .. } => 3,
            Self::LockTimeout { .. } | Self::Diverged { .. } | Self::HeadChanged { .. } => 4,
            Self::HookFailed { .. } => 5,
            Self::NotAStore { .. }
            | Self::SchemaTooNew { .. }
            | Self::SchemaTooOld { .. }
            | Self::Corrupt { .. } => 6,
            Self::ApplyFailed { .. } => 7,
            Self::Io(_) => 8,
            Self::Internal(_) => 70,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}"),
            Self::NotManaged(path) => write!(f, "not managed: {}", path.as_path().display()),
            Self::NotFound(path) => write!(f, "no such file: {}", path.as_path().display()),
            Self::NoSuchVersion { ver, latest } => {
                write!(
                    f,
                    "version {} does not exist (latest is {latest})",
                    ver.get()
                )
            }
            Self::Pruned { ver, oldest } => {
                write!(
                    f,
                    "version {} was pruned (oldest is {})",
                    ver.get(),
                    oldest.get()
                )
            }
            Self::LockTimeout { holder, waited } => {
                write!(f, "timed out after {waited:?} waiting for the store lock")?;
                if let Some(pid) = holder {
                    write!(f, " (held by pid {pid})")?;
                }
                Ok(())
            }
            Self::Diverged { path, hint } => {
                write!(
                    f,
                    "{} diverged from the recorded version: {hint}",
                    path.as_path().display()
                )
            }
            Self::HeadChanged { expected, actual } => write!(
                f,
                "head changed from {} to {} during the edit; re-run the command",
                expected.get(),
                actual.get()
            ),
            Self::HookFailed { ver, status } => {
                write!(
                    f,
                    "hook failed with status {status} after committing version {}",
                    ver.get()
                )
            }
            Self::NotAStore { root } => write!(
                f,
                "{} is not a bakon store (missing {marker})",
                root.as_path().display(),
                marker = crate::store::MARKER_FILE
            ),
            Self::SchemaTooNew { found, supported } => write!(
                f,
                "store schema {found} is newer than supported schema {supported}; upgrade bakon"
            ),
            Self::SchemaTooOld { found, supported } => write!(
                f,
                "store schema {found} is older than supported schema {supported}; run the migration"
            ),
            Self::Corrupt { what, detail } => write!(f, "corrupt {what}: {detail}"),
            Self::ApplyFailed { ver, source } => write!(
                f,
                "failed to write version {} back to the target file: {source}",
                ver.get()
            ),
            Self::Io(source) => write!(f, "I/O error: {source}"),
            Self::Internal(message) => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            Self::ApplyFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        Self::Io(source)
    }
}

/// 核心库统一使用的结果类型。
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AbsPath, Ver};

    fn path() -> AbsPath {
        AbsPath::new("/tmp/bakon-test").unwrap()
    }

    #[test]
    fn every_error_maps_to_a_stable_exit_code() {
        let cases: Vec<(Error, u8)> = vec![
            (Error::Usage("bad flag".into()), 2),
            (Error::NotManaged(path()), 3),
            (Error::NotFound(path()), 3),
            (
                Error::NoSuchVersion {
                    ver: Ver::new(9).unwrap(),
                    latest: Ver::new(3).unwrap(),
                },
                3,
            ),
            (
                Error::Pruned {
                    ver: Ver::new(1).unwrap(),
                    oldest: Ver::new(2).unwrap(),
                },
                3,
            ),
            (
                Error::LockTimeout {
                    holder: None,
                    waited: Duration::from_secs(2),
                },
                4,
            ),
            (
                Error::Diverged {
                    path: path(),
                    hint: "changed",
                },
                4,
            ),
            (
                Error::HeadChanged {
                    expected: Ver::new(1).unwrap(),
                    actual: Ver::new(2).unwrap(),
                },
                4,
            ),
            (
                Error::HookFailed {
                    ver: Ver::new(1).unwrap(),
                    status: 1,
                },
                5,
            ),
            (Error::NotAStore { root: path() }, 6),
            (
                Error::SchemaTooNew {
                    found: 2,
                    supported: 1,
                },
                6,
            ),
            (
                Error::SchemaTooOld {
                    found: 0,
                    supported: 1,
                },
                6,
            ),
            (
                Error::Corrupt {
                    what: "marker",
                    detail: "bad".into(),
                },
                6,
            ),
            (
                Error::ApplyFailed {
                    ver: Ver::new(1).unwrap(),
                    source: io::Error::other("denied"),
                },
                7,
            ),
            (Error::Io(io::Error::other("denied")), 8),
            (Error::Internal("bug".into()), 70),
        ];

        for (error, expected) in cases {
            assert_eq!(error.exit_code(), expected, "wrong exit code for {error}");
        }
    }

    #[test]
    fn success_and_diff_codes_are_never_errors() {
        let error = Error::Internal("bug".into());
        assert!(!matches!(error.exit_code(), 0 | 1));
    }
}

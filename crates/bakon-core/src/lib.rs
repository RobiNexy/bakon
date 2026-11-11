//! Bakon v1 核心库：存储、对象、清单与版本编号。
//!
//! 本 crate 不依赖终端、CLI 或任何外部命令；所有错误经 [`Error::exit_code`]
//! 映射为稳定的退出码。

mod clock;
mod fsutil;

pub mod apply;
pub mod error;
pub mod manifest;
pub mod object;
pub mod store;
pub mod types;

/// 本程序支持的存储布局 schema；标记、索引与 `.pending` 共用。
pub const SUPPORTED_SCHEMA: u32 = 1;

/// 清单的最新 schema：1 不含备注，2 允许 `versions[].note`。
pub const MANIFEST_SCHEMA: u32 = 2;

/// 仍可读取的最旧清单 schema。
pub const MANIFEST_MIN_SCHEMA: u32 = 1;

pub use error::{Error, Result};
pub use store::Store;
pub use types::{AbsPath, BlobHash, FileId, Ver};

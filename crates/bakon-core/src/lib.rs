//! Bakon v1 核心库：存储、对象、清单与版本编号。
//!
//! 本 crate 不依赖终端、CLI 或任何外部命令；所有错误经 [`Error::exit_code`]
//! 映射为稳定的退出码。

pub mod error;
pub mod store;
pub mod types;

pub use error::{Error, Result};
pub use store::{MARKER_FILE, SUPPORTED_SCHEMA, Store};
pub use types::{AbsPath, BlobHash, FileId, Ver};

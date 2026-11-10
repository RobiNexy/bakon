//! 命令行语法。字段与 `design/API__store-contract-and-cli.md` §5 对齐。

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "bakon",
    about = "Local version history for single files",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Store directory; defaults to $BAKON_STORE, then $BAKON_HOME/store, then ~/.bakon/store
    #[arg(long, global = true, value_name = "DIR")]
    pub store: Option<PathBuf>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value = "human")]
    pub format: Format,

    /// Milliseconds to wait for the store lock
    #[arg(long, global = true, value_name = "MS", default_value_t = 2000)]
    pub lock_timeout: u64,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Human-readable output; may change between minor versions
    Human,
    /// Tab-separated fields for scripts
    Plain,
    /// One JSON value per line
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create the store, or validate an existing one
    Init,
    /// Print the program version and build target
    Version,
    /// Track a file on first use, then record the content saved by the editor
    Edit {
        /// File to edit
        file: PathBuf,
    },
    /// List recorded versions, newest first
    Log {
        /// Tracked file
        file: PathBuf,
    },
    /// Write one recorded version to stdout
    Show {
        /// Tracked file
        file: PathBuf,
        /// Version number
        ver: u32,
    },
    /// Same as show; kept as a pipeline-friendly name
    Dump {
        /// Tracked file
        file: PathBuf,
        /// Version number
        ver: u32,
    },
    /// Show a unified diff between recorded versions
    Diff {
        /// Tracked file
        file: PathBuf,
        /// Older version; defaults to the previous version
        first: Option<u32>,
        /// Newer version; defaults to the latest version
        second: Option<u32>,
    },
}

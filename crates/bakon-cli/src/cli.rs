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

    /// Config file; defaults to $BAKON_CONFIG, then $BAKON_HOME/config.toml, then ~/.bakon/config.toml
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

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
        /// Skip the per-file hook
        #[arg(long)]
        no_hook: bool,
    },
    /// Restore a recorded version and record the restore as a new version
    Revert {
        /// Tracked file
        file: PathBuf,
        /// Version to restore
        ver: u32,
        /// Record the current divergent content as a salvage version first
        #[arg(long)]
        force: bool,
        /// Skip the per-file hook
        #[arg(long)]
        no_hook: bool,
    },
    /// List recorded versions, newest first
    Log {
        /// Tracked file
        file: PathBuf,
    },
    /// List every tracked path, sorted
    Ls,
    /// Show mapping details for one tracked file
    Path {
        /// Tracked file
        file: PathBuf,
    },
    /// Show the store location or its disk usage
    Store {
        #[arg(value_enum)]
        action: StoreAction,
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
    /// Trim old versions for one file or every file
    Prune {
        /// Tracked file; defaults to every file
        file: Option<PathBuf>,
        /// Report what would be removed without changing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Change the recorded path without touching the file on disk
    Mv {
        /// Currently tracked path
        old: PathBuf,
        /// New recorded path
        new: PathBuf,
    },
    /// Set, unset or show a per-file hook
    Hook {
        #[arg(value_enum)]
        action: HookAction,
        /// Tracked file
        file: PathBuf,
        /// Shell command for `set`
        command: Option<String>,
    },
    /// Show effective configuration or write a default file
    Config {
        #[arg(value_enum)]
        action: ConfigAction,
    },
    /// Check store consistency
    Verify {
        /// Repair derived data (sizes, index, leftovers)
        #[arg(long)]
        fix: bool,
        /// Also report space usage
        #[arg(long)]
        size: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum HookAction {
    Set,
    Unset,
    Show,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum StoreAction {
    /// Print the store root
    Path,
    /// Print disk usage under the store root
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ConfigAction {
    Show,
    Init,
}

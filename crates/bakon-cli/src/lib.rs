//! CLI 层：参数解析、输出格式与退出码映射。
//!
//! 本层只做 I/O 与格式转换；所有语义判断都在 `bakon-core`。

mod cli;

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use bakon_core::{Error, Result, Store};
use cli::{Cli, Command, Format};

/// 解析参数、执行命令并返回进程退出码。
pub fn run() -> ExitCode {
    let cli = Cli::parse_args();
    match dispatch(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("bakon: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

fn dispatch(cli: &Cli) -> Result<u8> {
    match &cli.command {
        Command::Init => init(cli),
        Command::Version => version(cli.format),
    }
}

fn init(cli: &Cli) -> Result<u8> {
    let path = store_path(cli)?;
    let (store, created) = Store::init(&path)?;
    let root = store.root().as_path().display().to_string();

    let line = match cli.format {
        Format::Human if created => format!("initialized {root}"),
        Format::Human => format!("already initialized: {root}"),
        Format::Plain => format!("{root}\t{}", if created { "created" } else { "existing" }),
        Format::Json => serde_json::json!({
            "store": root,
            "created": created,
            "schema": store.marker().schema,
        })
        .to_string(),
    };
    emit(&line)?;
    Ok(0)
}

fn version(format: Format) -> Result<u8> {
    let version = env!("CARGO_PKG_VERSION");
    let target = env!("BAKON_TARGET");

    let line = match format {
        Format::Human => format!("bakon {version} ({target})"),
        Format::Plain => format!("{version}\t{target}"),
        Format::Json => serde_json::json!({ "version": version, "target": target }).to_string(),
    };
    emit(&line)?;
    Ok(0)
}

/// 存储位置优先级：`--store` > `BAKON_STORE` > `BAKON_HOME/store` > `~/.bakon/store`。
fn store_path(cli: &Cli) -> Result<PathBuf> {
    if let Some(path) = &cli.store {
        return Ok(path.clone());
    }
    if let Some(path) = env::var_os("BAKON_STORE").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    if let Some(home) = env::var_os("BAKON_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home).join("store"));
    }
    let home = home_dir()
        .ok_or_else(|| Error::Usage("cannot determine the home directory; pass --store".into()))?;
    Ok(home.join(".bakon").join("store"))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// stdout 只放数据；写入失败视为 I/O 错误而不是 panic。
fn emit(line: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}").map_err(Error::Io)
}

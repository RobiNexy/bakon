//! CLI 层：参数解析、输出格式与退出码映射。
//!
//! 本层只做 I/O 与格式转换；所有语义判断都在 `bakon-core`。

mod cli;
mod commands;
mod config;

use std::process::ExitCode;

use bakon_core::Result;
use cli::{Cli, Command};
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
        Command::Init => commands::init(cli),
        Command::Version => commands::version(cli.format),
        Command::Edit { file, editor, note } => {
            commands::edit(cli, file, editor.as_deref(), note.as_deref())
        }
        Command::Revert {
            file,
            ver,
            force,
            note,
        } => commands::revert(cli, file, *ver, *force, note.as_deref()),
        Command::Note {
            file,
            ver,
            text,
            clear,
        } => commands::note(cli, file, *ver, text.as_deref(), *clear),
        Command::Log { file } => commands::log(cli, file),
        Command::Ls => commands::ls(cli),
        Command::Path { file } => commands::path(cli, file),
        Command::Store { action } => commands::store(cli, *action),
        Command::Show { file, ver } | Command::Dump { file, ver } => {
            commands::show(cli, file, *ver)
        }
        Command::Diff {
            file,
            first,
            second,
        } => commands::diff(cli, file, *first, *second),
        Command::Prune { file, dry_run } => commands::prune(cli, file.as_deref(), *dry_run),
        Command::Mv { old, new } => commands::mv(cli, old, new),
        Command::Hook {
            action,
            file,
            command,
        } => commands::hook(cli, *action, file, command.as_deref()),
        Command::Config { action } => commands::config(cli, *action),
        Command::Verify { fix, size } => commands::verify(cli, *fix, *size),
    }
}

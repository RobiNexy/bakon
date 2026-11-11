# 2026-11-11-1908__fix-cli-json-and-global-flags

- Mode: Copilot
- Task: 修复全量审计 A-12（状态命令忽略 `--format json`，全局 `--no-hook`/`--no-color` 与文档不一致）
- Did: `hook set`/`hook unset`/`config init` 增加 human/plain/json 输出；`--no-hook` 提升为 clap 全局参数并移除子命令重复定义；API 文档删除未实现的 `--no-color`
- Verified: `cargo test --workspace --all-targets` 142 通过；`cargo test --doc -p bakon-core` 通过；clippy `-D warnings`、`cargo fmt --all --check`、`cargo check --release` 通过
- Touched: [[design-audit-v1-2026-11-11]], [[API__store-contract-and-cli]], [[task_board]], [[project_state_now]]
- Next: A-10（跨平台测试门）与 P3 审计残余

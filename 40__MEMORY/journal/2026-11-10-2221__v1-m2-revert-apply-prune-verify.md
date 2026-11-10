# 2026-11-10-2221__v1-m2-revert-apply-prune-verify

- Mode: Autopilot
- Task: v1 M2：`revert` 与写回策略、`.pending` 恢复、`prune`、`mv`、`hook`、`config`、`verify`，以及 13 项审计缺陷的回归用例
- Did: 按目标类型写回（普通文件保留 inode、跟随符号链接、拒绝目录/悬空链接）；`revert` 含分歧拒绝与 `--force` salvage；`.pending` 标记与启动恢复；`prune` 按引用计数回收对象；`mv`、`hook`、`config`、`verify`（含 `--fix`/`--size`）；13 条回归用例
- Verified: `cargo test --workspace --all-targets` 102 通过（含 13 条回归）；clippy `-D warnings`、`cargo fmt --all --check` 通过；`go build`/`go vet`/gofmt 通过
- Touched: [[MODULE__bakon-v1-design]], [[API__store-contract-and-cli]], [[OPS__store-identity-recovery-and-retention]], [[project_state_now]], [[task_board]]
- Next: M3（崩溃注入、并发、Windows、交叉编译、性能）；配置默认位置改为 XDG，避免复用 0.x 的 `~/.bakon/config.toml`

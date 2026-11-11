# 2026-11-11-1808__fix-revert-commit-prune-open

- Mode: Copilot
- Task: 修复全量审计 A-03（revert 无 pending 重写）、A-04（重复路径）、A-05（prune 重复 id）、A-06（open 自动创建）
- Did: `revert` 目标已匹配且存在时 no-op；`commit` track 前检查路径键唯一；`prune` 对 id 去重并使用 `checked_sub`；`Store::open` 只打开已有标记，`init` 与首次 `edit` 才创建；新增 `begin_verify` 保留 `verify --fix` 的 pending 报告路径
- Verified: `cargo test --workspace --all-targets` 136 通过；clippy `-D warnings`、`cargo fmt --all --check`、`cargo check --release` 通过
- Touched: [[design-audit-v1-2026-11-11]], [[API__store-contract-and-cli]], [[SEQUENCE__write-protocol-and-failure-paths]], [[OPS__store-identity-recovery-and-retention]], [[MODULE__bakon-v1-design]], [[task_board]], [[project_state_now]]
- Next: 处理 A-07（路径 `..`）、A-08（权限）、A-09（锁类型）

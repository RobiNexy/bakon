# 2026-11-11-1744__fix-init-temp-and-pending-recovery

- Mode: Copilot
- Task: 修复全量审计 A-01（`init` 删除用户临时形状文件）与 A-02（恢复假成功并删除 `.pending`）
- Did: `init` 只忽略由 `MARKER_FILE` 派生的严格数字临时名，且不再删除任何匹配文件；A-02 校验 `entry.version`/`pending_apply`/`new_blob`，缺失清单、裁剪、不匹配或目标偏离时保留 `.pending` 并退出 6；仅“版本已记录且目标已等于新内容”或“清单头小于记录版本”丢弃，`verify --fix` 通过 `begin_verify` 报告 `pending_stale`；新增 1 条 pending 限制
- Verified: `cargo test --workspace --all-targets` 130 通过；clippy `-D warnings`、`cargo fmt --all --check`、`cargo check --release` 通过
- Touched: [[design-audit-v1-2026-11-11]], [[OPS__store-identity-recovery-and-retention]], [[SEQUENCE__write-protocol-and-failure-paths]], [[API__store-contract-and-cli]], [[task_board]], [[project_state_now]]
- Next: 继续 A-03 至 A-09；A-06（open 自动创建）与 A-09（锁类型）优先

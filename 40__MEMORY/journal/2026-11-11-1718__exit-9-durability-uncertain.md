# 2026-11-11-1718__exit-9-durability-uncertain

- Mode: Copilot
- Task: 按 [[ADR__exit-9-when-durability-uncertain]] 实现“清单 rename 后父目录 fsync 失败返回退出码 9”，并补故障注入测试与文档
- Did: `write_atomic` 增加 `Durable`/`VisibleButNotDurable` 阶段结果；`save_manifest` 抛出 `DurabilityUncertain` 并映射退出码 9；对象、`.pending`、索引、标记仍严格映射 8；`edit` 不执行钩子，`revert` 保留 `.pending`；新增 3 个 debug 故障注入端到端用例；新增 ADR 并同步退出码矩阵、README、API、OPS 与任务板
- Verified: `cargo test --workspace --all-targets` 123 通过；clippy `-D warnings`、`cargo fmt --all --check`、`cargo check --release` 通过
- Touched: [[ADR__exit-9-when-durability-uncertain]], [[design-audit-v1-2026-11-11]], [[MODULE__bakon-v1-design]], [[API__store-contract-and-cli]], [[SEQUENCE__write-protocol-and-failure-paths]], [[task_board]], [[project_state_now]]
- Next: 继续 A-01/A-02；退出码 9 的 `bakon store sync` 消解方式留作后续

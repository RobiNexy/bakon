# 2026-11-11-1901__fix-verify-corrupt-manifest

- Mode: Copilot
- Task: 修复全量审计 A-11（`verify` 遇到损坏清单整体失败并丢失报告）
- Did: `verify` 只从成功解析的清单构造引用集；损坏清单保留为 `manifest` 报告项；reachability 不完整时报告 `orphan_scan`，`--fix` 跳过孤儿回收与索引重建，避免删除损坏清单仍可能引用的对象
- Verified: `cargo test --workspace --all-targets` 140 通过；clippy `-D warnings`、`cargo fmt --all --check` 通过
- Touched: [[design-audit-v1-2026-11-11]], [[API__store-contract-and-cli]], [[MODULE__bakon-v1-design]], [[task_board]], [[project_state_now]]
- Next: A-10（跨平台测试）与 A-12（JSON/全局标志）

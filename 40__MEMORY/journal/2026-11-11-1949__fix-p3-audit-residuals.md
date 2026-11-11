# 2026-11-11-1949__fix-p3-audit-residuals

- Mode: Copilot
- Task: 在 Ubuntu 可验证范围内收口全量审计 P3（A-13 至 A-19、O-01 至 O-09、O-11 至 O-13），并记录 O-10/Windows 剩余
- Did: 共享锁改只读打开 `.lock`，只读布局可读；清单拒绝 `u32::MAX`、非 UTC 时间与非法 pending；`verify` 报告并清理严格命名的原子写临时文件，报告非法对象名；`ObjectStore::put` 修复损坏对象；`begin_apply`/`finish_apply` 校验版本、head 与唯一 pending；恢复目标非普通文件返回 7；`rename` 总是重写索引；`edit` 路径变化返回 `PathChanged` 4；`diff` 帮助与单参数语义对齐；CLI 全部走 `content_for`；`Committed` 携带 `TrimFailure`；配置原子写入并拒绝未知字段；broken pipe 不再映射为 8；实现 `Store::versions` 并更新架构文档；硬链接、并发 init、旧存储不迁移与全局引用扫描决定写入设计文档
- Verified: `cargo test --workspace --all-targets` 159 通过；`cargo test --doc -p bakon-core` 1 个 compile-fail 用例通过；clippy `-D warnings`、`cargo fmt --all --check`、`cargo check --release` 通过
- Touched: [[design-audit-v1-2026-11-11]], [[API__store-contract-and-cli]], [[DATA__version-manifest-and-numbering]], [[MODULE__bakon-v1-design]], [[OPS__store-identity-recovery-and-retention]], [[SEQUENCE__write-protocol-and-failure-paths]], [[task_board]], [[project_state_now]]
- Next: 仅剩 Windows/macOS 相关（A-10、P1 Windows 钩子/并发、大小写路径、O-10）与 Release 回滚演练；Ubuntu 可验证的 P3 已收口

# 2026-11-11-1854__fix-path-permissions-lock-types

- Mode: Copilot
- Task: 修复全量审计 A-07（`..` 词法折叠）、A-08（存储权限随 umask）、A-09（读写锁未类型化）
- Did: `AbsPath::new` 拒绝任何 `..`（退出 2），`AbsPath::join` 收紧为 `pub(crate)`；Unix 上新建目录 0700、新建文件 0600，覆盖存储根、布局、对象分片、`.lock` 与原子写临时文件；锁拆为 `ReadLock`/`WriteLock`，写接口与 `verify_fix` 要求 `&WriteLock`，`ReadLock` 附带 `compile_fail,E0308` 文档用例；`prune` 拆为写路径与 `prune_dry_run`
- Verified: `cargo test --workspace --all-targets` 138 通过；`cargo test --doc -p bakon-core` 1 个 compile-fail 用例通过；clippy `-D warnings`、`cargo fmt --all --check`、`cargo check --release` 通过
- Touched: [[design-audit-v1-2026-11-11]], [[API__store-contract-and-cli]], [[MODULE__bakon-v1-design]], [[OPS__store-identity-recovery-and-retention]], [[SEQUENCE__write-protocol-and-failure-paths]], [[task_board]], [[project_state_now]], README
- Next: A-10（跨平台测试）、A-11（损坏清单不中断 verify）、A-12（JSON/全局标志）

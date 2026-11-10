# 2026-11-11-0013__v1-type-path-prune-release

- Mode: Copilot
- Task: 继续修复 V1-7/V1-9/V1-11/V1-13，重建 cargo Release 工作流，并确认 CLI 子命令覆盖
- Did: 写回收敛到 `Committed::apply`，`apply::write_target` 改为 crate 内，`Ver::next`/`Ver::new` 与 `FileId::from_bytes` 收紧；`Store::content_for` 按 `codec` 分派；`AbsPath` 增加平台路径键并拒绝经过符号链接的 `..`；`prune` 在无删除时只读取目标清单，定理 4 区分持久化片段与全局引用扫描；新增 `.github/workflows/release.yml`（7 目标，cargo/cross）；确认 `ls`/`path`/`store` 均已实现并更新复核文档
- Verified: `cargo test --workspace --all-targets` 119 通过（core 57、regressions 13、cli 集成 35、cli 单元 7、crash 6、concurrency 1）；clippy `-D warnings`、`cargo fmt --all --check` 通过；`release.yml` 可被 YAML 解析；`bakon --help` 列出全部 16 个命令；符号链接 `..` 返回 2；无删除的单文件 `prune` 不读取其他损坏清单
- Touched: [[design-audit-v1-2026-11-10-recheck]], [[API__store-contract-and-cli]], [[DATA__version-manifest-and-numbering]], [[STATE__store-state-machine-and-invariants]], [[project_state_now]], [[task_board]], README
- Next: 在真实 tag 上验证 Release 与回滚；Windows/macOS 大小写路径；实际裁剪的持久化引用计数；不推送远端

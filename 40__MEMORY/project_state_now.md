# Project state now

> What is true now that is not obvious from code or Git and that the next session needs. Normally updated during compaction ([[AGENT__procedure__compaction]]). Delete empty sections; the file may be empty.

## Current state

- v1（Rust）是当前实现，已位于 `main`；0.x 的 Go 实现、`go.mod`、`internal/`、Go 发布脚本与 Go CI 已从代码树删除，只保留在历史提交与旧标签中。依据：[[ADR__replace-git-with-own-content-addressed-store]]、[[ADR__rewrite-in-rust]]。
- v1 形式论证与详细设计已完成；M0–M2 已落地在 `crates/`：对象层、清单层、`init`/`edit`/`log`/`ls`/`path`/`show`/`diff`/`revert`/`prune`/`mv`/`hook`/`config`/`verify`/`store`、写回与崩溃恢复。13 项审计缺陷的回归用例在 `crates/bakon/tests/regressions.rs`。
- M3 已完成崩溃注入、并发用例、性能实测、7 个目标的 `cargo check`，以及 Windows/macOS runner 冒烟（build、`init`、`verify`；Windows 另含 `edit`/`log`/`revert`）。cargo Release 工作流已建立但尚未在真实 tag 上运行；Windows 钩子与并发用例、7 个目标的可链接产物未验证。设计入口：[[MODULE__bakon-v1-design]]。
- 0.x 的 `docs/design.md` 与 `docs/new_design.md` 已删除；`docs/` 保留两份审计报告（[[design-audit-2026-11-10]] 与 [[design-audit-v1-2026-11-10]]）。`README.md`、`README.en.md` 与项目文档只描述 v1。
- 第二次形式化审计的 P1 已修复：裁剪与 `prune` 的孤儿对象会回收，`init` 能清理崩溃留下的标记临时文件，`verify` 会报告 pending 与 `.pending`，恢复会先 `salvage` 崩溃后的外部编辑；`ls`/`path`/`store` 已补齐。复核见 [[design-audit-v1-2026-11-10-recheck]]，剩余 P2/P3 见任务板。

## Known limitations

- v1 的正确性依赖 POSIX `rename` 与目录 `fsync`；Windows 无目录 `fsync` 等价语义，崩溃恢复退化为「可能丢失最后一次提交」。
- 7 个交叉编译目标的工具链尚未验证；本机只有宿主默认目标 `x86_64-unknown-linux-gnu`（Rust stable 1.99.1 已装，`~/.cargo/bin` 在 PATH）。
- 旧存储迁移不在 v1.0 范围；旧 `repo/` 不被读取也不被删除。
- `edit` 通过 `sh -c` 启动编辑器，Windows 语义属 M3。
- 复审剩余：`commit`/`prune`/`rename` 的写锁仍是调用点约定；Windows/macOS 大小写路径键未在 runner 验证；实际裁剪仍需全局引用扫描。`..` 路径、类型化写回、`codec` 分派与 CLI 命令已修复。
- 人工验收流程见 [[OPS__manual-testing]]；`README.md`/`README.en.md` 只保留介绍与使用说明。
- 端到端 1 MB 文本提交实测约 50 ms（本机 ext4/NVMe），验收预算已调整为 60 ms；成本主要是进程启动、编辑器与 fsync。

## Important deviations

- 复审剩余项：`commit`/`prune`/`rename` 的写锁仍是调用点约定；Windows/macOS 大小写路径键未在真实 runner 验证；实际裁剪仍需全局引用扫描。
- 0.x 的 `docs/design.md` 与 `docs/new_design.md` 已删除，旧审计只作为历史证据保留；当前设计以 `design/` 为准。

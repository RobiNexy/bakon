# Project state now

> What is true now that is not obvious from code or Git and that the next session needs. Normally updated during compaction ([[AGENT__procedure__compaction]]). Delete empty sections; the file may be empty.

## Current state

- v1（Rust）是当前工作线，位于 `v1-rust` 分支（领先 `main` 13 个提交）；`main` 保留 0.x（Go + git）作为历史。依据：[[ADR__replace-git-with-own-content-addressed-store]]、[[ADR__rewrite-in-rust]]。
- v1 形式论证与详细设计已完成；M0–M2 已落地在 `crates/`：对象层、清单层、`edit`/`log`/`show`/`diff`/`revert`/`prune`/`mv`/`hook`/`config`/`verify`、写回与崩溃恢复。13 项审计缺陷的回归用例在 `crates/bakon/tests/regressions.rs`。
- M3 已完成崩溃注入、并发用例、性能实测、7 个目标的 `cargo check`，以及 Windows/macOS runner 冒烟（build、`init`、`verify`；Windows 另含 `edit`/`log`/`revert`）。Windows 钩子与并发用例、7 个目标的可链接产物与发布路径未验证。设计入口：[[MODULE__bakon-v1-design]]。
- 0.x 的 `docs/design.md` 与 `docs/new_design.md` 已删除；`docs/` 只保留审计报告 [[design-audit-2026-11-10]]（历史证据）。`README.md`、`README.en.md` 与项目文档只描述 v1。
- 2026-11-10 对 v1 设计与实现做了第二次形式化审计：[[design-audit-v1-2026-11-10]]。4 项 P1：孤儿对象不可回收、`init` 崩溃前缀、`verify` 对 pending 不可判定、恢复覆盖崩溃后的外部编辑；另有路径身份、类型不变量与 CLI 契约等 P2。
- 0.x 代码仍在 `internal/`、`main.go`、`go.mod` 与 `scripts/build.sh`，不再接受独立缺陷修复，除非 v1 延期；旧 `repo/` 不被读取也不被删除。

## Known limitations

- v1 的正确性依赖 POSIX `rename` 与目录 `fsync`；Windows 无目录 `fsync` 等价语义，崩溃恢复退化为「可能丢失最后一次提交」。
- 7 个交叉编译目标的工具链尚未验证；本机只有宿主默认目标 `x86_64-unknown-linux-gnu`（Rust stable 1.99.1 已装，`~/.cargo/bin` 在 PATH）。
- 旧存储迁移不在 v1.0 范围。
- `edit` 通过 `sh -c` 启动编辑器，Windows 语义属 M3。
- 裁剪后对象不会自动回收，`verify --fix` 也不扫描 `objects/aa/` 下的孤儿；`max_versions` 只约束清单长度，不约束磁盘占用。
- 存在 `.pending` 或 `pending_apply` 时，共享锁读命令不恢复，`verify` 仍可能报告一致；下一次写命令恢复时会覆盖崩溃后对目标文件的外部编辑。
- 端到端 1 MB 文本提交实测约 50 ms（本机 ext4/NVMe），验收预算已调整为 60 ms；成本主要是进程启动、编辑器与 fsync。

## Important deviations

- `design/API__store-contract-and-cli.md` §5 列出的 `bakon ls`、`bakon path`、`bakon store` 尚未实现；CLI 目前只有 13 个命令。已在任务板跟踪。

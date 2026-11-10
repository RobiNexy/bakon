# Project state now

> What is true now that is not obvious from code or Git and that the next session needs. Normally updated during compaction ([[AGENT__procedure__compaction]]). Delete empty sections; the file may be empty.

## Current state

- 当前可运行实现是 0.x：Go + git 后端，功能完整，位于 `internal/`。2026-11-10 的形式化审计发现 13 项缺陷（[[design-audit-2026-11-10]]），其中 4 项可造成数据损坏。
- 已决定放弃 git 后端，用 Rust 重写 v1：[[ADR__replace-git-with-own-content-addressed-store]]、[[ADR__rewrite-in-rust]]。
- v1 形式论证与详细设计已完成；M0（工作区、错误码、存储打开）与 M1（对象层、清单层、纳管、`edit`/`log`/`show`/`diff`、编号与裁剪）已落地在 `crates/`。设计入口：[[MODULE__bakon-v1-design]]（含文档地图、阶段划分、验收判据）。
- 0.x 不再接受独立缺陷修复，除非 v1 延期；旧 `repo/` 不被读取也不被删除。

## Known limitations

- v1 的正确性依赖 POSIX `rename` 与目录 `fsync`；Windows 无目录 `fsync` 等价语义，崩溃恢复退化为「可能丢失最后一次提交」。
- 7 个交叉编译目标的工具链尚未验证；本机只有宿主默认目标 `x86_64-unknown-linux-gnu`（Rust stable 1.99.1 已装，`~/.cargo/bin` 在 PATH）。
- 旧存储迁移不在 v1.0 范围。
- `.pending` 恢复与目标文件写回策略尚未实现（M2）；当前若发现 `.pending` 会以退出码 6 拒绝写操作。
- `edit` 通过 `sh -c` 启动编辑器，Windows 语义属 M3。
- `README.md` 与 `docs/design.md` 仍描述 0.x，尚未同步 v1。

## Important deviations

- `docs/` 与 `design/` 描述两个不同版本：`docs/` 是 0.x 的实现设计（含 §12 的实现裁决），`design/` 是 v1 的目标设计。讨论当前代码以 `docs/` 为准，讨论下一步以 `design/` 为准。
- 0.x 的 `docs/design.md` §12 与 `new_design.md` 之间存在自相矛盾（`migrate` 是否实现），v1 文档不再引用二者。

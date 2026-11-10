# Project state now

> What is true now that is not obvious from code or Git and that the next session needs. Normally updated during compaction ([[AGENT__procedure__compaction]]). Delete empty sections; the file may be empty.

## Current state

- 当前可运行实现是 0.x：Go + git 后端，功能完整，位于 `internal/`。2026-11-10 的形式化审计发现 13 项缺陷（[[design-audit-2026-11-10]]），其中 4 项可造成数据损坏。
- 已决定放弃 git 后端，用 Rust 重写 v1：[[ADR__replace-git-with-own-content-addressed-store]]、[[ADR__rewrite-in-rust]]。
- v1 的形式论证与详细设计已完成，代码尚未开始，处于 M0 之前。设计入口：[[MODULE__bakon-v1-design]]（含文档地图、阶段划分、验收判据）。
- 0.x 不再接受独立缺陷修复，除非 v1 延期；旧 `repo/` 不被读取也不被删除。

## Known limitations

- v1 的正确性依赖 POSIX `rename` 与目录 `fsync`；Windows 无目录 `fsync` 等价语义，崩溃恢复退化为「可能丢失最后一次提交」。
- 7 个交叉编译目标的工具链尚未验证；本机未安装 Rust 工具链（`rustc` 不在 PATH）。
- 旧存储迁移不在 v1.0 范围。
- `README.md` 与 `docs/design.md` 仍描述 0.x，尚未同步 v1。

## Important deviations

- `docs/` 与 `design/` 描述两个不同版本：`docs/` 是 0.x 的实现设计（含 §12 的实现裁决），`design/` 是 v1 的目标设计。讨论当前代码以 `docs/` 为准，讨论下一步以 `design/` 为准。
- 0.x 的 `docs/design.md` §12 与 `new_design.md` 之间存在自相矛盾（`migrate` 是否实现），v1 文档不再引用二者。

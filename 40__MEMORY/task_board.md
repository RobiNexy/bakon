# Task board

> Unfinished work worth keeping across sessions, in priority order. Remove items when done; Git keeps history. Assumes a single writer; coordinate before parallel sessions edit it.

<!-- One line per task. Link an existing design file when one applies; mark blockers with Q-<topic>. Remove the reference when the question is resolved. Empty is fine. -->

> 审计来源：[[design-audit-2026-11-10]]；v1 设计入口：[[MODULE__bakon-v1-design]]。

- P0 v1 M0：Rust 工作区与 crate 骨架、错误类型与退出码、标记文件与存储打开（[[MODULE__bakon-v1-design]] §10）
- P0 v1 M1：对象层、清单层、纳管、`edit`、`log`、`show`、`diff`，含编号与裁剪的属性测试
- P1 v1 M2：`revert` 与写回策略、`prune`、`mv`、`hook`、`config`、`verify`，13 项缺陷的回归用例
- P1 v1 M3：崩溃注入、并发冲突、Windows 语义、7 个目标的交叉编译、性能实测
- P1 同步 `README.md` 与 `docs/design.md`：标注 0.x 为历史，或改写为 v1
- P2 决定是否为旧存储提供 feature-gated 迁移工具（`legacy-git`）
- P2 0.x 缺陷不单独修；若 v1 延期，按 [[design-audit-2026-11-10]] §5 的顺序处理

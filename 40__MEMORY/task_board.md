# Task board

> Unfinished work worth keeping across sessions, in priority order. Remove items when done; Git keeps history. Assumes a single writer; coordinate before parallel sessions edit it.

<!-- One line per task. Link an existing design file when one applies; mark blockers with Q-<topic>. Remove the reference when the question is resolved. Empty is fine. -->

> 审计来源：[[design-audit-2026-11-10]]；v1 设计入口：[[MODULE__bakon-v1-design]]。

- P1 v1 M3：崩溃注入、并发冲突、Windows 语义、7 个目标的交叉编译、性能实测（[[MODULE__bakon-v1-design]] §10）
- P1 v1 发布路径：用 cargo 产物替换 Go 的 `scripts/build.sh`、`dist/` 与 Release 工作流，并演练一次回滚（[[AGENT__procedure__change-path]]）
- P1 同步 `README.md` 与 `docs/design.md`：标注 0.x 为历史，或改写为 v1
- P2 决定是否为旧存储提供 feature-gated 迁移工具（`legacy-git`）
- P2 0.x 缺陷不单独修；若 v1 延期，按 [[design-audit-2026-11-10]] §5 的顺序处理

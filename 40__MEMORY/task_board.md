# Task board

> Unfinished work worth keeping across sessions, in priority order. Remove items when done; Git keeps history. Assumes a single writer; coordinate before parallel sessions edit it.

<!-- One line per task. Link an existing design file when one applies; mark blockers with Q-<topic>. Remove the reference when the question is resolved. Empty is fine. -->

> 审计来源：[[design-audit-2026-11-10]]、[[design-audit-v1-2026-11-10]]；v1 设计入口：[[MODULE__bakon-v1-design]]。

- P1 v1 M3 剩余：Windows 钩子与并发用例、7 个目标的可链接产物（[[MODULE__bakon-v1-design]] §10）
- P1 v1 发布路径：用 cargo 产物建立 Release 工作流，并演练一次回滚（[[AGENT__procedure__change-path]]）
- P2 v1 复审 V1-7：Ver/FileId 与写回顺序的不变量未由类型签名承载（[[design-audit-v1-2026-11-10]]）
- P2 v1 复审 V1-9：大小写与符号链接 `..` 的路径身份未归一化（[[design-audit-v1-2026-11-10]]）
- P2 v1 复审 V1-11：prune 仍扫描全库版本，成本不满足定理 4（[[design-audit-v1-2026-11-10]]）
- P3 v1 复审 V1-13：`codec` 字段不参与读取分派（[[design-audit-v1-2026-11-10]]）
- P2 决定是否为旧存储提供 feature-gated 迁移工具（`legacy-git`）

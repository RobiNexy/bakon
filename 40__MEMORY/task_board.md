# Task board

> Unfinished work worth keeping across sessions, in priority order. Remove items when done; Git keeps history. Assumes a single writer; coordinate before parallel sessions edit it.

<!-- One line per task. Link an existing design file when one applies; mark blockers with Q-<topic>. Remove the reference when the question is resolved. Empty is fine. -->

> 审计来源：[[design-audit-2026-11-10]]；v1 设计入口：[[MODULE__bakon-v1-design]]。

- P1 v1 M3 剩余：Windows 钩子与并发用例、7 个目标的可链接产物与发布路径（[[MODULE__bakon-v1-design]] §10）
- P1 v1 复审 V1-1：裁剪与 prune 崩溃产生的孤儿对象没有回收路径（[[design-audit-v1-2026-11-10]]）
- P1 v1 复审 V1-2：init 崩溃前缀留下临时标记，下一次 init 拒绝目录（[[design-audit-v1-2026-11-10]]）
- P1 v1 复审 V1-3/V1-4：verify 不检查 pending；恢复覆盖崩溃后的外部编辑（[[design-audit-v1-2026-11-10]]）
- P2 v1 复审 V1-5/V1-6：verify --fix 索引修复假成功；time 与版本连续性未校验（[[design-audit-v1-2026-11-10]]）
- P2 v1 复审 V1-7：Ver/FileId 与写回顺序的不变量未由类型签名承载（[[design-audit-v1-2026-11-10]]）
- P2 v1 复审 V1-8：读命令在共享锁下写 index（[[design-audit-v1-2026-11-10]]）
- P2 v1 复审 V1-9/V1-10：大小写与符号链接 `..` 的路径身份；非 UTF-8 路径留孤儿（[[design-audit-v1-2026-11-10]]）
- P2 v1 复审 V1-11/V1-13/V1-14：prune 成本、codec 分派、写回错误映射（[[design-audit-v1-2026-11-10]]）
- P1 补齐 API 契约中的 `ls`、`path`、`store` 命令（[[API__store-contract-and-cli]] §5）
- P1 v1 发布路径：用 cargo 产物替换 Go 的 `scripts/build.sh`、`dist/` 与 Release 工作流，并演练一次回滚（[[AGENT__procedure__change-path]]）
- P2 决定是否为旧存储提供 feature-gated 迁移工具（`legacy-git`）
- P2 0.x 缺陷不单独修；若 v1 延期，按 [[design-audit-2026-11-10]] §5 的顺序处理

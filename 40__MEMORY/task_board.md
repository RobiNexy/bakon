# Task board

> Unfinished work worth keeping across sessions, in priority order. Remove items when done; Git keeps history. Assumes a single writer; coordinate before parallel sessions edit it.

<!-- One line per task. Link an existing design file when one applies; mark blockers with Q-<topic>. Remove the reference when the question is resolved. Empty is fine. -->

> 审计来源：[[design-audit-2026-11-10]]、[[design-audit-v1-2026-11-10]]、[[design-audit-v1-2026-11-10-recheck]]、[[design-audit-v1-2026-11-11]]；v1 设计入口：[[MODULE__bakon-v1-design]]。

- P2 v1 全量审计：`..` 经过普通文件/缺失前缀时按词法折叠，路径身份与内核不一致（A-07）
- P2 v1 全量审计：对象与存储目录权限随 `umask`，0600 文件内容可能以 0664 存放（A-08）
- P2 v1 全量审计：读写锁未类型化，写接口不要求 guard（A-09，取代原复审残余条目）
- P2 v1 全量审计：CLI 测试依赖 `sh`/可执行脚本与 `/tmp` 断言，Windows 全量测试不可用（A-10）
- P2 v1 全量审计：`verify` 遇到损坏清单在孤儿扫描处整体失败，报告项丢失（A-11）
- P2 v1 全量审计：`hook set/unset`、`config init` 忽略 `--format json`；全局 `--no-hook`/`--no-color` 缺失（A-12）
- P1 v1 M3 剩余：Windows 钩子与并发用例（[[MODULE__bakon-v1-design]] §10）
- P2 v1 发布：演练一次 Release 回滚（[[AGENT__procedure__change-path]]）
- P2 v1 复审残余：Windows/macOS 的大小写路径键未在真实 runner 上验证（[[design-audit-v1-2026-11-10-recheck]]）
- P2 v1 复审残余：实际裁剪仍需全局引用扫描，若要 $O(k)$ 需持久化引用计数（[[design-audit-v1-2026-11-10-recheck]]）
- P2 决定是否为旧存储提供 feature-gated 迁移工具（`legacy-git`）
- P3 v1 全量审计残余：A-13 至 A-19 与 O-01 至 O-13 见 [[design-audit-v1-2026-11-11]] §6

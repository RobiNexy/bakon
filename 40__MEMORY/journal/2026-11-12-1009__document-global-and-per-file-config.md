# 2026-11-12-1009__document-global-and-per-file-config

- Mode: Copilot
- Task: 在 README 中补充全局配置文件与单文件配置说明，并讨论 hook 的强化方向
- Did: 中英文 README 新增“配置文件：全局与单文件”，覆盖 TOML 字段、查找顺序、清单 `hook`/`retention` 语义、当前单文件 retention 缺少 CLI 设置命令；hook 与 retention 章节增加交叉提示
- Verified: `git diff --check` 通过；纯文档改动，未跑无关代码测试
- Touched: 无
- Next: 如需强化 hook，优先做超时、手动重跑和 hook 并发控制

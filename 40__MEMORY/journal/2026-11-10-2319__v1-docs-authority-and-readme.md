# 2026-11-10-2319__v1-docs-authority-and-readme

- Mode: Copilot
- Task: 以 v1（Rust）设计为唯一依据，删除 0.x 设计文档并重写 README
- Did: 删除 `docs/design.md`、`docs/new_design.md`；`README.md` 与 `README.en.md` 重写为 v1 的构建、命令、存储、配置、钩子与退出码说明；`project_state_now`、任务板、项目定位与约定同步到 Rust 技术栈；修正 `MODULE__bakon-v1-design` §4 默认存储路径与 OPS/实现不一致；任务板新增 CLI 契约缺口（`ls`/`path`/`store`）
- Verified: `rg` 确认除历史日志与状态记录外无对已删除文档的引用；纯文档变更，未运行代码门禁（上一会话 106 测试通过，本次未改代码）
- Touched: [[project_state_now]], [[task_board]], [[project_purpose_and_boundary]], [[project_conventions]], [[MODULE__bakon-v1-design]]
- Next: 实现 `ls`/`path`/`store`；完成 7 个目标的可链接产物与 v1 发布路径

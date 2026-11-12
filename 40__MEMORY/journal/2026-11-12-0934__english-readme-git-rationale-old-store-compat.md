# 2026-11-12-0934__english-readme-git-rationale-old-store-compat

- Mode: Copilot
- Task: 补齐英文 README、回答为何不用 Git、更新 GitHub 项目描述，并确认旧版无备注存档可读可管理可补注释
- Did: `README.md`/`README.en.md` 增加语言入口与“为什么不直接用 Git”章节；`gh repo edit` 将描述从 “Built on git” 改为自包含内容寻址存储；新增 schema 1 旧清单回归测试；用真实 v1.0.2 二进制建库，v1.1.0 完成 `log`/`edit`/`note`/`mv`/`hook`/`prune`/`verify`，补备注升 schema 2、清除后回 schema 1
- Verified: `cargo test --workspace --all-targets` 172 通过；`cargo test --doc -p bakon-core` 通过；clippy `-D warnings`、fmt 通过；GitHub 描述已确认更新
- Touched: [[FEATURE__version-notes]]、[[project_state_now]]
- Next: 无

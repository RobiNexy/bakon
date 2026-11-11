# 2026-11-11-1011__user-guide-editor-flag-1.0.1

- Mode: Copilot
- Task: 按“读者照着做”的目标重写中英文使用说明书，加入 `edit --editor`、安装与 alias 建议，并把写作指令登记为项目程序
- Did: `README.md`/`README.en.md` 改为按用户目标分节、每节“前提 → 步骤 → 常见偏差 → 完成后”的详细说明书；新增安装到 `~/.local/bin`/cargo/`/usr/local/bin`、alias `be` 与 `--editor` 用法；新增 [[AGENT__procedure__writing-user-manual]] 并在入口注册；`bakon edit` 增加 `--editor`，优先级在 `$VISUAL` 之前；版本升至 1.0.1；更新 API 文档、人工手册与状态
- Verified: `cargo test --workspace --all-targets` 120 通过；clippy `-D warnings`、`cargo fmt --all --check` 通过；Release run `42943898624` 成功；v1.0.1 的 7 个归档与 checksums 校验通过；发布产物中 `bakon edit file --editor <cmd>` 实测可用
- Touched: README, [[OPS__manual-testing]], [[AGENT__procedure__writing-user-manual]], [[00__AGENT__START_HERE]], [[API__store-contract-and-cli]], [[project_state_now]]
- Next: 人工走查说明书；Release 回滚演练；Windows/macOS 平台补充

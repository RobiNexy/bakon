# 2026-11-10-2300__ci-windows-macos-smoke

- Mode: Autopilot
- Task: 在真实 Windows/macOS runner 上验证 v1 的基本运行时语义
- Did: CI 新增 `native-smoke`（Windows、macOS）：构建、`version`、两次 `init`、`verify`；Windows 额外覆盖 `edit`、`log`、`revert` 与写回。首次 Windows 运行暴露 `cmd /C` 参数被二次转义，改用 `CommandExt::raw_arg` 后通过
- Verified: CI run `62fe5bf`（Windows/macOS 冒烟）与 `50e1058`（Windows edit/revert）全部成功；本地 `cargo test --workspace --all-targets` 106 通过，clippy `-D warnings`、fmt 通过
- Touched: [[project_state_now]], [[task_board]]
- Next: Windows 钩子与并发用例、7 个目标的可链接产物与发布路径

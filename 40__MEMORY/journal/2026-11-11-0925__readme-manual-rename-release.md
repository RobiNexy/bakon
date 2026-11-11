# 2026-11-11-0925__readme-manual-rename-release

- Mode: Copilot
- Task: 精简 README、重命名目录与仓库为小写、建立并验证 Release、打 tag 推送、补充人工验收手册
- Did: `README.md`/`README.en.md` 只保留介绍与使用说明；新增 [[OPS__manual-testing]] 全流程手册；`Cargo.toml` 仓库地址改为小写；本地目录与 GitHub 仓库改为 `bakon`；重建 `.github/workflows/release.yml`（Linux cross、Intel macOS 交叉编译 darwin arm64、Windows amd64/arm64 原生）；修正 checksums 路径；创建并推送 `v1.0.0`
- Verified: `cargo test --workspace --all-targets` 119 通过；clippy `-D warnings`、`cargo fmt --all --check` 通过；Release run `42938481752` 成功；7 个归档与 `checksums.txt` 下载后逐一 `sha256sum -c` 通过；linux-amd64 产物 `version` 输出 `bakon 1.0.0`
- Touched: [[OPS__manual-testing]], [[project_state_now]], [[task_board]], README
- Next: 人工按手册验收；演练一次 Release 回滚；Windows/macOS 平台补充用例

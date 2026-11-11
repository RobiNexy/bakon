# 2026-11-11-2139__add-version-notes

- Mode: Copilot
- Task: 为每个文件版本增加可选备注，设计 CLI 用法、数据格式与兼容策略，实现并发布 1.1.0
- Did: 新增 `-m/--note`（`edit`/`revert`）与 `bakon note` 查看/设置/清除；`log` 三种格式输出备注，JSON 增加 `note`，钩子增加 `BAKON_NOTE`；清单 schema 1/2 按是否有备注切换；版本升至 1.1.0，提交 `4b3c1bc`，推送 tag `v1.1.0`
- Verified: `cargo test --workspace --all-targets` 171 通过；`cargo test --doc -p bakon-core` 通过；clippy `-D warnings`、fmt、7 个目标 `cargo check`、release 构建通过；Release run 43034905278 成功，7 个归档与 `checksums.txt` 下载后 7/7 校验通过；发布版 `version` 为 1.1.0，`init`/`edit -m`/`log`/`note`/`note --clear`/`verify` 冒烟通过；1.0.2 读取 schema 2 按预期退出 6
- Touched: [[FEATURE__version-notes]]、[[DATA__version-manifest-and-numbering]]、[[API__store-contract-and-cli]]、[[project_state_now]]
- Next: Windows/macOS 备注路径仅由 CI 构建覆盖，未在真实 runner 跑全量 CLI 用例

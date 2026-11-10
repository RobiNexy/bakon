# 2026-11-10-2206__v1-m1-object-manifest-edit

- Mode: Autopilot
- Task: 补 change-path 缺口（工具链固定、Rust CI、约定与发布路径记录），并实现 v1 M1：对象层、清单层、纳管、edit、log、show、diff
- Did: 固定 Rust 1.99.1 并加入 fmt/clippy/test 三项 CI 门禁；对象层（deflate + SHA-256、原子写入、去重、损坏判定）、清单层（严格 schema、三态保留、裁剪、索引缓存）、写锁（改用标准库 `File::lock`，替代文档中的 fs2）与启动清理；`edit`/`log`/`show`/`dump`/`diff` 可用
- Verified: `cargo test --workspace --all-targets` 64 通过；clippy `-D warnings`、`cargo fmt --all --check` 通过；冒烟：纳管→编辑→log/show/diff（diff 退出 1），外部持锁时 `--lock-timeout 100` 退出 4；`go build`/`go vet`/gofmt 通过
- Touched: [[MODULE__bakon-v1-design]], [[API__store-contract-and-cli]], [[DATA__version-manifest-and-numbering]], [[project_state_now]], [[task_board]]
- Next: M2（`revert`、写回策略、`prune`、`verify`）；`.pending` 恢复随 `revert` 一起实现

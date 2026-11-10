# 2026-11-10-2202__v1-m0-workspace-and-store-open

- Mode: Copilot
- Task: v1 M0：Rust 工作区与 crate 骨架、错误类型与退出码、标记文件与存储打开
- Did: 新增 Cargo 工作区与 `crates/{bakon-core,bakon-cli,bakon}`；四个 newtype（`AbsPath`/`FileId`/`Ver`/`BlobHash`）与 `Error::exit_code` 穷尽映射；`BAKON_STORE` 原子写入、schema 门与打开/初始化规则；`bakon version`、`bakon init` 可用
- Verified: `cargo build`、`cargo test`（24 通过）、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all --check` 全部通过；冒烟：init 重复执行退出 0，外部非空目录退出 6；`go build ./...`、`go vet ./...`、gofmt 未受影响
- Touched: [[MODULE__bakon-v1-design]], [[OPS__store-identity-recovery-and-retention]], [[project_state_now]], [[task_board]]
- Next: M1 对象层与清单层；锁、`.pending` 恢复与 `objects/tmp` 清理尚未实现（M1/M2）

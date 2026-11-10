# 2026-11-10-2241__v1-m3-concurrency-perf-docs

- Mode: Autopilot
- Task: v1 M3 中不依赖外部环境的部分：并发用例、性能实测、文档同步
- Did: 新增双进程竞争用例（编辑器阶段不持锁，两次提交一成一冲突）；锁诊断去掉一次 fsync，对象压缩降为最快级别，索引无变化时不重写；`README.md`、`README.en.md`、`docs/design.md` 标注 0.x 为历史
- Verified: `cargo test --workspace --all-targets` 103 通过（含并发 1）；clippy `-D warnings`、`cargo fmt --all --check`、`go build`/`go vet`/gofmt 通过；实测 1 MB 文本端到端提交约 50 ms、随机字节约 60 ms、小文件约 24 ms、`log` 6 ms、`verify` 48 ms、冷启动 3 ms，本机单次 fsync 约 3 ms
- Touched: [[project_state_now]], [[task_board]], [[MODULE__bakon-v1-design]], [[API__store-contract-and-cli]]
- Next: 崩溃注入（trait Journal 注入）、Windows 语义、7 个目标交叉编译；50 ms 与 5 ms 预算的差距需要裁决

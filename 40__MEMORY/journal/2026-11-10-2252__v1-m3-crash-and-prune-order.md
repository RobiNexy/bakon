# 2026-11-10-2252__v1-m3-crash-and-prune-order

- Mode: Autopilot
- Task: 崩溃注入、Windows 命令分派、7 目标检查，并修复 prune 的持久化顺序
- Did: debug 构建在每次 fsync/rename 后读取 `BAKON_TEST_CRASH_AFTER` 终止进程；崩溃矩阵覆盖 `edit`、`revert`、`prune`，重开后断言可读、可修复、恢复幂等；Windows 命令改走 `cmd /C`；CI 增加 6 个目标的 `rust-cross` 检查；`prune` 改为「先提交清单、再移走对象」，旧顺序会在崩溃后被启动清理误删仍被引用的对象
- Verified: `cargo test --workspace --all-targets` 106 通过；把 prune 顺序临时改回旧实现时新崩溃用例失败，改回后通过；6 个交叉目标 `cargo check` 与 CI `rust-cross` 全部成功；clippy `-D warnings`、fmt、`go build`/`go vet`/gofmt 通过
- Touched: [[MODULE__bakon-v1-design]], [[DATA__blob-store-and-durability]], [[SEQUENCE__write-protocol-and-failure-paths]], [[project_state_now]]
- Next: Windows runner 实测与 7 个目标的可链接产物；发布路径保持 0.x 直到这些验证完成

# 2026-11-10-2358__v1-p1-fixes-commands-go-removal

- Mode: Copilot
- Task: 修复复审 P1、补齐 `ls`/`path`/`store`、重新形式化审计、删除 0.x Go 实现并合并到 `main`
- Did: 提交裁剪与 `prune`/`verify --fix` 回收孤儿对象；`.pending` 改为记录 id、版本、旧/新哈希，恢复时把崩溃后的外部编辑记为 `salvage` 再追加 `revert`；`init` 清理标记临时文件；`verify` 报告 pending 与孤儿、`--fix` 索引失败返回 8；校验时间与版本连续性；拒绝非 UTF-8 路径；空 `prune` 输出 JSON；新增 `ls`/`path`/`store` 命令；删除 `main.go`、`go.mod`、`go.sum`、`internal/`、`scripts/build.sh`、Go Release/CI 任务与本地 `dist/`；更新 README、设计文档与两份复审报告；本地快进合并到 `main`
- Verified: `cargo test --workspace --all-targets` 117 通过（core 55、regressions 13、cli 集成 35、cli 单元 7、crash 6、concurrency 1）；clippy `-D warnings`、`cargo fmt --all --check` 通过；R1–R5 与 R7–R13 复核实验复现（静默拒绝 pending、init 崩溃、索引假成功等已修复；符号链接 `..` 仍复现）
- Touched: [[design-audit-v1-2026-11-10-recheck]], [[MODULE__bakon-v1-design]], [[SEQUENCE__write-protocol-and-failure-paths]], [[project_state_now]], [[task_board]], README
- Next: 复审剩余 V1-7/V1-9/V1-11/V1-13；建立 cargo 发布工作流；不推送远端

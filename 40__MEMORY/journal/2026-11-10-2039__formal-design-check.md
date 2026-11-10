# 2026-11-10-2039__formal-design-check

- Mode: Copilot
- Task: 按形式化求解协议检查 Bakon 的设计
- Did: 建立状态机、映射关系、序列投影三个模型并逐一比对设计声明；实测 14 个边界反例，定位 4 个 P0 缺陷，其中 store 指向既有仓库时 prune 会重写该系统仓库历史并丢弃文件
- Verified: `go build` 通过；实验脚本 `/tmp/opencode/design-check{,2,3}.sh` 与锁阻塞测试在隔离 `BAKON_HOME` 下运行，报告中标注「实测」的结论均来自这些运行；本轮未改代码，未重跑 `go test`
- Blocked: 四项架构级决定待人工裁决：`Q-repo-identity`、`Q-id-scheme`、`Q-index-authority`、`Q-revert-overwrite`
- Touched: [[task_board]], Q-id-scheme, Q-index-authority, Q-revert-overwrite
- Next: 按 P0 顺序修复；prune 复杂度需要先决定存储模型再动

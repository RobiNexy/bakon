# 2026-11-10-2114__answer-storage-and-language

- Mode: Copilot
- Task: 回答 git 依赖、自研存储可行性、是否改用 Rust，并记录存储模型待裁决
- Did: 实测无 git 时的失败形态与子进程成本；新增 `Q-storage-backend`，prune 成本条目绑定该问题
- Verified: 无 git 时 `edit` 退出 1、错误为原始 exec 文本，`log` 退出 3；50 版本 `log` 82 ms；`show` 10 ms；prune 在 100 提交处 750 ms
- Touched: Q-storage-backend, [[task_board]]
- Next: 等待存储模型裁决与既有四项裁决

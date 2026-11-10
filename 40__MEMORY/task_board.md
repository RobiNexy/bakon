# Task board

> Unfinished work worth keeping across sessions, in priority order. Remove items when done; Git keeps history. Assumes a single writer; coordinate before parallel sessions edit it.

<!-- One line per task. Link an existing design file when one applies; mark blockers with Q-<topic>. Remove the reference when the question is resolved. Empty is fine. -->

> 审计来源：[[design-audit-2026-11-10]]，下列 F 编号与其 §4 对应。


- P0 `internal/gitx`.Init 以 `--is-inside-work-tree` 判定仓库存在：store 在既有仓库内时提交写入外层仓库；store 在既有仓库根时，prune 重写该系统仓库历史并丢弃多文件提交中的其余路径（实测丢文件）— blocked by Q-repo-identity
- P0 revert 先写回后提交：提交失败时目标已被替换且无版本记录；写回用 rename，覆盖符号链接并更换 inode
- P0 `GIT_DIR` / `GIT_WORK_TREE` 未净化：指向别处时静默读到空历史或提交进错误仓库
- P1 mv 后旧路径重新纳管产生 ID 别名、两条路径共享同一段历史 — blocked by Q-id-scheme
- P1 revert 到最新版本时静默丢弃磁盘上的外部改动，不留版本 — blocked by Q-revert-overwrite
- P1 基线提交先落索引：提交失败后残留空条目，ver 0 语义丢失，verify 不报
- P1 per-file `max_versions=0` 无法表达「不限制」，回退到全局上限
- P1 diff 出错退出码为 1，与「差异=1、出错=2」契约冲突；ExitCode 依赖错误字符串匹配
- P1 钩子内调用需要写锁的 bakon 命令导致死锁（锁不可重入、无超时）
- P2 prune 在保留上限处重写全库历史：100 提交时单次 edit 约 750 ms（无裁剪约 26 ms）
- P2 `gc --prune=now` 与不取锁的读者竞争，读者可能碰到已删除对象
- P2 编辑器会话期间锁被长期持有，第二个写操作无提示地阻塞
- P2 文档漂移：design.md 写钩子失败退出码 2（实现 5）、`BAKON_PREV` 首值定义含糊、new_design §13 仍列 `bakon migrate`

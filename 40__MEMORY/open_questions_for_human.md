# Open questions for the human

> Only questions that block work or affect safety, compatibility or lasting decisions. Write a blocking question the moment it arises. When resolved: write the lasting decision in its authoritative document, remove task-board references, then remove the question here.
> IDs are `Q-<topic>`, unique within the project, stable once created, not numbered.

### Q-repo-identity: 是否要求 store 必须是 bakon 专用仓库，并拒绝其他目录？

- Impact: 当前只检查「目录位于某个工作树内」。store 指向既有仓库根时，prune 会重写该仓库历史并丢弃多文件提交中的其余路径（已实测丢文件）；指向其子目录时，提交写进外层仓库且 bakon 自己读不回来。
- Options: (1) 初始化时写入标记文件，缺失则拒绝并提示 `bakon config init`；(2) 要求 `repo/.git` 为仓库根且 `rev-parse --show-toplevel` 等于 store；(3) 仅修判定，不改写规则。
- Decision: pending

### Q-id-scheme: 是否允许改变纳管 ID 的生成方案以修复 mv 后的别名？

- Impact: 现有 ID 是路径哈希（`sha256(abs)` 前 6 字节）。`mv` 保留 ID，旧路径再次纳管会算出同一个 ID，两条路径共享一段历史，版本号与内容互相串写。
- Options: (1) 纳管时生成随机盐存入索引，`id = H(abs, salt)`，旧条目原样保留；(2) 不改 ID，纳管前检查同 ID 已存在则拒绝并提示 `bakon mv`；(3) 不改，只在文档中声明该行为。
- Decision: pending

### Q-index-authority: 索引是权威数据还是可重建缓存？

- Impact: 决定索引与 git 的写入顺序、崩溃窗口是否可接受、`verify` 是否需要 `--fix`。
- Options: (1) 权威数据：先提交后写索引，verify 双向校验；(2) 缓存：允许丢失，由 `verify --fix` 从 `files/*` 与提交正文的 `path:` 行重建。
- Decision: pending

### Q-storage-backend: 是否把 git 从「历史引擎」降为「对象存储」，或彻底自研存储？

- Impact: 决定 F1（prune 重写无关仓库）、F12（稳态约 750 ms）、F13（`GIT_DIR` 劫持）、F6 与索引一致性问题是被修复还是整体消失。把 git 降为对象存储后，prune 变成清单裁剪，不再重写历史。
- Options: (1) 保持提交模型，只加前置校验与重试；(2) 每版一个 blob，版本清单进索引/manifest，git 只承担 `hash-object` / `cat-file` / `gc`；(3) 自研内容寻址存储，去掉 git 依赖。
- Decision: pending

### Q-revert-overwrite: revert 到最新版本、而磁盘内容已偏离时如何处理？

- Impact: 当前静默覆盖且不产生版本，外部改动不可恢复。
- Options: (1) 覆盖并追加一个版本记录；(2) 不覆盖，退出码 4，要求 `--force`；(3) 保持现状并在帮助中声明。
- Decision: pending

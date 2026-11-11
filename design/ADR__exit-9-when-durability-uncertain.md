# ADR__exit-9-when-durability-uncertain

## Metadata

- Status: Accepted
- Proposed: 2026-11-11
- Accepted: 2026-11-11
- Supersedes: none

## Context and decision

清单写入的原子片段是「临时文件 -> 写入 -> `fsync` 文件 -> `rename` -> `fsync` 父目录」。`rename` 一旦返回成功，新清单已经对所有进程可见；父目录 `fsync` 只决定它在断电或系统崩溃后是否仍然存在。[[design-audit-v1-2026-11-11]] 的 A-20 指出，当前实现把父目录 `fsync` 失败折叠为普通 I/O 失败（8），但新清单可能已经可见；用户重试会得到 `HeadChanged`，无法判断命令到底提交没有。

决定：**把 `rename` 定义为提交点。** 清单 `rename` 成功、父目录 `fsync` 失败时，返回专用错误 `DurabilityUncertain`，退出码 **9**。它表示「已提交且可见，但持久性未确认」。不回滚、不自动重试整个操作、不执行钩子；对象写入、`.pending` 写入、索引写入和标记写入仍按未提交失败处理，退出码 8。

## Rationale

- 「报告普通失败」会让退出码 8 同时表示「没提交」和「可能已提交」，与 [[API__store-contract-and-cli]] 的脚本契约冲突。
- 「视为成功但警告」会让退出码 0 承诺了工具无法保证的持久性，对备份工具不可接受。
- 「回滚到旧清单」需要旧清单副本，且回滚本身也要 `rename` 与 `fsync`，在同一个失败的屏障上可能制造第三种状态。
- 「读取 head」只能确认可见性，不能确认持久性；`write_atomic` 已经知道失败发生在 `rename` 前还是后，按阶段分类比读 head 更精确。
- 对象写入失败发生在清单提交前：对象即使可见也只是孤儿，由 `verify --fix` 或 `prune` 回收，返回 8 即可。`.pending` 必须在清单提交前持久；它的目录 `fsync` 失败时继续提交会制造「清单有 pending、恢复记录却可能丢失」的状态，因此也返回 8。

## Consequences

- `Error` 增加 `DurabilityUncertain { id, version, source }`，`Error::exit_code` 返回 9；`Display` 明确说明版本可见、持久性未确认，并提示先运行 `bakon log` / `bakon verify`，不要盲目重试。
- `fsutil::write_atomic` 返回 `WriteOutcome::{Durable, VisibleButNotDurable}`；`save_manifest` 把后者映射为 `DurabilityUncertain`，其他调用方用 `require_durable()` 映射为 8。
- `edit`/`revert`/`prune`/`mv`/`hook`/`verify --fix` 都可能返回 9；`diff`/`show`/`log`/`ls` 不写清单，不会返回 9。退出码矩阵见 [[SEQUENCE__write-protocol-and-failure-paths]] §8。
- 钩子在 9 时不执行：钩子的副作用不应建立在一个可能被断电丢失的版本上。钩子仍然不保证执行；用户确认可见状态后可以手动运行或再次修改。
- `revert` 在 9 时保留 `.pending`，目标文件尚未写回；下一次写命令或 `verify --fix` 继续恢复。
- `verify` 只能确认当前可见状态一致，不能证明持久性；消息与文档明确这一点。
- 测试通过 `BAKON_TEST_FAIL_FSYNC_DIR=<目录名>`（仅 debug 构建）在 `rename` 后注入父目录 `fsync` 失败；覆盖 `edit` 的 9 与可见版本、`revert` 的 `.pending` 保留与后续恢复。
- 旧脚本若把 8 当作所有环境错误的唯一值，需要增加 9 分支；9 是新增码，不复用旧含义。

## Revisit when

- 增加 `bakon store sync` 或 `verify --durability`，可以显式重试目录 `fsync` 并消解不确定状态。
- 目标平台或文件系统不提供目录 `fsync` 等价语义时，9 的含义需要按平台重新定义；Windows 当前没有目录 `fsync`，不会走该分支。
- 若未来引入事务日志或 WAL，提交点与持久性屏障可能移动，届时以新的 ADR 取代本决策。

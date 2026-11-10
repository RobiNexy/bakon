# Bakon v1 修复后的再次形式化审计（2026-11-10）

审计对象是 `main` 上的 v1 实现（Rust）。前次审计见 [[design-audit-v1-2026-11-10]]；本报告只复核该审计的判定与修复后的状态，不重复 0.x 的历史缺陷。

## 1. 框架选择

沿用带崩溃切点的离散状态机；辅助关系与可达性分析对象回收，类型论检查编译期不变量，序理论检查版本编号与裁剪。修复后的关键变化是状态机新增了两条转移：提交裁剪后的对象回收，以及 pending 恢复中的 `salvage` 追加。审计目标是确认这两条转移使前次的不可达性质变为可达，并找出修复引入的新边界。

## 2. 形式化陈述

状态仍为 $S = (M, O, I, D, X)$，派生量同前次：$\mathrm{orphan}(S) = O \setminus \mathrm{ref}(M)$，$\mathrm{pending}(S)$ 为带待写回标记的版本集合。

修复后待判定的性质：

- $\mathbf{P_1}$ **空间活性**：从任意可达状态出发，`prune` 或 `verify --fix` 存在一条使 $\mathrm{orphan}(S) = \emptyset$ 的有限转移；正常提交裁剪不能制造无回收路径的孤儿。
- $\mathbf{P_2}$ **初始化全前缀**：`init` 的所有崩溃前缀都能被下一次 `init` 接受或自动修复。
- $\mathbf{P_3}$ **判定完备**：`verify` 返回 0 时，不存在未报告的 `.pending`、`pending_apply` 或孤儿对象。
- $\mathbf{P_4}$ **恢复安全**：崩溃后的目标外部内容不会被恢复流程静默覆盖。
- $\mathbf{P_5}$ **类型承载**：$\mathbf{S_2}$ 与 $\mathbf{S_6}$ 由类型签名推出。
- $\mathbf{P_6}$ **裁剪成本**：`prune` 对单文件的成本不超过 $O(k)$，与全库版本数无关。
- $\mathbf{P_7}$ **路径身份**：路径等价关系与文件系统实际打开的文件一致。
- $\mathbf{P_8}$ **输出全域**：`--format json` 在任何输入上只输出 JSON。

## 3. 判定摘要

| ID | 前次结论 | 复核结果 | 证据 |
|---|---|---|---|
| V1-1 | 孤儿对象不可回收 | 已修复 | 提交裁剪调用 `reclaim_removed`；`prune` 与 `verify --fix` 扫描全部对象 |
| V1-2 | `init` 崩溃前缀被拒绝 | 已修复 | 初始化临时文件被识别并清理 |
| V1-3 | `verify` 对 pending 不可判定 | 已修复 | `verify` 报告 `.pending` 与 `pending_apply`，`--fix` 先恢复 |
| V1-4 | 恢复覆盖外部编辑 | 已修复 | `.pending` 记录新旧哈希，外部内容记为 `salvage` |
| V1-5 | `verify --fix` 索引假成功 | 已修复 | `save_index` 返回 `Result`，写入失败返回 8 |
| V1-6 | `verify` 不校验时间与连续性 | 已修复 | `Manifest::validate` 校验 RFC 3339 与连续版本号 |
| V1-7 | 类型签名未承载 `S_2`/`S_6` | 已修复（写回与构造入口） | `Committed::apply` 是唯一公开写回入口；`apply::write_target` 为 crate 内；`Ver::next`/`Ver::new`、`FileId::from_bytes` 已收紧 |
| V1-8 | 读命令共享锁写索引 | 已修复 | `resolve` 不再写索引；`verify --fix` 负责重建 |
| V1-9 | 路径身份与内核不一致 | 已修复（Linux 路径） | 经过符号链接的 `..` 返回用法错误；`AbsPath::key` 归一化大小写 |
| V1-10 | 非 UTF-8 路径晚失败并留孤儿 | 已修复 | `AbsPath::new` 返回用法错误，对象未写入 |
| V1-11 | `prune` 成本随全库增长 | 部分修复 | 无删除的单文件 `prune` 只读目标清单；实际裁剪仍需全局引用扫描 |
| V1-12 | CLI 契约与 JSON 输出缺口 | 已修复 | 新增 `ls`/`path`/`store`；空 `prune` 输出 JSON |
| V1-13 | `codec` 不参与读取分派 | 已修复 | `Store::content_for` 按 `VersionRecord.codec` 分派 |
| V1-14 | 写回竞态映射为 3 | 已修复 | `applied` 把 `NotAFile` 映射为 `ApplyFailed`（7） |

## 4. 交叉论证

### 4.1 空间活性

修复后，提交路径在清单提交后调用 `reclaim_removed`（`crates/bakon-core/src/store/mod.rs:344` 与 `:548-564`）。该函数先求全局引用集，再移动候选对象，最后清空 `trash/`。因此裁剪产生的孤儿在同一个写事务内被处理；即使回收失败，`verify --fix` 的 `reclaim_orphans` 也会扫描 `objects/aa/` 并收敛。$\mathbf{P_1}$ 成立。

### 4.2 初始化前缀

`is_empty_dir` 现在忽略 `BAKON_STORE.*.tmp` 文件，`initialize` 在写标记前清理它们（`crates/bakon-core/src/store/mod.rs:950-1000`）。崩溃前缀 $I_2$ 不再是拒绝状态，而是被下一次 `init` 归入 $I_1$ 并继续。$\mathbf{P_2}$ 成立。

### 4.3 判定完备与恢复安全

`.pending` 现在记录 `{id, version, old_blob, new_blob}`（`crates/bakon-core/src/store/mod.rs:934-940`）。`verify` 检查 `.pending` 与每个清单的 `pending_apply`；`--fix` 在扫描前调用 `recover`。恢复按三种情况分派：

- 当前哈希等于 `new_blob`：只清标记。
- 当前内容缺失或哈希等于 `old_blob`：写回 `new_blob`。
- 其他哈希：追加 `salvage`，再追加新的 `revert` 并写回。

第三种情况保证外部内容进入历史；$\mathbf{P_4}$ 成立。$\mathbf{P_3}$ 成立的前提是 `verify` 能读到 `.pending`；`read_pending` 的 JSON 解析失败会作为 `pending_file` 项报告。

### 4.4 类型承载

写回入口已经收敛：`apply::write_target` 是 crate 内函数，外部只能通过 `Committed::apply` 写回；`Ver::new`、`Ver::next` 与 `FileId::from_bytes` 不再是公开构造入口，`Ver::parse` 明确表示“外部查找输入”。因此 $\mathbf{S_6}$ 的写回部分由类型承载。剩余缺口是 `commit` 的写锁仍是调用点约定，而不是签名参数；本报告把这一点记为类型系统的残余，而不是运行时缺陷。

### 4.5 成本、路径与输出

`prune` 先只读取目标清单；没有版本可删的单文件 `prune` 不扫描全库。实际发生裁剪时，仍需要一次全库引用扫描来判断对象是否共享；这是候选计算的 CPU 成本，不是持久化片段。$\mathbf{P_6}$ 的持久化片段部分成立，命令级成本在裁剪路径上仍与全库版本数相关。

`AbsPath::new` 拒绝经过符号链接的 `..`，`AbsPath::key` 在 Windows/macOS 上做大小写归一化；`link/../x` 不再被词法折叠到另一个文件。$\mathbf{P_7}$ 在 Linux 上成立；跨平台大小写行为仍需在对应 runner 上验证。

`Store::content_for` 按 `VersionRecord.codec` 分派；`prune` 的空结果按格式输出；`ls`/`path`/`store` 已实现，$\mathbf{P_8}$ 成立。

## 5. 反例与边界

以下实验使用修复后的二进制与隔离存储。

| 编号 | 操作 | 观测 | 结论 |
|---|---|---|---|
| R1 | `max_versions=2`，连续 5 次提交 | 对象数 2，引用数 2；`verify` 返回 0 | V1-1 修复 |
| R2 | 在 `prune` 的清单写入后崩溃，执行 `verify --fix` | 崩溃时对象 4；修复后对象 2 | V1-1 修复 |
| R3 | `BAKON_TEST_CRASH_AFTER=1 init` 后再次 `init` | 第一次 137；第二次 0，标记存在 | V1-2 修复 |
| R4 | 回退写入 pending 后崩溃，执行 `verify` | 返回 6，报告 `.pending` 与 `pending_apply`；`verify --fix` 返回 0 并写回目标 | V1-3 修复 |
| R5 | 崩溃后手写目标为 `external`，执行 `verify --fix` | 目标为 `one`；日志出现 `salvage` 大小为 8 与新 `revert` | V1-4 修复 |
| R7 | 用目录替换 `index.json`，执行 `verify --fix` | 返回 8，不再报告 `[fixed]` | V1-5 修复 |
| R8 | 清单 `time=not-a-time` | `verify` 返回 6，指出版本与非法时间 | V1-6 修复 |
| R9 | 非 UTF-8 文件名执行 `edit` | 返回 2；`objects/` 为 0 | V1-10 修复 |
| R10 | 路径 `link/../x`，`link` 指向 `real/` | 返回用法错误 2，两个文件都未修改 | V1-9 修复 |
| R11 | 删除 `index.json` 后执行 `log` | `log` 成功，索引未重建 | V1-8 修复 |
| R12 | `--format json prune` 无可裁剪版本 | 输出 `{"dry_run":false,"files":[]}` | V1-12 修复 |
| R13 | `ls`、`path`、`store path`、`store size` | 均返回 0，输出符合格式 | V1-12 修复 |

V1-7 的写回与构造入口已收敛；V1-11 的剩余部分是实际裁剪时的全局引用扫描；V1-13 已按条目分派。当前没有前次 P1 的运行时反例。

## 6. 落地指导

| 发现 | 修改 | 验证 |
|---|---|---|
| V1-7 残余 | `commit`/`prune`/`rename` 仍未要求 `&StoreLock` 参数；把写接口改为显式接收锁守卫 | `trybuild` 编译失败用例：未持锁调用写接口不通过 |
| V1-9 残余 | 在 Windows/macOS runner 上验证大小写折叠；确认大小写敏感卷是否需要运行时探测 | 两种拼写只产生一个清单；大小写敏感卷不误合并 |
| V1-11 残余 | 若要让实际裁剪也达到 $O(k)$，需要持久化引用计数；否则保持当前“持久化片段 $O(k)$ + 全局扫描”的表述 | 构造大量其他版本，测量“无删除”与“有删除”两条路径 |

## 7. 独立校验

### 7.1 已执行

- `cargo test --workspace --all-targets`：119 个测试通过（core 57、regressions 13、cli 集成 35、cli 单元 7、crash 6、concurrency 1）。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo fmt --all --check`：通过。
- R1 至 R5 与 R7 至 R13 的反例均以真实二进制与隔离存储复现。

### 7.2 未覆盖

- Windows 与 macOS 的大小写不敏感与 Unicode 归一化未在对应 runner 上构造反例；V1-9 的大小写部分仍是静态结论。
- `commit` 的写锁守卫仍是调用点约定；`trybuild` 编译失败用例尚未加入。
- 大规模 `prune` 成本只由代码路径推断，未做 $10^5$ 量级测量。
- 恢复时目标被删除（而非改写）的语义仍是 `[待确认]`：当前实现写回 pending 内容，不记录删除事件。

### 7.3 最弱前提

- 所有结论假设单机本地文件系统、单写者遵守锁、存储目录不被外部主动篡改。
- `.pending` 的旧格式不再兼容；v1 尚未发布，因此没有迁移负担。
- `verify` 的 pending 报告与恢复都依赖 `.pending` 可读；文件损坏会被报告为 `pending_file`，需要人工按 `.pending` 与清单重建。

# Bakon v1 设计与实现的再次形式化审计（2026-11-10）

审计对象是 `v1-rust` 分支的 `design/` 与 `crates/`。历史缺陷清单见 [[design-audit-2026-11-10]]；本报告只以 v1 文档与代码为依据，不重复 0.x 的结论。

## 1. 框架选择

主框架是带崩溃切点的离散状态机，与 v1 设计一致：写操作由单写锁串行化，编辑器会话不持锁，剩余的不确定性主要是持久化前缀，而不是并发交错。辅助框架有三项：

- 关系与可达性：对象集合 $O$ 与清单引用集合 $\mathrm{ref}(M)$ 的差集，用于判定空间回收是否可达。
- 类型论：检查设计声称的编译期不变量是否真的由类型签名承载，而不是由调用点约定承载。
- 序理论与输出契约：版本号单调、保留后缀，以及 `--format` 的输出全域。

`[另一解]` 把存储视为一个只追加的日志，把回收视为垃圾收集器的活性性质。结论相同：只要转移系统里不存在删除某类对象的边，该类对象就永远不可达删除。本文使用集合与可达性，因为它能直接对应代码中的 `put`、`save_manifest` 与 `prune`。

不使用概率模型：随机量只有 `FileId` 与临时文件名，碰撞不影响下述结论。不使用连续或拓扑模型：状态空间是有限目录树的离散快照。

## 2. 形式化陈述

### 2.1 状态与派生量

沿用 v1 设计的状态五元组：

$$S = (M, O, I, D, X)$$

其中 $M$ 是清单映射，$O$ 是对象映射，$I$ 是路径缓存，$D$ 是目标文件投影，$X$ 是锁与运行期状态。定义：

$$\mathrm{ref}(M) = \{\, v.\mathrm{blob} \mid (f, m) \in M,\ v \in m.\mathrm{versions} \,\}$$

$$\mathrm{orphan}(S) = O \setminus \mathrm{ref}(M)$$

$$\mathrm{pending}(S) = \{\, f \mid (f,m) \in M,\ \mathrm{head}(m).\mathrm{pending\_apply} \,\}$$

$$\mathrm{projected}(S) = \{\, f \mid (f,m) \in M,\ D(f) = \mathrm{content}(\mathrm{head}(m)) \,\}$$

`init` 的前缀状态单独定义：

- $I_0$：目录不存在。
- $I_1$：目录存在且为空。
- $I_2$：目录中有写入标记的临时文件，尚无 `BAKON_STORE`。
- $I_3$：`BAKON_STORE` 已原子替换完成。
- $I_4$：`files/`、`objects/`、`objects/tmp/`、`trash/` 已建立。

### 2.2 待判定性质

- $\mathbf{P_1}$ **空间活性**：从任意可达状态出发，存在有限的后续操作使 $\mathrm{orphan}(S) = \emptyset$；正常提交的裁剪不能产生没有回收路径的孤儿。
- $\mathbf{P_2}$ **初始化全前缀**：`init` 的每个崩溃前缀都能被下一次 `init` 接受或自动修复。
- $\mathbf{P_3}$ **判定完备**：`verify` 返回 0 时，存储与投影均一致；若 `.pending` 或 `pending_apply` 存在，`verify` 必须报告或修复。
- $\mathbf{P_4}$ **恢复安全**：恢复流程不得覆盖既不是旧内容也不是新内容的 $D(f)$ 而不留记录。
- $\mathbf{P_5}$ **类型承载**：$\mathbf{S_2}$ 与 $\mathbf{S_6}$ 是由类型签名推出的性质，而不是由调用点顺序保证的经验性质。
- $\mathbf{P_6}$ **裁剪成本**：`prune` 对单个文件的成本与全库版本总数无关，不超过 $O(k)$，其中 $k$ 是被删版本数。
- $\mathbf{P_7}$ **路径身份**：路径等价关系与文件系统实际打开的文件一致，包括大小写不敏感文件系统与符号链接下的 `..`。
- $\mathbf{P_8}$ **输出全域**：`--format json` 在任何输入上都只输出 JSON。

### 2.3 判定摘要

| ID | 结论 | 严重度 | 关联性质 |
|---|---|---|---|
| V1-1 | 提交裁剪与 `prune` 崩溃产生的孤儿对象没有回收路径 | P1 | $\mathbf{P_1}$ |
| V1-2 | `init` 崩溃可留下临时标记，下一次 `init` 拒绝该目录 | P1 | $\mathbf{P_2}$ |
| V1-3 | `verify` 不读取 `.pending` 与 `pending_apply`，共享锁读命令不恢复 | P1 | $\mathbf{P_3}$ |
| V1-4 | 崩溃后的恢复无条件覆盖目标文件，可丢弃外部编辑 | P1 | $\mathbf{P_4}$ |
| V1-5 | `verify --fix` 的索引修复忽略写入失败，仍报告已修复 | P2 | $\mathbf{P_3}$ |
| V1-6 | `verify` 不校验 `time` 格式与版本连续性 | P2 | $\mathbf{P_3}$ |
| V1-7 | 类型签名没有承载 `Ver`、`FileId` 与写回顺序的不变量 | P2 | $\mathbf{P_5}$ |
| V1-8 | 读命令在共享锁下写 `index.json` | P2 | $\mathbf{S_{10}}$ |
| V1-9 | 大小写与符号链接 `..` 下的路径身份与文件系统不一致 | P2 | $\mathbf{P_7}$ |
| V1-10 | 非 UTF-8 路径在写入对象之后才失败，留下孤儿 | P2 | $\mathbf{P_1}$ |
| V1-11 | `prune` 扫描全库版本，成本不满足定理 4 | P2 | $\mathbf{P_6}$ |
| V1-12 | CLI 缺少契约命令，且空 `prune` 的 JSON 输出退化为文本 | P2 | $\mathbf{P_8}$ |
| V1-13 | 清单的 `codec` 字段不参与读取分派 | P3 | 设计一致性 |
| V1-14 | 写回竞态下 `NotAFile` 被映射为 3 而不是 7 | P3 | 失败分类 |

## 3. 交叉论证

### 3.1 空间活性：状态机与可达性

`Objects` 只有两类转移会增加 $O$：对象写入 $\mathrm{Put}_O$，以及初始化。只有 `prune` 的候选移动会减少 $O$。设一次 `prune` 删除的版本集合为 $R$，候选集合为

$$C = \{\, b \in \mathrm{blob}(R) \mid \mathrm{count}(b) = 1 \,\}$$

其中 $\mathrm{count}(b)$ 是删除前的全局引用数。`prune` 只移动 $C$ 中的对象（`crates/bakon-core/src/store/mod.rs:460-475`），随后清空 `trash/`。

提交路径则不同：`prepare_commit` 直接调用 `Manifest::trim`（`crates/bakon-core/src/store/mod.rs:249-267`），`commit` 写入新清单（`crates/bakon-core/src/store/mod.rs:305-307`），但不计算、也不移动被裁掉版本引用的对象。设裁剪前清单为 $V$，裁剪后为 $V'$，则被移除的 blob 集合

$$B = \mathrm{blob}(V) \setminus \mathrm{blob}(V')$$

立即进入 $\mathrm{orphan}$。下一次 `prune` 看到 $|V'| = m$，直接 `continue`（`crates/bakon-core/src/store/mod.rs:444-447`），不会把 $B$ 当作候选。`verify --fix` 只清空 `objects/tmp` 与 `trash`（`crates/bakon-core/src/store/verify.rs:178-198`），从不扫描 `objects/aa/` 下的孤儿。于是从该状态出发，不存在任何删除 $B$ 的转移。反复提交使 $|\mathrm{orphan}|$ 单调增长，$\mathbf{P_1}$ 不成立。

同一论证覆盖两个崩溃前缀：

- `commit` 已写对象、尚未写清单时崩溃：对象进入 $\mathrm{orphan}$，且没有任何后续转移把它作为候选。
- `prune` 已写裁剪后的清单、尚未移动完候选时崩溃：剩余候选进入 $\mathrm{orphan}$，重跑 `prune` 因 $|V'| = m$ 而跳过。

因此这不是“回收延迟”，而是“回收不可达”。

### 3.2 初始化全前缀：原子替换与空目录判定

`initialize` 通过 `write_atomic` 在存储根目录写 `BAKON_STORE`（`crates/bakon-core/src/store/mod.rs:713-721`）。`write_atomic` 的临时文件放在 `tmp_dir`，而这里的 `tmp_dir` 正是存储根目录（`crates/bakon-core/src/fsutil.rs:33-61`）。崩溃点位于“临时文件已 `fsync`、尚未 `rename`”时，状态是 $I_2$。

`init` 的接受条件是：目录不存在、目录为空、或目录含合法标记。$I_2$ 三者都不满足，于是进入 `NotAStore`（`crates/bakon-core/src/store/mod.rs:119-134`）。`init` 没有清理已知临时文件前缀的转移，也没有把 $I_2$ 视为可修复前缀。因此 $\mathbf{P_2}$ 不成立。

### 3.3 判定完备与恢复安全：投影与共享锁

`verify` 的输入域是 `list_ids()` 返回的清单、对象内容、索引与两个残留目录（`crates/bakon-core/src/store/verify.rs:53-211`）。它不读取 `.pending`，也不检查 `pending_apply`。因此状态

$$S_p = (M, O, I, D, X) \quad \text{满足} \quad \mathrm{pending}(S_p) \neq \emptyset,\ D \neq \mathrm{content}(\mathrm{head})$$

仍在 `verify` 的“一致”纤维中。共享锁读命令不会调用 `recover`；只有 `begin_write` 才调用（`crates/bakon-core/src/store/mod.rs:173-178`）。于是 `verify` 返回 0，而 `log` 已显示一个尚未写回的 `revert` 版本。$\mathbf{P_3}$ 不成立。

恢复转移本身是

$$D(f) := \mathrm{content}(\mathrm{head}(M(f)))$$

（`crates/bakon-core/src/store/mod.rs:540-563`），没有对当前 $D(f)$ 的任何前置条件。若崩溃后外部程序把目标文件改成 $D'$，且 $D'$ 不属于 $M$，恢复会直接覆盖 $D'$，不产生版本。$\mathbf{P_4}$ 不成立。

`.pending` 的损坏同样落在 `verify` 的盲区：`read_pending` 只在写锁路径被调用（`crates/bakon-core/src/store/mod.rs:573-594`），所以一个非法的 `.pending` 可以让 `verify` 返回 0，而下一个写命令返回 6。

### 3.4 类型承载：设计判断与实现签名

设计声称 `apply` 只定义在 `Committed` 上，`FileId` 只能由 `generate` 产生，`Ver` 只能由 `next` 产生（[[API__store-contract-and-cli]] §1 与 §2，[[STATE__store-state-machine-and-invariants]] 的 $\mathbf{S_2}$、$\mathbf{S_6}$）。实际签名是：

- `pub fn write_target(path: &AbsPath, content: &[u8])` 是自由函数（`crates/bakon-core/src/apply.rs:15`），任何调用点都可以在提交前调用它。
- `Committed` 只有 `id`、`version`、`source` 三个访问器，没有 `apply` 方法（`crates/bakon-core/src/store/mod.rs:99-111`）。
- `Ver::new` 是 `pub`（`crates/bakon-core/src/types.rs:152`），并且 `Ver` 实现了 `Deserialize`（`crates/bakon-core/src/types.rs:181-186`）。
- `FileId::from_bytes` 是 `pub`（`crates/bakon-core/src/types.rs:105`）。

因此“写回先于提交不可表达”和“版本号只能来自清单”不是类型定理，而是当前调用点的顺序约定。回归用例能证明当前命令的顺序正确，但不能证明任意库调用者不能构造违反 $\mathbf{S_6}$ 的路径。$\mathbf{P_5}$ 不成立。这是设计与实现之间最重要的形式性差距，因为 Rust 重写的核心收益正是把 F3 与 F8 变成编译期错误。

### 3.5 成本、路径与输出

`prune` 先读取 `list_ids()` 的全部清单，再对所有清单的所有版本计数（`crates/bakon-core/src/store/mod.rs:423-437`）。即使只裁剪一个文件，成本也是

$$T_{\mathrm{prune}} = \Theta\left(\sum_{f \in \mathrm{dom}(M)} |V_f| + k\right)$$

而不是定理 4 声称的 $O(k)$。$\mathbf{P_6}$ 不成立。

`AbsPath` 的等价关系是清理后的 `PathBuf` 相等（`crates/bakon-core/src/types.rs:19-33`），`clean` 对 `..` 做纯词法折叠（`crates/bakon-core/src/types.rs:79-93`）。这有两个后果：

- 在大小写不敏感文件系统上，`C:\Foo` 与 `c:\foo` 是两个不同的 `AbsPath`，`resolve` 的路径比较也区分大小写（`crates/bakon-core/src/store/mod.rs:186-205`），同一个文件可以被纳管两次。
- 在符号链接路径中，`link/../x` 的词法结果是 `x`，内核解析结果可能是链接目标目录的兄弟文件；工具实际编辑的是词法结果，而不是内核结果。

$\mathbf{P_7}$ 在 Linux 上对普通路径成立，在大小写不敏感平台与含符号链接 `..` 的输入上不成立。

`prune` 在无版本可裁剪时直接输出 `"nothing to prune"`（`crates/bakon-cli/src/commands.rs:224-226`），不区分 `--format json`。$\mathbf{P_8}$ 不成立。

## 4. 边界与反例

以下实验均在本机 Linux/amd64、debug 构建上执行，使用隔离的 `--store` 与临时编辑器。

| 编号 | 操作 | 观测 | 结论 |
|---|---|---|---|
| R1 | `max_versions = 2`，连续 5 次 `edit` | 清单保留版本 4、5，`objects/` 有 6 个对象；`verify --fix` 返回 0；`prune` 输出 nothing；对象仍为 6 | V1-1 |
| R2 | `max_versions = 0` 建立 4 个版本，再用 `max_versions = 2` 执行 `prune`，在清单写入后崩溃 | 清单已截为版本 2、3，对象仍为 4；重跑 `prune` 输出 nothing，`verify --fix` 仍返回 0，对象仍为 4 | V1-1 |
| R3 | `BAKON_TEST_CRASH_AFTER=1 bakon init` | 存储根目录只剩 `BAKON_STORE.<pid>.<seq>.<rand>.tmp`；再次 `init` 返回 6，提示不是 bakon 存储 | V1-2 |
| R4 | 回退到版本 0，在 pending 清单写入后崩溃 | `.pending` 存在，`pending_apply = true`，目标仍是旧内容；`verify` 返回 0 且目标不变 | V1-3 |
| R5 | 在 R4 的崩溃后把目标改成用户内容，再执行一次 `edit` | 恢复先于编辑器执行，目标被改回 pending 版本的 `one`，用户内容没有版本记录 | V1-4 |
| R6 | 写入非法 `.pending`（`not-an-id`） | `verify` 返回 0；下一次 `edit` 返回 6 | V1-3 |
| R7 | 把 `index.json` 替换为目录，再执行 `verify --fix` | 输出 `index ... [fixed]`，退出 0；`index.json` 仍是目录 | V1-5 |
| R8 | 把清单中的 `time` 改为 `not-a-time`，把版本号改为 `0`、`2` | `verify` 返回 0，`log` 原样输出非法时间 | V1-6 |
| R9 | 用非 UTF-8 文件名执行 `edit` | 退出 70，`files/` 为 0 个清单，`objects/` 留下 1 个对象 | V1-10 |
| R10 | 在路径 `link/../x` 中令 `link` 指向 `real/` | 工具编辑 `targetdir/x`；内核语义应编辑 `real/x` | V1-9 |
| R11 | 删除 `index.json` 后执行 `log` | `log` 成功，并重新创建 `index.json` | V1-8 |
| R12 | `bakon --format json prune`，无可裁剪版本 | stdout 为 `nothing to prune`，不是 JSON | V1-12 |

R1 与 R2 说明孤儿不是“下一次 `prune` 会处理”的临时状态。R4 与 R5 说明 pending 状态在 `verify` 与恢复之间形成两个不同的可观测错误：一个报告一致，一个丢弃编辑。R9 说明非 UTF-8 路径在对象写入之后才失败，操作不是“失败即无变化”。

## 5. 落地指导

| 发现 | 可执行修改 | 验证方式 |
|---|---|---|
| V1-1 | 在 `commit` 的裁剪提交后，按全局引用计数把被移除版本的对象移入 `trash/`；或在 `prune` 与 `verify --fix` 中扫描 `objects/aa/` 的全部对象，删除不在 `ref(M)` 中的对象。顺序必须保持“清单先行，对象后删” | 新增回归：`max_versions = 2` 连续提交后，对象数等于引用数；在 `prune` 的每个崩溃点重开后，`verify --fix` 能收敛到同一对象数 |
| V1-2 | 标记临时文件使用固定前缀，并让 `init` 把“只有该前缀临时文件”的目录视为 $I_1$；或在 `init` 中先清理该前缀再判定空目录 | 对 `init` 枚举 `BAKON_TEST_CRASH_AFTER` 的每个点，重开后 `init` 均返回 0 |
| V1-3 | `verify` 增加 `.pending` 解析检查与 `pending_apply` 检查；只读 `verify` 将 pending 报告为不一致项，`verify --fix` 走写锁并执行恢复 | pending 状态、非法 `.pending`、pending 已清但文件未写三种用例 |
| V1-4 | `.pending` 记录 `{id, old_hash, new_hash}`；恢复前比较当前目标哈希：等于 `new_hash` 只清标记，等于 `old_hash` 才写回，其他值先记 `salvage` 或拒绝 | 崩溃后外部改写目标的用例，断言用户内容进入 `salvage` 或收到冲突退出 |
| V1-5 | 让 `save_index` 返回 `Result`，在 `verify --fix` 中传播；只有写入成功后才把 `fixed` 置为真 | 把 `index.json` 换成目录或只读文件，断言 `verify --fix` 返回 8 或 6，而不是 0 |
| V1-6 | `Manifest::validate` 校验 `time` 为 RFC 3339；校验版本号连续，或明确把空洞定义为合法并写入文档 | 非法时间与空洞清单的回归用例，断言退出 6 与具体项 |
| V1-7 | 把 `Ver::new`、`FileId::from_bytes` 收紧为 `pub(crate)` 或测试专用；把写回改为 `Committed::apply(self, ...)`；写接口要求 `&WriteGuard` | `trybuild` 编译失败用例：在 `Prepared` 上调用 `apply`、在未持锁时调用 `commit` 均不通过 |
| V1-8 | 读取路径不要写索引；把缓存修复留给 `verify --fix` 或显式升级为写锁。若保留该行为，必须在锁文档中声明为派生写例外 | 共享锁下删除索引并执行 `log`，断言不创建索引；或断言行为被文档化 |
| V1-9 | 定义平台路径键：Windows 与 macOS 默认大小写折叠；对 `..` 保留内核语义或拒绝含 `..` 的用户输入；不要用词法折叠后的路径替代实际打开路径 | Windows 上 `C:\Foo` 与 `c:\foo` 只能产生一个清单；含符号链接 `..` 的输入与 shell 行为一致 |
| V1-10 | 在 `AbsPath::new` 拒绝非 UTF-8 路径并返回用法错误；或把路径编码为可逆字节串。无论哪种，都必须在 `put_object` 之前完成校验 | 非 UTF-8 文件名用例，断言退出 2 且 `objects/` 无新增 |
| V1-11 | `prune` 只读取请求的清单；候选对象使用全局引用计数或一次全库引用扫描，但不重复加载全部清单。若成本保持不变，修改定理 4 的表述 | 构造 1 个目标文件与大量其他版本，测量 `prune <file>` 不随全库版本线性增长 |
| V1-12 | 补齐 `ls`、`path`、`store`；或从契约中删除它们。空 `prune` 按 `--format` 输出 JSON/plain | CLI 契约测试覆盖每个命令与 JSON 空结果 |
| V1-13 | 把 `Store::content` 改为接收 `VersionRecord`，按 `codec` 分派；或删除 `codec` 字段并明确只支持 deflate | 增加一个未来 codec 的单元测试，或删除扩展性承诺 |
| V1-14 | 写回失败统一映射为 `ApplyFailed`（7），包括 `NotAFile` 与写回阶段的 IO；恢复失败时保留 `.pending` 并报告 7 | 在预检与写回之间把目标替换为目录，断言 7 且 `.pending` 可恢复 |

V1-1、V1-3 与 V1-4 应优先于发布路径工作，因为它们分别对应空间不可控、判定假阳性与数据丢失。

## 6. 独立校验

### 6.1 已执行

- `cargo test --workspace --all-targets`：106 个测试通过（core 51、regressions 31、cli 集成 13、cli 单元 7、crash 3、concurrency 1）。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo fmt --all --check`：通过。
- R1 至 R12 的反例均以真实二进制与隔离存储复现；关键输出已列于第 4 节。

### 6.2 未覆盖

- Windows 与 macOS 的大小写不敏感、Unicode 归一化与 `cmd /C` 转义只做了静态推断，未在对应 runner 上构造路径身份反例。
- 大规模 `prune` 成本只由代码路径推断，未做 $10^5$ 量级测量。
- `trybuild` 类型的编译失败用例尚未加入，因此 V1-7 是签名层结论，不是测试结论。
- V1-9 的大小写部分与 V1-14 的写回竞态只做了代码路径推断，未在对应平台上构造反例；V1-9 的符号链接 `..` 部分已在 Linux 上复现。
- 目录 `fsync` 失败后 `rename` 已生效的“失败但已提交”语义未构造故障注入；这属于错误报告语义的 `[待确认]` 项。

### 6.3 最弱前提与待确认

- 所有结论假设单机本地文件系统、单写者遵守锁、存储目录不被外部主动篡改；后两项与 v1 设计的最弱前提一致。
- `[待确认]` 恢复是否允许覆盖崩溃后的外部编辑。当前实现选择覆盖；若保留该选择，必须把它写进用户可见文档，否则它与 F5 的“静默丢弃分歧”是同一类风险。
- `[待确认]` 读命令写派生索引是否算锁纪律的例外。若算例外，`S_{10}` 的表述需要放宽；若不算，`resolve` 不能在共享锁下调用 `save_index`。
- `[待确认]` 是否继续保留 `codec` 扩展点。若保留，读取路径必须按条目分派；若不保留，删除字段比保留一个不生效的承诺更诚实。
- `[待确认]` 路径规范化是否接受“词法清理优先于内核解析”。当前实现选择了词法清理；这会使含符号链接 `..` 的输入与 shell 行为不同。

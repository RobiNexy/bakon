# STATE__store-state-machine-and-invariants

本报告是整个重构的形式基础。它把 v1 存储定义为一个带崩溃切点的状态机，列出必须成立的不变量，并证明崩溃后状态的可判定性。后续四份报告各自展开其中一组：[[DATA__blob-store-and-durability]]、[[DATA__version-manifest-and-numbering]]、[[SEQUENCE__write-protocol-and-failure-paths]]、[[OPS__store-identity-recovery-and-retention]]。

## 1. 为什么用这个框架

系统的并发空间被两个设计选择压扁：写操作串行、编辑器会话期间不持锁。剩下的不确定性只有崩溃位置。因此正确性问题是「某个前缀是否可达」而不是「某个交错是否可达」，属于离散状态机与原子性模型，不属于并发算法范畴。

不使用概率模型：唯一随机量是 ID 生成，其碰撞概率可以按界计算，不作为正确性依据。不使用分布式模型：存储是单机单目录，没有复制与共识。

## 2. 状态

定义五元组

$$S = (M, O, I, D, X)$$

| 分量 | 类型 | 含义 |
|---|---|---|
| $M$ | $\mathrm{FileId} \rightharpoonup \mathrm{Manifest}$ | 权威的版本清单，一个文件一个 |
| $O$ | $\mathrm{Hash} \rightharpoonup \mathrm{Bytes}$ | 不可变对象库，内容寻址 |
| $I$ | $\mathrm{AbsPath} \rightharpoonup \mathrm{FileId}$ | 派生缓存，路径到身份的映射 |
| $D$ | $\mathrm{AbsPath} \rightharpoonup \mathrm{Bytes}$ | 目标文件，由外部（编辑器、其他程序）拥有 |
| $X$ | 运行期 | 锁与非持久状态，不属于持久语义 |

清单 $\mathrm{Manifest} = (id, path, cfg, V)$，其中 $cfg = (max, hook)$，$V = (v_0, \ldots, v_{n-1})$ 为按版本号升序的版本记录：

$$v_k = (\mathrm{ver}, \mathrm{blob}, \mathrm{size}, \mathrm{time}, \mathrm{source}, \mathrm{pending}, \mathrm{note})$$

派生量：$\mathrm{head}(m) = v_{n-1}$（$n \ge 1$），$\mathrm{maxver}(m) = \mathrm{ver}(v_{n-1})$。$\mathrm{note}$ 是可选注解，不是版本事实；它可以被 `bakon note` 单独修改，不影响其他分量。

## 3. 操作与原子片段

每个操作被分解为持久化片段。极小的持久化原语只有两个：

- $\mathrm{Put}_O(h, b)$：在 `objects/` 下创建对象，带临时文件、`fsync`、`rename`、目录 `fsync`。目标路径已存在则跳过。
- $\mathrm{Put}_M(m)$：把整个清单写到临时文件，`fsync`，`rename` 覆盖，目录 `fsync`。

以及一个非持久化但受保护的片段：$\mathrm{Write}_D(p, b)$，对目标文件的原子替换。

| 操作 | 片段序列 |
|---|---|
| 初始化 | 创建目录、$\mathrm{Put}_M$ 无、写入标记文件 |
| 纳管 | 生成 $id$、$\mathrm{Put}_O$（基线内容）、$\mathrm{Put}_M$（含 $v_0$）、更新 $I$ |
| 提交 | 选择来源内容、$\mathrm{Put}_O$、$\mathrm{Put}_M$（追加 $v_n$）、更新 $I$ |
| 回退 | 读 $\mathrm{head}$、$\mathrm{Put}_O$、$\mathrm{Put}_M$（追加 $v_n$ 且 $\mathrm{pending}=1$）、$\mathrm{Write}_D$、$\mathrm{Put}_M$（$\mathrm{pending}=0$） |
| 备注 | 读 $V$、$\mathrm{Put}_M$（只改 $v_k$ 的 $\mathrm{note}$，必要时切换清单 schema） |
| 裁剪 | 读 $V$、$\mathrm{Put}_M$（截断后缀）、移动并删除无引用对象 |
| 改名 | $\mathrm{Put}_M$（改 $path$）、更新 $I$ |
| 读 | 无持久化片段 |

## 4. 不变量

以下十二条是 v1 的正确性契约。每条都对应一个可检查的机制，而不是一句原则。

- $\mathbf{S_1}$ **身份注册**：$\mathrm{dom}(M)$ 与 $\mathrm{range}(I)$ 在无缓存滞后时相等；$I$ 是单射；$id$ 在生成后不再改变。
- $\mathbf{S_2}$ **清单权威**：任何版本事实都只能读自 $M$。版本号、来源、时间、大小不得从文件名、时间戳或目录顺序推导。
- $\mathbf{S_3}$ **编号单调**：$V$ 的版本号严格递增；追加时 $\mathrm{ver} = \mathrm{maxver}(m) + 1$；裁剪保留后缀，已分配的号不复用。
- $\mathbf{S_4}$ **内容完整**：$\forall (h, b) \in O$，`inflate` 后内容的 $\mathrm{SHA256}$ 等于 $h$。
- $\mathbf{S_5}$ **提交点唯一**：一次写事务中只有一个 $\mathrm{Put}_M$ 改变持久语义；崩溃后 $M$ 等于旧值或新值，不存在混合。
- $\mathbf{S_6}$ **破坏性写后置**：$\mathrm{Write}_D$ 只发生在对应版本已进入 $M$ 之后；否则该版本标记 $\mathrm{pending}=1$，由恢复流程完成。
- $\mathbf{S_7}$ **缓存安全**：任意 $I$ 内容都不影响只读结果的正确性，只影响查找速度；$I$ 缺失或损坏时可由 $M$ 重建。
- $\mathbf{S_8}$ **保留**：裁剪后 $|V| = \min(|V|, \max(m))$，保留的是最新后缀；被移除条目引用的对象在无其他引用时可回收。
- $\mathbf{S_9}$ **格式门**：操作前校验标记与清单的 `schema`；高于本程序的 schema 一律拒绝，不做猜测性解析。
- $\mathbf{S_{10}}$ **锁纪律**：写持排他锁，读持共享锁，编辑器会话不持锁；提交时用「进入编辑前的 $\mathrm{head}$」做比较交换，不一致则冲突退出。
- $\mathbf{S_{11}}$ **钩子位置**：钩子在提交点之后、锁释放之后执行；至多一次；失败不回滚版本。
- $\mathbf{S_{12}}$ **错误分类**：每个错误类型唯一映射到一个退出码，映射由类型决定，不由消息文本决定。

## 5. 提交点定理

**定理 1（崩溃可判定）** 设一次操作为片段序列 $f_1, \ldots, f_n$。若在任意片段边界崩溃，则存在某个 $k \in \{0, \ldots, n\}$ 使持久状态等于执行前 $k$ 个片段后的状态，且 $\mathbf{S_1}$ 至 $\mathbf{S_9}$ 成立。

*证明骨架。* 按片段类型分类：

- $\mathrm{Put}_O$ 只增加不可达或新增对象，不改变 $M$，故 $S_2$、$S_3$ 不受影响；新对象若未被引用，只影响空间（$\mathbf{S_8}$ 要求它可回收，不要求它立即消失）。
- $\mathrm{Put}_M$ 是 `rename` 后的整体替换。操作系统保证读者看到旧文件或新文件之一，因此 $M$ 始终是合法清单，$\mathbf{S_2}$ 与 $\mathbf{S_3}$ 自动成立。
- $\mathrm{Write}_D$ 不是提交点。它只可能把 $D$ 推进到「与某个已持久版本相同」的状态；$\mathrm{pending}$ 标记使「文件已改、版本未记」不可能出现（$\mathbf{S_6}$）。
- 更新 $I$ 是派生写，可丢失（$\mathbf{S_7}$）。
- 崩溃点落在两个 $\mathrm{Put}_M$ 之间时，$M$ 处于 $\mathrm{pending}=1$ 的合法状态，恢复流程补齐 $\mathrm{Write}_D$ 并清标记。

因此任意前缀都对应一个可达状态，且不变量未被破坏。$\square$

**定理 2（不产生无记录破坏）** 若崩溃发生在任意 $\mathrm{Write}_D$ 之前，则 $D$ 未被本次操作修改；若发生在之后，则对应版本已在 $M$ 中。故不存在「目标文件已变而版本不存在」的状态，`edit` 除外——`edit` 的写由编辑器施加，属于用户意图，见 [[SEQUENCE__write-protocol-and-failure-paths]]。

**定理 3（无别名）** 设 $id$ 在纳管时随机生成并检查唯一性，则 $id$ 不是路径的函数。因此「路径被腾空后再次纳管」不会产生同一 $id$，$\mathbf{S_1}$ 与版本号交错问题同时消失。

**定理 4（裁剪成本）** 裁剪的持久化片段为「一次 $\mathrm{Put}_M$ + 至多 $|V| - \max$ 次对象删除」，与全库版本总数无关。为了判定对象是否仍被引用，命令需要一次全库引用扫描；这是 CPU 成本，不是持久化片段。没有任何版本可删的单文件 `prune` 只读取目标清单，不扫描全库。对照 0.x：每次裁剪需重放全库提交，成本 $O(|\mathcal{G}|)$，实测 100 次提交时约 750 ms。

## 6. 崩溃点枚举

| 崩溃位置 | 持久后果 | 恢复动作 |
|---|---|---|
| 对象写入中途 | 残留临时文件，无引用 | 启动清理 `objects/tmp/` |
| $\mathrm{Put}_O$ 之后、$\mathrm{Put}_M$ 之前 | 孤儿对象 | 下次裁剪或 `verify --fix` 回收 |
| $\mathrm{Put}_M$ 之后、更新 $I$ 之前 | 缓存陈旧 | 查找未命中时扫描清单并修复 |
| $\mathrm{Put}_M(\mathrm{pending}=1)$ 之后、$\mathrm{Write}_D$ 之前 | 文件未更新，版本已存在 | 恢复流程写回并清标记 |
| $\mathrm{Write}_D$ 之后、$\mathrm{pending}=0$ 之前 | 文件已更新，标记残留 | 恢复流程重复写回（幂等）并清标记 |
| 裁剪中删除对象时 | 部分对象已删，清单已先更新 | 残留对象无害；清单不引用它们 |

## 7. 与 0.x 审计的对应

| 旧缺陷 | 在 v1 中的状态 |
|---|---|
| F1 重写无关仓库 | 不可表达：没有提交链，裁剪只改自己的清单 |
| F2 路径随工作树偏移 | 不可表达：不调用 git，不依赖工作树 |
| F3 写回先于持久化 | 由 $\mathbf{S_6}$ 与定理 2 排除 |
| F4 rename 覆盖符号链接 | 写回策略改为「按目标类型分派」，见 [[SEQUENCE__write-protocol-and-failure-paths]] |
| F5 静默丢弃分歧 | 目标偏离 head 且目标是 head 时默认拒绝，退出码 4，`--force` 才覆盖 |
| F6 索引先于基线 | 由 $\mathbf{S_7}$ 排除：清单先行，索引是缓存 |
| F7 verify 漏检 | 由 [[API__store-contract-and-cli]] 的检查表覆盖 |
| F8 mv 后 ID 别名 | 由定理 3 排除 |
| F9 `max_versions=0` 不可表达 | 由 $\mathrm{Option}$ 语义表达「未设置」，0 表示不限制 |
| F10 diff 错误映射为 1 | 由 $\mathbf{S_{12}}$ 与按命令的退出码矩阵排除 |
| F11 钩子死锁 | 由 $\mathbf{S_{11}}$ 排除：钩子在锁外执行 |
| F12 裁剪成本 | 由定理 4 排除 |
| F13 `GIT_DIR` 劫持 | 不可表达：无子进程 |

## 8. 最弱前提

- 文件系统提供原子 `rename` 与目录 `fsync`（POSIX 语义；Windows 需 `ReplaceFile` 或 `MoveFileEx` 的等价语义）。
- 单机单文件系统；不支持网络文件系统上的可靠锁。
- $\mathrm{SHA256}$ 抗第二原像；本设计只用它做内容标识与完整性校验，不用于安全边界。
- 进程崩溃与断电被同等对待；不建模磁盘固件层的数据丢失。

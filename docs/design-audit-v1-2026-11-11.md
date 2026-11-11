# Bakon v1 全量形式化审计（2026-11-11）

审计对象是 `main` 上的 v1 实现（Rust 1.0.1，21 个 `.rs` 文件，6,838 行物理行）与 `design/` 下的契约。前两次审计见 [[design-audit-v1-2026-11-10]] 与 [[design-audit-v1-2026-11-10-recheck]]；本文不重复已修复的 V1-1 至 V1-14，而是逐模块、逐函数重新建立契约，并判定一份外部代码评审的 10 项意见。

## 1. 框架选择

主框架仍是带崩溃切点的离散状态机。写操作由单写锁串行化，编辑器会话不持锁，因此主要不确定性来自持久化前缀，而不是任意并发交错。辅助框架有四项：

- 关系与可达性：对象集合与清单引用集合的差，用于判定孤儿、临时文件与 `.pending` 的可回收性。
- 类型论：检查设计声称的编译期不变量是否由签名承载，还是仍由调用点顺序承载。
- 序理论：版本号严格递增、裁剪保留后缀、恢复追加版本的单调性。
- 信息流与输出契约：目标内容进入存储后的可见性（权限），以及 `--format json` 的输出全域。

`[另一解]` 把存储视为只追加日志，把恢复视为日志重放，把 `.pending` 视为未提交事务的 redo 记录；结论相同：恢复必须是幂等闭包，不能在没有完成或证明 no-op 的情况下删除 redo 记录。本文使用集合与可达性，因为它能直接对应 `put`、`save_manifest`、`move_to_trash` 与 `clear_pending`。

不使用概率模型：随机量只有 `FileId` 与临时文件名，碰撞概率不影响下述结论。不建模磁盘固件层的数据丢失；所有结论以“读到的字节要么正确、要么可被哈希判定为错误”为前提。

## 2. 形式化陈述

### 2.1 状态与派生量

在原有五元组上增加 pending 日志与临时工件：

$$S=(M,O,I,D,X,P,T)$$

| 分量 | 类型 | 含义 |
|---|---|---|
| $M$ | $\mathrm{FileId}\rightharpoonup\mathrm{Manifest}$ | 权威清单 |
| $O$ | $\mathrm{BlobHash}\rightharpoonup\mathrm{Bytes}$ | 内容寻址对象 |
| $I$ | $\mathrm{AbsPath}\rightharpoonup\mathrm{FileId}$ | 派生索引缓存 |
| $D$ | $\mathrm{AbsPath}\rightharpoonup\mathrm{Bytes}$ | 目标文件投影 |
| $X$ | 运行期 | 锁与进程状态 |
| $P$ | $\mathrm{FileId}\times\mathrm{Ver}\times\mathrm{Option}(\mathrm{Hash})\times\mathrm{Hash}$ 的有限序列 | `.pending` 记录 |
| $T$ | 文件名集合 | `objects/tmp`、`files/`、根目录中的临时文件 |

派生量：

$$\mathrm{ref}(M)=\{\,r.\mathrm{blob}\mid (f,m)\in M,\ r\in m.\mathrm{versions}\,\}$$

$$\mathrm{orphan}(S)=O\setminus\mathrm{ref}(M)$$

$$\mathrm{pending}(M)=\{\,r\mid (f,m)\in M,\ r\in m.\mathrm{versions},\ r.\mathrm{pending\_apply}\,\}$$

$$\mathrm{projected}(S)=\{\,f\mid D(f)=\mathrm{content}(\mathrm{head}(M(f)))\,\}$$

恢复关系 $R$ 定义为：对每个 $p\in P$，若 $M(p.\mathrm{id})$ 中存在 pending 版本且目标内容与 $p$ 的新旧哈希可判定，则执行写回、salvage 或 no-op，并在完成后删除 $p$；若无法判定，则保留 $p$ 并报告。

### 2.2 待判定性质

- $\mathbf{P_1}$ **空间活性**：从任意可达状态出发，存在有限转移使 $\mathrm{orphan}(S)=\emptyset$ 且临时文件集合 $T$ 收敛；正常提交与 `prune` 不能制造无回收路径的孤儿。
- $\mathbf{P_2}$ **初始化与归属**：`init` 的每个崩溃前缀都能被下一次 `init` 接受或修复；`init` 不得删除任何不属于本程序的普通文件。
- $\mathbf{P_3}$ **判定完备**：`verify` 返回 0 时，不存在未报告的 `.pending`、`pending_apply`、孤儿、索引不一致或损坏清单；若存在无法修复的项，必须返回非零并保留证据。
- $\mathbf{P_4}$ **恢复安全与幂等**：恢复不得覆盖既非旧值也非新值的 $D$ 而不留记录；无法完成的恢复必须保留 `.pending`；$R(R(S))=R(S)$。
- $\mathbf{P_5}$ **类型承载**：写锁、写回顺序、身份注册与路径等价由签名或构造器承载，而不是由调用点约定承载。
- $\mathbf{P_6}$ **裁剪成本**：无删除的单文件 `prune` 不随全库版本数增长；实际裁剪的持久化片段为 $O(k)$。
- $\mathbf{P_7}$ **路径身份**：`AbsPath` 的等价关系与内核实际打开的文件一致，包括 `..`、符号链接与大小写不敏感卷。
- $\mathbf{P_8}$ **输出全域**：`--format json` 对支持 JSON 的命令只输出 JSON；全局参数在文档承诺的位置可用。
- $\mathbf{P_9}$ **身份注册**：同一路径键最多对应一个清单；同一 `FileId` 只对应一个清单；重复不静默合并。
- $\mathbf{P_{10}}$ **并发提交点**：编辑器会话结束后，`expected_head` 比较交换拒绝任何被其他写者推进的提交；冲突保留磁盘内容。
- $\mathbf{P_{11}}$ **本地机密性**：存储中的对象继承目标文件的访问边界，至少不因工具而扩大可读范围。
- $\mathbf{P_{12}}$ **临时工件收敛**：写入失败或崩溃留下的临时文件可被 `init`、`verify` 或下一次写入识别并清理。

### 2.3 判定摘要

| ID | 结论 | 严重度 | 关联性质 |
|---|---|---|---|
| A-01 | `init` 把 `BAKON_STORE.*.tmp` 形状的普通文件当作自己的临时文件删除 | P1（已修复） | $\mathbf{P_2}$ |
| A-02 | 恢复对缺失清单或未提交 pending 返回成功并删除 `.pending`；`verify --fix` 随后报告一致 | P1（已修复） | $\mathbf{P_3},\mathbf{P_4}$ |
| A-03 | `revert` 的“已匹配”分支经 `Committed::apply` 写回，没有 `.pending` 保护 | P2（已修复） | $\mathbf{P_4},\mathbf{P_5}$ |
| A-04 | 公共 `prepare_track`/`commit` 不检查路径唯一，可产生两份同路径清单 | P2（已修复） | $\mathbf{P_5},\mathbf{P_9}$ |
| A-05 | `prune` 接收重复 `FileId` 时计数下溢，debug 构建 panic | P2（已修复） | $\mathbf{P_1},\mathbf{P_6}$ |
| A-06 | `Store::open` 等价于 `init`；读命令会在错误路径上创建存储，`verify` 还会返回 0 | P2（已修复） | $\mathbf{P_2},\mathbf{P_3}$ |
| A-07 | `..` 前面是普通文件或缺失路径时仍按词法折叠，路径身份与内核不一致 | P2 | $\mathbf{P_7}$ |
| A-08 | 对象与存储目录使用默认 `umask` 权限；0600 文件内容可写成 0664 对象 | P2 | $\mathbf{P_{11}}$ |
| A-09 | `StoreLock` 不区分读写；写接口不要求 guard；`Store::committed` 可绕过 pending | P2 | $\mathbf{P_5}$ |
| A-10 | 测试套件依赖 `sh`、可执行脚本与 `/tmp` 断言；Windows 全量测试不可用 | P2 | 跨平台质量门 |
| A-11 | 一份损坏清单使 `verify` 在孤儿扫描处整体失败，已收集的报告项丢失 | P2 | $\mathbf{P_3}$ |
| A-12 | `hook set/unset`、`config init` 忽略 `--format json`；全局 `--no-hook`/`--no-color` 未实现 | P2 | $\mathbf{P_8}$ |
| A-13 | 只读存储无法读取：共享锁仍以读写方式打开 `.lock` | P3 | $\mathbf{P_8}$ |
| A-14 | 清单头为 `u32::MAX` 时 `verify` 接受，下一次提交返回内部错误 70 | P3 | $\mathbf{P_3}$ |
| A-15 | `files/`、根目录中的清单/索引/pending 临时文件没有清理路径 | P3 | $\mathbf{P_1},\mathbf{P_{12}}$ |
| A-16 | `.lock` 只记录 PID，未记录设计承诺的开始时间；写入截断非原子 | P3 | 诊断契约 |
| A-17 | `diff` 单参数帮助文本与实际方向相反 | P3 | $\mathbf{P_8}$ |
| A-18 | `revert`/`show`/`diff` 绕过 `content_for`，未来 codec 会解码错误 | P3 | 设计一致性 |
| A-19 | `Committed::trim_failed` 只有布尔，丢失对象哈希与失败原因 | P3 | 诊断契约 |
| A-20 | 目录 `fsync` 失败发生在 `rename` 之后时，命令报告失败但提交可能已可见 | P2（已裁决：退出 9） | $\mathbf{P_3}$ |

次要观察 O-01 至 O-13 列于第 6.4 节，不再单独占用主表。

## 3. 逐模块审计

### 3.1 `crates/bakon-core/src/types.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `AbsPath::new` | 绝对化、拒绝非 UTF-8、拒绝经过符号链接的 `..`、词法清理 | 基本成立；对普通文件/缺失前缀后的 `..` 仍折叠，见 A-07 |
| `AbsPath::join` | 以本路径为根拼接并清理 | 仅供内部固定组件使用；不检查符号链接，不能作为公共安全构造器 |
| `AbsPath::as_path` / `Display` / `AsRef` | 只读访问 | 直接 |
| `AbsPath::key` | 平台路径身份键 | Windows/macOS 一律小写；大小写敏感卷会误合并，跨平台未验证 |
| `AbsPath` 的 `Serialize` / `Deserialize` | UTF-8 绝对路径 | 严格；反序列化会调用 `new`，对含 `..` 的陈旧清单可能做文件系统检查 |
| `reject_symlinked_parent_traversal` | 拒绝符号链接后的 `..` | 对符号链接成立；对普通文件与缺失前缀不成立，见 A-07 |
| `clean` | 词法清理 `.` 与 `..` | 不解析符号链接；是 A-07 的直接来源 |
| `FileId::generate` | 生成 128 位随机身份 | 随机源可用；唯一性由调用方检查，见 A-04 |
| `FileId::as_bytes` / `to_hex` / `from_hex` | 编码与解析 | 严格小写十六进制；`from_bytes` 已收紧为测试专用 |
| `Ver::ZERO` / `parse` / `new` / `next` / `get` | 版本号与唯一递增 | `next` 使用 `checked_add`；溢出映射为内部错误而非损坏，见 A-14 |
| `BlobHash::from_bytes` / `as_bytes` / `to_hex` / `from_hex` | 内容哈希 | 构造自由；哈希与内容的一致性只在对象读取时校验 |
| `hex_encode` / `hex_decode` | 十六进制 | 严格长度与小写；实现正确 |
| `uuid::v4_bytes` / `format` / `is_valid` | UUID 生成与格式 | 版本位设置正确；`is_valid` 只校验格式，不校验版本/变体位 |

### 3.2 `crates/bakon-core/src/error.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `Error` 枚举 | 语义分类 | 分类清晰；`Usage` 同时存在于核心与 CLI，文档枚举未列 `Usage`/`AlreadyManaged` |
| `Error::exit_code` | 唯一退出码映射 | 穷尽匹配；`NotAFile` 为 3，写回路径由 `applied`/`apply_pending` 映射为 7，持久性未确认为 9（A-20 已实现） |
| `Display` / `source` / `From<io::Error>` | 展示与错误链 | 直接；`Corrupt` 的 `detail` 不保留底层 `serde` 错误类型 |
| `Result` | 统一结果 | 直接 |

### 3.3 `crates/bakon-core/src/clock.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `now_rfc3339` | UTC RFC 3339 时间 | 正确；格式化失败映射为内部错误；时钟回拨不影响版本序 |

### 3.4 `crates/bakon-core/src/fsutil.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `fsync_dir` | 目录项持久化 | Unix 正确；非 Unix 为空操作，已在设计中声明 |
| `write_atomic` | 临时文件、写入、`fsync`、`rename`、父目录 `fsync` | 基本协议正确；不 `fsync` 临时目录（可接受）；父目录 `fsync` 失败发生在 `rename` 后，见 A-20 |
| `write_and_sync` | 写满并 `fsync` | 正确；崩溃点插在此后 |
| `durability_point` | 测试崩溃注入 | debug 有效，release 空操作；计数为进程内全局 |
| `temporary_path` | 临时文件名 | PID、序号、随机数；碰撞时 `create_new` 直接失败而不重试 |
| `remove_dir_contents` | 清空目录但保留目录 | 不跟随符号链接；不 `fsync`；条目消失时返回错误 |

临时文件的权限由 `OpenOptions` 默认值与 `umask` 决定，见 A-08。清单、索引与 pending 的临时文件位于各自目标目录，见 A-15。

### 3.5 `crates/bakon-core/src/manifest.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `Manifest::baseline` | 构造单版本清单 | 正确；字段公开，构造本身不校验 |
| `head` / `max_ver` / `oldest_ver` / `find` | 版本查询 | 直接 |
| `append` | 追加版本 | 仅 `debug_assert` 检查递增；release 下依赖调用点 |
| `trim` | 保留最新 $m$ 个，0 为不限 | 正确；保留后缀，不重编号 |
| `effective_max_versions` | 文件级三态与全局默认 | 正确 |
| `head_mut` | 可变头记录 | 供 pending 标记使用；公开可绕过校验 |
| `validate` | schema、非空、严格连续、时间格式 | 基本成立；不检查头 `< u32::MAX`（A-14）、UTC、时间序与 `pending_apply` 来源（O-01） |
| `Retention::is_inherit` / `from_max_versions` | 三态保留 | 正确；`NonZeroU32` 构造有 `expect`，但分支已排除 0 |
| `Retention` 的 `Serialize` / `Deserialize` | JSON 缺失/0/n | 正确；`deny_unknown_fields` |
| `VersionRecord::new` | 版本记录 | 正确；`size` 与 `blob` 的一致性由调用点与 verify 保证 |
| `Codec` / `Source` | 枚举 | 仅 `Deflate` 与四种来源；严格小写 |

### 3.6 `crates/bakon-core/src/object/mod.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `ObjectStore::new` | 构造对象根 | 直接 |
| `put` | 内容寻址写入，重复写入为 no-op | 基本成立；已存在路径不校验内容（O-04）；默认权限见 A-08 |
| `get` | 读取、解压、哈希校验 | 正确；缺失/解压失败/哈希不符均为 `Corrupt` 6；不限制解压后大小 |
| `path` | `objects/aa/<rest>` | 正确；哈希为小写十六进制 |
| `list_hashes` | 扫描两层分片 | 忽略非法名称；不报告损坏/多余条目（O-05） |
| `cleanup_tmp` | 清空 `objects/tmp` | 正确；仅需写锁的约定未由类型承载 |
| `hash_bytes` | SHA-256 | 正确 |
| `deflate` | 确定性 deflate | 依赖 `flate2` 固定级别的确定性；未跨版本验证 |

### 3.7 `crates/bakon-core/src/store/marker.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `Marker::new` | 生成 schema、UUID、时间、工具标识 | 正确；UUID 不参与存储内容绑定 |
| `Marker::validate` | schema 优先，UUID/时间/工具字段 | 正确；不校验 UUID 版本位与时间 UTC（次要） |
| `TOOL_ID` | `bakon/<major>.<minor>` | 正确；1.0.1 仍为 `bakon/1.0` |

### 3.8 `crates/bakon-core/src/apply.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `write_target` | 按目标类型分派 | 正确；目录/设备/悬空链接拒绝；符号链接写穿并保留链接 |
| `write_existing` | 原地截断写入并 `fsync` | 保留 inode/属主/权限；截断后失败会留下部分内容，见 A-03；硬链接会连带修改，见 O-03 |
| `create_new` | 0644 创建缺失文件并 `fsync` 文件与目录 | 正确；父目录缺失返回 8，写回路径映射为 7 |
| `is_dangling_symlink` | 区分缺失与悬空链接 | 基本正确；符号链接环在 `metadata` 阶段返回 8 而非 3 |

### 3.9 `crates/bakon-core/src/store/mod.rs`

本模块按职责分表。

**打开、初始化与布局**

| 函数 | 契约 | 结论 |
|---|---|---|
| `Store::init` | 不存在/空目录初始化；非空无标记拒绝；标记存在则校验 | 基本正确；临时文件判定过宽，见 A-01 |
| `Store::compose` / `Store::open` | 组装；打开已存在存储 | `open` 直接调用 `init`，见 A-06 |
| `root` / `marker` / `set_default_max_versions` | 访问器 | 直接；默认上限未校验 |
| `initialize` | 清临时标记、写标记、建布局 | 正确；崩溃前缀可由下一次 init 修复 |
| `ensure_layout` | 创建四个子目录并 `fsync` 根 | 基本正确；嵌套目录的父目录未逐一 `fsync` |
| `is_empty_dir` / `is_init_temp` | 空目录判定与标记临时文件识别 | 前缀过宽，见 A-01 |
| `remove_init_temps` | 删除标记临时文件 | 同 A-01 |
| `read_marker` | 严格解析标记 | 正确 |

**锁**

| 函数 | 契约 | 结论 |
|---|---|---|
| `begin_write` | 排他锁并恢复 | 正确；恢复失败会释放锁 |
| `begin_verify` | 排他锁但不自动恢复 | 供 `verify --fix` 先报告 `pending_stale` 再处理 `.pending` |
| `lock_shared` | 共享锁，不恢复 | 正确；OPS 已改为“写命令启动时恢复”（O-11 已修正） |
| `acquire` | 轮询、超时、写 holder | 基本正确；共享锁仍写打开 `.lock`，见 A-13 |
| `write_holder` / `read_holder` | PID 诊断 | 截断重写非原子；无开始时间，见 A-16 |
| `StoreLock::drop` | 解锁 | 正确；内核在进程崩溃时释放 |

**读、解析与索引**

| 函数 | 契约 | 结论 |
|---|---|---|
| `resolve` | 路径到身份；缓存未命中扫描 | 正确；$O(n)$ 是文档化的缓存失效成本；`&mut` 只用于内存缓存，不持久化（外部评审 R3 部分成立） |
| `manifest` / `load_manifest_optional` | 读取并校验清单 | 正确；id 与文件名一致性检查在 load 中 |
| `content` / `content_for` | 对象读取与 codec 分派 | 正确；CLI 有绕过，见 A-18 |
| `committed` | 为已存在版本构造写回句柄 | 公共构造器扩大写回面，见 A-03/A-09 |
| `head` / `all_ids` / `list_ids` | 查询 | 直接；`list_ids` 忽略非法文件名 |
| `manifest_path` / `index_path` | 路径构造 | 直接 |
| `index_cache` / `load_index` / `save_index` / `remember` | 派生缓存 | 读失败退化为空；写失败忽略；`rename` 后若 `remember` 提前返回，磁盘旧键可能保留（O-13） |

**写、提交与改名**

| 函数 | 契约 | 结论 |
|---|---|---|
| `prepare_track` | 生成身份与基线提交 | 不检查路径唯一，见 A-04 |
| `prepare_commit` | 追加版本并裁剪 | 正确；调用方须持写锁的约定未类型化 |
| `commit` | 对象先行、清单原子替换、回收裁剪对象 | 幂等与崩溃语义基本正确；`expected_head` 的 `(None,Some)`/`(Some,None)` 统一为内部错误，消息含糊（R2） |
| `save_manifest` | 原子写清单 | 正确；临时文件残留见 A-15 |
| `next_file_id` | 随机身份去重 | 只检查文件存在；碰撞概率可忽略 |
| `rename` | 只改清单路径并修索引 | 正确；无锁签名，见 A-09 |
| `set_hook` | 写清单钩子 | 正确；无锁签名 |

**裁剪与回收**

| 函数 | 契约 | 结论 |
|---|---|---|
| `prune` | 选择清单、裁剪、移动候选、清 trash | 基本正确；重复 id 已去重并用 checked 计数（A-05 已修复）；全库引用扫描的成本仍与全库版本数相关（V1-11 残余） |
| `move_to_trash` | `rename` 到 `trash` | 正确；未 `fsync` 目录，崩溃后最多留下可重试残留 |
| `referenced_blobs` | 全库引用集 | 正确；任一清单损坏即整体失败 |
| `reclaim_removed` | 回收被裁剪且无引用对象 | 正确；失败只留下孤儿，由后续 verify/prune 收敛 |
| `reclaim_orphans` | 扫描全部对象并回收 | 正确；`list_hashes` 的盲区见 O-05 |
| `orphan_hashes` / `objects` | 只读辅助 | 直接 |

**pending 与恢复**

| 函数 | 契约 | 结论 |
|---|---|---|
| `recover` | 逐条恢复、清 pending、清 tmp/trash | 已按 A-02 收紧：无法证明的记录保留并退出 6；可证明未提交的记录丢弃并由 `verify --fix` 报告 |
| `recover_entry` | 按 `entry.version` 匹配并校验 `new_blob` | 已修复：不再取最后一个 `pending_apply`；清单缺失、裁剪或不匹配时保留 `.pending` |
| `append_revert_and_apply` | salvage 后追加 revert 并写回 | 崩溃在 `write_pending` 与 `commit` 之间会留下未提交条目；下一次按“清单头小于记录版本”判为 `pending_stale` |
| `apply_pending` | 写回并把 Io/NotAFile 映射为 7 | 正确 |
| `clear_pending_version` / `finish_apply` | 清版本标记与 `.pending` | 正确；`finish_apply` 只清头，不校验被应用版本（O-06） |
| `pending_path` / `read_pending` / `write_pending` / `clear_pending` | `.pending` 读写 | 严格 schema；空文件视为空；写为原子替换 |
| `begin_apply` | 写 pending 记录 | 不校验版本/头，公共可误用（O-06） |
| `read_recovery_target` | 读取目标；缺失/非普通文件 | 正确；非普通文件错误未映射为 7（O-07） |

### 3.10 `crates/bakon-core/src/store/verify.rs`

| 函数 | 契约 | 结论 |
|---|---|---|
| `VerifyOptions` / `VerifyItem` / `SizeEntry` / `VerifyReport` | 报告结构 | 直接 |
| `VerifyReport::ok` | 所有项已修复 | 正确；无法修复项保持 false |
| `Store::verify` | 清单、pending、索引、孤儿、残留、尺寸 | 基本正确；损坏清单在孤儿扫描处整体失败，见 A-11；`--fix` 用 `begin_verify` 先报告 `pending_stale`，无法证明的记录保留并退出 6（A-02 已修复） |

`verify` 不检查目标文件是否等于 head；这是设计选择（外部编辑是合法状态），不是缺陷，但 OPS 的“任意命令启动时恢复”与 API 的“verify 一致”措辞需要区分“存储一致”与“投影一致”。

### 3.11 CLI：`config.rs`、`cli.rs`、`commands.rs`、`lib.rs`、`main.rs`

**配置**

| 函数 | 契约 | 结论 |
|---|---|---|
| `Config::store_path` / `max_versions` | 配置取值 | 正确；相对 `store` 相对当前工作目录解析，未相对配置文件 |
| `config::path` | 位置优先级 | 正确；与设计一致 |
| `config::load` | TOML 解析，缺失取默认 | 正确；未知字段被忽略（O-08） |
| `config::init` | 写默认模板，已存在不覆盖 | 正确；非原子、无 `fsync`（O-08） |

**语法与分发**

| 函数 | 契约 | 结论 |
|---|---|---|
| `Cli::parse_args` | clap 解析 | 正确；clap 错误直接退出 2 |
| `Format` / `Command` / `HookAction` / `StoreAction` / `ConfigAction` | 命令模型 | 基本正确；`diff` 参数命名与帮助见 A-17；全局 `--no-hook`/`--no-color` 缺失，见 A-12 |
| `run` / `dispatch` | 解析、执行、错误到退出码 | 正确；错误写 stderr，数据写 stdout 的约定基本成立 |

**命令**

| 函数 | 契约 | 结论 |
|---|---|---|
| `init` | 初始化并输出 | 正确；未持锁（O-12） |
| `version` | 版本与目标 | 正确 |
| `edit` | 纳管、编辑器、CAS 提交、钩子 | 主路径正确；编辑器后路径被改名/重纳管返回内部错误 70（O-02）；读到的编辑内容在恢复后未重读，但 `expected_head` CAS 会阻止已推进头的提交（本次未构造反例） |
| `revert` | 分歧检查、salvage、pending、写回、钩子 | 主路径正确；“已匹配”且目标存在时 no-op（A-03 已修复）；读取目标版本用 `content` 而非 `content_for`，见 A-18；`finish_apply` 失败映射为 8/9（O-07 已修正） |
| `prune` | 共享/排他锁、单文件/全库、输出 | 正确；库层重复 id 已由 A-05 修复 |
| `mv` | 改映射 | 正确；`rename` 的锁约定见 A-09 |
| `hook` | show/set/unset | 语义正确；JSON 输出缺失，见 A-12 |
| `config` | show/init | 语义正确；init 忽略格式，见 A-12 |
| `verify` | 只读/修复、格式、退出码 | 主路径正确；损坏清单时报告丢失，见 A-11；一致性提示先于 `[fixed]` 列表，属 UX 次要项 |
| `log` | 版本列表，最新在前 | 正确；时间显示收敛到秒 |
| `ls` | 全部路径 | 正确；重复路径被 `BTreeMap` 静默折叠（A-04 的投影） |
| `path` | 路径、id、清单路径 | 正确 |
| `store` / `tree_size` | 根目录与占用 | 正确；符号链接不计入；递归深度未限制（次要） |
| `show` / `diff` | 原始字节与统一 diff | 语义正确；`show`/`diff` 用 `content` 而非 `content_for`（A-18）；broken pipe 映射为 8（O-09） |
| `find_version` / `previous_version` | 版本查找与前一版 | 正确；`NoSuchVersion`/`Pruned` 区分清晰 |
| `source_name` / `display_time` / `as_text` | 格式化与二进制判定 | 正确 |
| `read_target` / `read_if_exists` | 目标读取 | 正确；非普通文件与悬空链接区分 |
| `warn_trim` / `applied` | 裁剪警告与写回映射 | 正确；`trim_failed` 信息不足见 A-19 |
| `run_hook` / `run_shell_with_env` | 锁外钩子 | 正确；`BAKON_HOOK` 防递归；不保证执行 |
| `run_editor` / `run_shell` / `shell_quote` | 编辑器启动与引用 | Unix 正确；Windows `%` 展开与 `cmd` 引用未验证（O-10） |
| `load_config` / `open_store` / `open_store_with` | 配置与打开 | 已拆分：首次 `edit` 用 `init`，其他命令用 `open`（A-06 已修复） |
| `store_path` / `editor_command` / `home_dir` | 优先级 | 基本正确；Windows 仅用 `USERPROFILE` |
| `lock_timeout` / `emit` | 超时与输出 | 正确；`emit` 的 broken pipe 见 O-09 |

### 3.12 测试

| 文件 | 规模 | 覆盖 | 结论 |
|---|---:|---|---|
| `crates/bakon-core/src/*` 单元测试 | 57 | 类型、清单、对象、存储、锁、恢复 | 主路径与部分边界好；缺属性模型 |
| `crates/bakon/tests/cli.rs` | 36 | 端到端命令、格式、退出码 | 覆盖广；依赖 `sh` 与可执行脚本，Windows 不可用 |
| `crates/bakon/tests/regressions.rs` | 13 | F1 至 F13 | 回归有效；部分 `#[cfg(unix)]` |
| `crates/bakon/tests/crash.rs` | 6 | init/edit/revert/prune 崩溃点 | 未覆盖“恢复过程自身崩溃”与多 pending |
| `crates/bakon/tests/concurrency.rs` | 1 | 双编辑器冲突 | 未覆盖 prune/mv/verify 与编辑器+恢复 |
| `cargo test --workspace --all-targets` | 120 | — | 基线通过 |

设计 §8 承诺“生成任意操作序列并断言 S1 至 S8”的属性测试，当前只有 `manifest.rs` 的编号/裁剪生成序列，没有跨模块模型测试。这是覆盖缺口，不是已证明的缺陷。

## 4. 外部评审逐条判定

| 编号 | 评审意见 | 判定 | 依据与修正 |
|---|---|---|---|
| R1 | `StoreLock` 没有区分读写 | **成立** | `StoreLock` 只有 `file`；`commit`/`rename`/`set_hook`/`begin_apply`/`finish_apply` 均不接收 guard；见 A-09 |
| R2 | `commit` 的 `_ =>` 兜底信息含糊 | **成立（低）** | 两个方向都落到“changed while tracking”；`(Some,None)` 更应是清单缺失/损坏；消息不承诺，但可改进 |
| R3 | `resolve` 是 $O(n)$、接收 `&mut self`、不持久化索引 | **部分成立** | $O(n)$ 是缓存 miss 的文档化兜底；`&mut` 只用于内存缓存；不持久化是 V1-8 的有意修复，不是回归。没有正确性缺陷，只有 API 形状与性能提示 |
| R4 | `prune` 的 `expect("blob was counted")` 应改为错误 | **成立且更严重** | 重复 `FileId` 会先在下标处算术下溢；见 A-05。应同时去重、检查计数并返回错误 |
| R5 | `recover_entry` 多处借用 `manifest.path`，可读性差 | **不成立（非缺陷）** | 借用能编译且生命周期清晰；克隆路径只会增加分配。可作为可读性重构，不进入缺陷清单 |
| R6 | 部分 `pub` 方法缺少“调用方须持锁”文档 | **成立** | `commit`、`rename`、`set_hook`、`begin_apply`、`finish_apply` 都缺；见 A-09 |
| R7 | `write_holder` 每次截断重写，可能被并发读到中间态 | **成立（低）** | 仅诊断用；另外设计承诺的开始时间未写入，见 A-16 |
| R8 | `is_init_temp` 硬编码 `BAKON_STORE.` 前缀 | **成立且更严重（已修复）** | 不只是常量重复：任何 `BAKON_STORE.*.tmp` 普通文件都会被删除，见 A-01；现在前缀由 `MARKER_FILE` 派生，严格数字匹配且不删除 |
| R9 | 测试的 `path()` 使用 `/tmp`，Windows 会失败 | **部分成立** | 单元测试的 `/tmp` 是路径断言，Windows 上字符串断言确实会失败；实际文件 I/O 已多用 `TempDir`。更大的 Windows 障碍是 `.sh` 编辑器与 `sh -c`，见 A-10 |
| R10 | `Committed::trim_failed` 只有布尔，诊断信息有限 | **成立（低）** | 见 A-19；建议携带失败对象/错误或日志 |

评审的正面判断（清单先行、对象后回收、salvage、错误分层、锁语义、测试质量）与本次代码阅读一致；本轮没有发现这些主路径的反例。

## 5. 反例与实验

以下实验使用 `target/debug/bakon` 与本机 Linux/amd64，隔离存储与临时目录。

| 编号 | 操作 | 观测 | 对应 |
|---|---|---|---|
| R-01 | 创建只含 `BAKON_STORE.notes.tmp`（内容为用户数据）的目录，执行 `init` | 修复后退出 6，文件保留；修复前退出 0 且文件被删除 | A-01 |
| R-02 | 初始化空存储，写入指向不存在 `FileId` 的合法 `.pending`，执行 `verify` | 退出 6，报告 `pending_apply` | A-02 |
| R-03 | 对 R-02 执行 `verify --fix` | 修复后退出 6，`.pending` 保留；修复前退出 0 并删除 | A-02 |
| R-04 | `--store <不存在路径> verify` | 修复后退出 6，路径不存在；修复前退出 0 并创建标记、布局与 `.lock` | A-06 |
| R-05 | 目录中有普通文件 `blocker` 与 `victim`，执行 `edit <dir>/blocker/../victim` | 内核读取该路径返回 ENOTDIR；`bakon` 退出 0 并把 `<dir>/victim` 纳管 | A-07 |
| R-06 | 纳管 0600 文件，检查对象权限 | 对象与清单为 0664，目录为 0775（受 `umask` 影响） | A-08 |
| R-07 | 将存储设为只读，执行 `log` | 退出 8，`I/O error: Permission denied` | A-13 |
| R-08 | 把单版本清单的 `ver` 改为 `4294967295`，执行 `verify`，再执行 `edit` | `verify` 返回 0；`edit` 返回 70 “version number overflow” | A-14 |
| R-09 | 破坏一个清单后执行 `verify` 或 `verify --format json` | 只打印 `corrupt manifest`，退出 6；没有报告其余项 | A-11 |
| R-10 | `--format json hook set <file> true`、`--format json config init` | 输出人类文本而非 JSON | A-12 |
| R-11 | 跟踪 v0=one、v1=two、v2=three，执行 `diff <file> 1` | 比较 v0 与 v1，而帮助把第一个参数描述为 older | A-17 |
| R-12 | 公共 API 以同一 `FileId` 两次调用 `prune([id,id])` | 修复后去重并正常完成；修复前 debug 下溢 panic | A-05 |
| R-13 | 公共 API 对同一路径两次 `prepare_track`/`commit` | 修复后第二次 `AlreadyManaged`，只有一份清单；修复前两份 | A-04 |
| R-14 | `.pending` 版本大于清单头，执行 `verify --fix` | 退出 0，报告 `pending_stale [fixed]`，`.pending` 被删除 | A-02 |

## 6. 新发现详述

### 6.1 P1

**A-01 `init` 删除用户文件。** `is_init_temp` 只检查文件名以 `BAKON_STORE.` 开头、以 `.tmp` 结尾且是普通文件；`is_empty_dir` 把这样的目录视为空，`initialize` 与后续 `init` 都会调用 `remove_init_temps` 删除它（`crates/bakon-core/src/store/mod.rs:1097-1150`）。形式陈述：存在状态 $S$，其中目录 $d$ 只含普通文件 $f$，$\mathrm{name}(f)=\texttt{BAKON\_STORE.notes.tmp}$；`init(d)` 从 $S$ 到达 $S'$，且 $f\notin T(S')$。这违反 $\mathbf{P_2}$ 的“不得删除不属于本程序的普通文件”，也违反项目对“非空无标记目录”的保守原则。已修复：临时文件模式收紧为 `^BAKON_STORE\.[0-9]+\.[0-9]+\.[0-9]+\.tmp$`，并且匹配项只被忽略、不再删除；非数字形状的同前缀文件按用户数据拒绝初始化。上述 R-01 现在退出 6 且保留文件。

**A-02 恢复假成功并删除 `.pending`。** `recover` 对所有条目调用 `recover_entry`；后者在 `load_manifest_optional(entry.id)` 返回 `None` 时直接 `Ok(())`，在清单中没有 `pending_apply` 时也直接 `Ok(())`；随后 `recover` 无条件 `clear_pending()`（`crates/bakon-core/src/store/mod.rs:698-842, 912`；`store/verify.rs:55-67`）。因此 `verify --fix` 会在没有完成任何写回、也没有证明 no-op 的情况下删除唯一的恢复记录并返回 0。第二个触发路径是恢复自身崩溃：`append_revert_and_apply` 先 `write_pending(new_version)`，再 `commit`；若在两者之间崩溃，下一次恢复看到清单中没有该版本，按同样逻辑清除 `.pending`。形式陈述：存在 $S$ 使 $P(S)\neq\emptyset$、$\mathrm{pending}(M(S))=\emptyset$，而 $R(S)$ 删除 $P$ 且不改变 $D$；这与 $\mathbf{P_4}$ 的“无法完成必须保留”冲突。已修复：`recover_entry` 现在校验 `entry.version`、`pending_apply` 与 `entry.new_blob == record.blob`；清单缺失、版本被裁剪、blob 不匹配或目标偏离时保留 `.pending` 并退出 6。只有“版本已记录且目标等于新内容”或“清单头小于记录版本”才丢弃 `.pending`，后者由 `verify --fix` 报告为 `pending_stale`；多条记录视为损坏。上述 R-03 现在退出 6 且保留 `.pending`。

### 6.2 P2

**A-03 写回没有 pending 的分支。** `Store::committed(id, ver)` 对任意已存在版本返回 `Committed`，`Committed::apply` 直接调用 `write_target`，不经过 `begin_apply`（`crates/bakon-core/src/store/mod.rs:127-138, 261-274`）。CLI 在 `revert` 的“目标内容等于 head”分支使用它（`crates/bakon-cli/src/commands.rs:171-176`）：若目标已是普通文件，`write_existing` 先 `O_TRUNC` 再写；磁盘满或 I/O 错误发生在截断之后时，目标留下部分内容，而 `.pending` 不存在，恢复流程没有依据。形式陈述：存在转移 $S\to S'$，其中 $D$ 被部分写入、$P(S')=\emptyset$，且 $M$ 的 pending 为空；这违反 $\mathbf{P_4}$。已修复：目标已存在时直接 no-op，不再截断重写；只有目标缺失时才创建。`Store::committed` 仍公开，旧版本写回面留给 A-09 收紧。

**A-04 公共提交不检查路径唯一。** `prepare_track` 生成新 `FileId`，`commit` 只比较 head 并写清单，不检查 $M$ 中是否已有相同 `path.key()`（`crates/bakon-core/src/store/mod.rs:281-296, 323-387`）。CLI 通过 `resolve` 规避，但核心库公开 API 可以直接产生两份同路径清单，违反 $\mathbf{S_1}$/$\mathbf{P_9}$。后果是 `resolve` 返回其中一份、`ls` 静默折叠、后续 `edit` 可能更新非预期历史；`verify` 只能报告 `duplicate_path`，不能自动修复。已修复：`commit` 的 track 分支（`expected_head == None`）先调用 `resolve`，路径已被任何清单占用时返回 `AlreadyManaged`（4），不再写入第二份清单。

**A-05 `prune` 重复 id 下溢。** `prune` 用 `BTreeSet` 去重选择，但随后 `for id in ids` 遍历原始切片（`crates/bakon-core/src/store/mod.rs:534-570`，下溢在 `:556-557`）。重复 id 的第二次迭代会对同一批 removed 记录再次执行 `*count -= 1`；当计数已为 0 时 debug 下溢 panic，release 下回绕为极大值，报告与候选集合均不可信。公共 API 的签名 `&[FileId]` 不排除重复。已修复：入口先把 `ids` 转成 `BTreeSet` 再遍历，计数使用 `checked_sub`，缺失计数与下溢返回 `Internal`，不再 panic 或回绕。

**A-06 `Store::open` 自动创建存储。** `open` 直接调用 `init`；`init` 在路径不存在时创建目录、标记与布局（`crates/bakon-core/src/store/mod.rs:143-187`；CLI 在 `crates/bakon-cli/src/commands.rs:971-979` 调用）。`verify`、`log`、`ls` 等读命令都先 `open`，因此 `bakon --store /typo verify` 会创建一个空存储并返回 0；`log` 随后会返回 3（未纳管），但创建副作用已经发生。API §3 把 `Store::open` 的失败定义为 6（未初始化），而 OPS §1 又说“不存在则创建”；两条契约冲突。已修复：`Store::open` 只接受已有标记，缺失、空目录或无标记都返回 6 且不创建；`init` 命令与首次 `edit` 使用 `Store::init` 的创建语义，其他读/写命令使用 `open`。上述 R-04 现在退出 6 且路径不存在。

**A-07 `..` 的词法折叠。** `reject_symlinked_parent_traversal` 只在 `..` 前的组件是符号链接或规范路径不一致时拒绝（`crates/bakon-core/src/types.rs:97-144`）；如果前缀是普通文件，`fs::canonicalize` 等于自身，检查通过，随后 `clean` 弹出该文件组件。于是 `d/blocker/../victim` 被当作 `d/victim`，而内核会因 `blocker` 不是目录返回 ENOTDIR。缺失前缀同理。修复：对用户输入直接拒绝任何 `..`（最简单且与文档“规范化绝对路径”一致），或用 `openat`/`realpath` 保留内核语义。仅靠符号链接检查不足以建立路径身份。

**A-08 存储权限扩大可读范围。** `write_atomic` 与 `create_dir_all` 使用默认模式（`crates/bakon-core/src/fsutil.rs:33-51`；对象写入在 `object/mod.rs:38-51`）；在常见 `umask 022` 下，对象为 0644、目录为 0755，在 `umask 002` 下为 0664/0775。目标文件若是 0600，其明文内容被压缩后仍以更宽权限存放。设计明确不做加密，但“不做加密”不等于“默认让同组/其他用户可读”。修复：存储根 0700、`files/`/`objects/`/`trash/`/临时文件 0600，或在创建时显式设置；文档说明本地机密性边界。

**A-09 锁与写回没有类型承载。** `StoreLock` 不携带 `exclusive`（`crates/bakon-core/src/store/mod.rs:907-970`），`commit`/`rename`/`set_hook`/`begin_apply`/`finish_apply` 不接收 guard，`Store::committed` 还能为旧版本构造写回句柄。因此 $\mathbf{S_{10}}$ 与 $\mathbf{S_6}$ 的写回部分仍是调用点约定。CLI 的主路径遵守锁，但库调用者可以编译出“共享锁下提交”或“未持锁写回”。修复：`ReadLock`/`WriteLock` 两个类型；写方法接收 `&WriteLock`；`Committed` 只在持有写锁的事务中构造；用 `trybuild` 添加未持锁调用失败的编译用例。

**A-10 测试的跨平台门。** 单元测试在 `manifest.rs:264`、`types.rs:351-388`、`store/mod.rs:1118-1120` 中使用 `/tmp/...` 字面量；Windows 上 `AbsPath::new` 会把它变成带盘符的路径，字符串断言（如 `"/tmp/file"`）失败。集成测试大量写入 `.sh` 编辑器并用 `sh -c` 执行；`cli.rs`、`crash.rs`、`concurrency.rs` 的多数用例没有 `#[cfg(unix)]`。因此 `cargo test` 在 Windows 上不可作为门禁，与“跨平台项目”的声明不匹配。修复：断言用 `TempDir` 派生路径；脚本编辑器按平台生成（Windows 用 `.cmd`/`cmd /C`）或把 Unix-only 用例显式门控；在 Windows/macOS runner 跑全量测试，而不只是 smoke。

**A-11 `verify` 的整体失败。** `verify` 先把损坏清单收集为报告项，随后 `orphan_hashes()` 调用 `referenced_blobs()`（`crates/bakon-core/src/store/verify.rs:63-74, 216`；`store/mod.rs:593-601`），后者对每个 id 调用 `manifest(id)?`；任一份损坏清单返回 6，整个 `verify` 以错误结束，已收集的报告项与 `--size` 结果全部丢失。修复：引用集只由成功解析的清单构造，损坏清单单独报告并标记不可修复；或者返回“报告 + 错误”而不是丢弃报告。

**A-12 CLI 输出与全局参数。** `hook set`/`hook unset`/`config init` 没有 `cli.format` 分支（`crates/bakon-cli/src/commands.rs:321-341, 350-358`），`--format json` 下仍输出人类文本；API §5 把 `--no-hook` 与 `--no-color` 列为全局参数，但 `Cli` 只有 `--store`、`--config`、`--format`、`--lock-timeout`，全局位置传入这两个参数会退出 2。`--no-hook` 只在 `edit`/`revert` 子命令上存在。修复：状态类命令补 JSON/plain 输出；把 `no_hook` 提升为全局并与子命令合并，或修正文档；`--no-color` 当前无颜色输出，可删除或实现。

### 6.3 P3

**A-13 只读存储不可读。** `acquire` 以 `read(true).write(true)` 打开 `.lock`（`crates/bakon-core/src/store/mod.rs:907-915`），共享锁也需要写权限；只读挂载或 0444 的 `.lock` 使 `log` 返回 8。共享锁只需要读 fd 即可使用 `flock(LOCK_SH)`；修复为共享路径以只读打开，且不在共享路径写 holder。需要验证 Windows 的 `try_lock_shared` 对只读句柄的行为。

**A-14 版本上界。** `Manifest::validate` 只要求严格连续，不要求 `head < u32::MAX`（`crates/bakon-core/src/manifest.rs:95-146`；`types.rs:213-218`）；单版本 `u32::MAX` 被 `verify` 接受，下一次 `Ver::next` 返回 `Internal` 70。修复：把 `head == u32::MAX` 视为 `Corrupt`（不可再追加），或在 `Ver::next` 溢出时返回明确的“版本号耗尽”错误并让 `verify` 报告。

**A-15 临时文件不收敛。** `write_atomic` 的临时文件位于目标目录：清单在 `files/`，索引与 `.pending` 在根目录（`crates/bakon-core/src/fsutil.rs:33-62`；`store/mod.rs:419-424, 669-676, 861-869`；清理仅见 `verify.rs:231-251`）。崩溃或删除失败会留下 `files/<id>.json.<pid>...tmp`、`index.json.*.tmp`、`.pending.*.tmp`；`verify` 只清理 `objects/tmp` 与 `trash`，`init` 为避免误删用户文件，已不再删除标记临时文件。修复：统一使用专用 `tmp/` 目录，或在 `verify`/`init` 中按严格模式清理这些前缀并报告。

**A-16 锁文件元数据。** 设计说 `.lock` 记录 PID 与开始时间（`design/SEQUENCE__write-protocol-and-failure-paths.md:18`），代码只写 PID（`crates/bakon-core/src/store/mod.rs:947-957`）；`set_len(0)` 后 `writeln` 非原子，超时读者可能看到空/半截内容。仅诊断用，不影响锁语义。修复：写入 `pid start_unix` 并让 `read_holder` 解析两者；或修正文档。

**A-17 `diff` 参数语义。** `Cli::Diff` 的 `first` 帮助是 “Older version”，`second` 是 “Newer version”（`crates/bakon-cli/src/cli.rs:111-119`）；但单参数路径把唯一的参数当作 **newer**，与它的前一版比较（`crates/bakon-cli/src/commands.rs:691-707`）。因此 `diff file 1` 比较 v0 与 v1，而帮助会让用户以为比较 v1 与最新。修复：把字段改名为 `version`/`base`/`target`，或调整帮助并加测试。

**A-18 codec 分派绕过。** `Store::content_for` 按 `VersionRecord.codec` 分派，但 `revert`（目标内容）、`show`、`diff` 直接调用 `Store::content(blob)`（`crates/bakon-cli/src/commands.rs:136, 673, 711-712`；`store/mod.rs:250-259`）。当前只有 `Deflate`，所以没有运行时错误；一旦增加第二种 codec，这三条路径会解码错误。修复：CLI 全部改用 `content_for`；或删除 `codec` 扩展承诺。

**A-19 `trim_failed` 诊断。** `reclaim_removed` 返回 `Result<usize>`（`crates/bakon-core/src/store/mod.rs:604-619`），`commit` 把任意错误折叠为 `bool`（`:377-386`）；用户只知道“旧对象未回收”，不知道是哪个哈希、移动失败还是清空 `trash` 失败。修复：`Committed` 携带 `Option<TrimFailure { hash: Option<BlobHash>, source: Error }>`，CLI 输出具体信息。

**A-20 `fsync` 失败后的提交语义。** `write_atomic` 在 `rename` 成功之后才对父目录 `fsync`（`crates/bakon-core/src/fsutil.rs:49-58`）；若 `fsync` 失败，函数返回 `Io` 8，但新清单在运行中的系统里已经可见。`commit` 因此报告失败，用户重试时 `expected_head` 已推进，得到 `HeadChanged` 4。状态机的不变量没有被破坏（清单仍是旧的或新的），但“命令失败”与“提交已发生”不一致。修复不是简单忽略 `fsync` 错误（那会削弱持久性承诺），而是明确提交结果的三态：已持久、已可见但未证明持久、未提交；至少在文档与错误消息中说明重试前应读取 head，并增加故障注入测试。该问题已于 2026-11-11 由 [[ADR__exit-9-when-durability-uncertain]] 裁决：`rename` 为提交点，父目录 `fsync` 失败返回退出码 9；实现与测试见该 ADR 与 `fsutil`。

### 6.4 次要观察

| 编号 | 观察 | 严重度 |
|---|---|---|
| O-01 | `Manifest::validate` 不要求时间 UTC/递增，不限制 `pending_apply` 只出现在 `Revert` 或头版本 | P3 |
| O-02 | `edit` 在编辑器期间路径被 `mv`/重纳管时返回 `Internal` 70，而不是用户冲突 4 | P3 |
| O-03 | 原地写回保留 inode，因此硬链接的其他路径会被连带修改；设计未声明 | P3 |
| O-04 | `ObjectStore::put` 发现对象路径已存在即返回成功，不校验既有对象是否完整 | P3 |
| O-05 | `list_hashes` 忽略非法分片/文件名；损坏对象文件不会被 `verify` 报告或清理 | P3 |
| O-06 | `begin_apply` 不校验版本/头；`finish_apply` 只清头，可能清错版本 | P3 |
| O-07 | 恢复时非普通文件返回 3 而非 7；`finish_apply` 的退出码已按 8/9 修正 | P3 |
| O-08 | `config init` 非原子、无 `fsync`；未知 TOML 字段被静默忽略；提供 `--store` 时仍要求配置文件可解析 | P3 |
| O-09 | `emit`/`show`/`diff` 把 broken pipe 映射为 8 并打印错误，`bakon show ...` 管道到 `head` 不符合常见预期 | P3 |
| O-10 | Windows `shell_quote` 只双写引号；`%VAR%` 在 `cmd` 引号内仍会展开；未在 runner 验证 | P3 |
| O-11 | 文档/API 残余：`Store::versions` 未实现；架构图仍有 `version/`、`fs2` | P3 |
| O-12 | `Store::init`/`open` 的布局修复与临时清理不持锁；并发 init 会竞争，但最终标记合法 | P3 |
| O-13 | `rename` 删除内存旧索引键后，若 `remember` 因新键已映射而提前返回，磁盘旧键可能保留；`resolve` 仍能纠正 | P3 |

## 7. 交叉论证

### 7.1 恢复是闭包，不是尽力而为

把恢复写成关系 $R\subseteq S\times S$。$\mathbf{P_4}$ 要求 $R$ 在全定义处满足 $R(R(S))=R(S)$，且对不可判定输入保持 $P$ 不变。A-02 的反例是 $R$ 在 $M$ 缺失时把 $P$ 映射为空：$R(S)=S'$ 且 $P(S')=\emptyset$，但 $R(S')=S'$ 只是“稳定”，不是“完成”。这与 0.x 审计里的 V1-1 同型：不是“回收延迟”，而是“回收不可达”。修复的判定条件应是“存在 pending 版本且哈希匹配”或“目标已等于新值”，二者都不可满足时必须保留记录。

### 7.2 类型承载与运行时锁

类型论视角下，若写接口签名是 `fn commit(&mut self, ...)`，则“持有写锁”不在类型中；若改成 `fn commit(&mut self, guard: &WriteLock, ...)`，则共享锁下的调用不可编译。A-09 说明当前实现仍把 $\mathbf{S_{10}}$ 放在调用点。反过来，`Committed::apply` 消费 `self` 只证明“版本已存在”，不证明“本次写回有 pending”，因此 $\mathbf{S_6}$ 只承载了一半。修复不是加注释，而是让“可写回”成为由 `begin_apply` 产生的线性令牌。

### 7.3 序理论与裁剪

版本号的严格递增与裁剪后缀在 `Manifest` 层成立；A-05 的下溢不是序错误，而是“选择集合”与“迭代集合”不一致：`selected` 去了重，`ids` 没有。A-14 是序的边界：$u32::MAX$ 没有后继，`checked_add` 把“数据已到边界”映射成“内部 bug”。序理论视角下，应在状态入口把不可表示的后继声明为损坏，而不是在转移时 panic/内部错误。

### 7.4 路径身份与信息流

A-07 说明 `AbsPath` 的等价关系不是内核等价关系：词法折叠把“ENOTDIR”变成了另一个可写路径。A-08 说明另一个方向：即使路径身份正确，对象层的权限仍可能把 0600 的内容暴露给同组用户。前者是完整性/正确性，后者是机密性；二者不能用同一个 `clean` 或 `NotAStore` 解决。

### 7.5 输出契约

`--format json` 是脚本接口。A-12 的反例说明“数据命令”和“状态命令”被区别对待，但文档没有这种区分。输出全域应按命令建立矩阵：每个命令在每个格式下要么有定义，要么显式声明不支持；`hook set`/`config init` 目前落在矩阵之外。

## 8. 落地指导与优先级

| 优先级 | 发现 | 修改 | 验证 |
|---|---|---|---|
| 1 | A-01 | 收紧临时文件模式；只删除本次初始化已知文件；优先专用 `tmp/` | 只含 `BAKON_STORE.notes.tmp` 的目录被拒绝或保留；崩溃前缀仍可恢复 |
| 1 | A-02 | 恢复前校验 `entry.version`/`new_blob`；无法判定保留 `.pending` 并报告；恢复崩溃测试 | 缺失清单、未提交版本、恢复中崩溃三种用例均不返回 0 且保留证据 |
| 2 | A-03 | 已匹配分支 no-op；实际写回先 `begin_apply`；收紧 `Store::committed` | 只读/磁盘满/崩溃注入下无“部分目标 + 无 pending” |
| 2 | A-04 | track 提交检查路径键唯一；写锁入参 | 库调用两次 track 同路径，第二次返回 `AlreadyManaged`/冲突 |
| 2 | A-05 | `ids` 去重；`checked_sub`；`expect` 改错误 | 重复 id 的 `prune` 不再 panic，报告唯一 |
| 2 | A-06 | `open` 要求标记；`init` 显式；读命令不创建 | `--store <typo> verify` 返回 6 且不产生文件 |
| 2 | A-07 | 拒绝任何 `..` 或使用内核语义 | `blocker/../victim` 返回用法错误；符号链接用例仍拒绝 |
| 2 | A-08 | 存储根 0700、对象/清单/临时 0600 | 0600 目标的对象仍为 0600；`umask` 不扩大 |
| 2 | A-09 | `ReadLock`/`WriteLock`；写接口收 guard；`trybuild` | 未持锁提交/共享锁提交不可编译 |
| 2 | A-10 | 测试路径用 `TempDir`；脚本按平台门控 | Windows/macOS runner 全量 `cargo test` |
| 2 | A-11 | 引用集只用有效清单；损坏清单单独报告 | 一份损坏 + 一份缺对象，`verify` 同时报告两项 |
| 2 | A-12 | 状态命令补 JSON；全局参数或文档 | 每个命令 `--format json` 输出可解析；`--no-hook` 全局位置可用 |
| 2 | A-20 | 定义 `rename` 后 `fsync` 失败的提交语义；故障注入 | 失败后重跑不产生重复版本，且用户能判断是否已提交 |
| 3 | A-13 至 A-19 | 见第 6 节 | 对应单元/端到端用例 |
| 3 | O-01 至 O-13 | 见第 6.4 节 | 文档同步或小修 |

## 9. 独立校验

### 9.1 已执行

- `cargo test --workspace --all-targets`：120 个测试通过（core 57、regressions 13、cli 集成 36、cli 单元 7、crash 6、concurrency 1）。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo fmt --all --check`：通过。
- R-01 至 R-11 与 R-14 使用真实二进制与隔离存储复现；R-12/R-13 已转为 `store` 单元回归用例。
- 0600 文件的对象权限、只读存储、损坏清单、`u32::MAX` 清单、`.pending` 缺失清单均按第 5 节观测。
- A-20 的退出码 9 实现后测试总数为 123；A-01 至 A-06 修复后为 136；本节的 120 是审计时基线。

### 9.2 未覆盖

- Windows/macOS 的真实路径大小写、Unicode 归一化、`cmd` 引用与只读锁行为只做静态推断；A-10 的 Windows 测试失败也是静态结论。
- 磁盘满导致 `write_existing` 在 `O_TRUNC` 后失败的运行时反例未构造；A-03 是代码路径结论。
- 恢复过程自身崩溃的反例由代码路径推出，未做完整崩溃矩阵；A-02 的第二触发路径仍是 `[需验证]`。
- 大规模 `prune`（$10^5$ 文件/版本）未测量；V1-11 的成本结论沿用代码路径。
- 目录 `fsync` 失败后的“失败但已提交”语义未做故障注入；A-20 是设计残余。
- 权限修复后的 Windows ACL 语义未验证。

### 9.3 最弱前提

- 单机本地文件系统；`rename` 原子、目录 `fsync` 有效；单写者遵守锁；存储目录不被外部主动篡改。
- `.pending` 与清单被视为可信输入；本审计不防御能同时改写清单与对象的主动攻击者。
- 机密性结论只讨论本地文件权限，不承诺加密或抗 root/同用户读取。

## 10. 结论

v1 的核心设计仍然成立：对象先行、清单原子替换、清单先行回收、salvage 保留外部编辑、退出码分层，这些主路径在本轮逐函数阅读中没有发现新的反例。外部评审的 10 项意见中，7 项成立，2 项部分成立，1 项只是可读性；其中 R4 与 R8 的实际后果比评审描述的更严重。

真正需要优先处理的是两个 P1：A-01 的“初始化删除用户文件”和 A-02 的“恢复假成功并删除 pending”。它们都违反了项目最核心的保守原则：不确定时保留，而不是猜测后继续。其余 P2 集中在库 API 的边界（A-03 至 A-09）、验证与测试门（A-10 至 A-12）以及 `fsync` 失败语义（A-20）。这些不改变正常 CLI 主路径的结论，但决定了“作为库使用”和“跨平台发布”是否配得上当前文档的承诺。

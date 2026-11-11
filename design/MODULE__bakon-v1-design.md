# MODULE__bakon-v1-design

本文件是把审计结论转成可执行工程的详细设计：目标、模块、数据格式、算法、并发、错误模型、测试与实施顺序。形式依据见 [[STATE__store-state-machine-and-invariants]]；具体契约见 [[API__store-contract-and-cli]]。语言为 Rust，依据 [[ADR__rewrite-in-rust]]；存储自建，依据 [[ADR__replace-git-with-own-content-addressed-store]]。

## 1. 目标与非目标

目标：

- 单文件二进制，零运行时依赖（不调用任何外部命令）。
- 单机、单人、无协作的文件版本历史，语义与 0.x 相同：纳管、编辑提交、回退追加、裁剪、钩子。
- 13 项审计缺陷中，11 项在结构上不可表达或由不变量排除；2 项（F5、F9）由显式策略解决。
- 崩溃后任意状态可被 `verify` 判定为「一致」或「可修复」。

非目标：多人协作、跨机同步、加密与签名、目录级管理、自动合并、增量备份、旧存储自动迁移。

## 2. 文档地图

| 主题 | 文档 |
|---|---|
| 为什么不依赖 git | [[ADR__replace-git-with-own-content-addressed-store]] |
| 为什么用 Rust | [[ADR__rewrite-in-rust]] |
| `rename` 提交点与退出码 9 | [[ADR__exit-9-when-durability-uncertain]] |
| 状态机、不变量、崩溃点 | [[STATE__store-state-machine-and-invariants]] |
| 对象层与持久化 | [[DATA__blob-store-and-durability]] |
| 清单、编号、保留 | [[DATA__version-manifest-and-numbering]] |
| 命令时序与失败路径 | [[SEQUENCE__write-protocol-and-failure-paths]] |
| 归属、恢复、备份、迁移 | [[OPS__store-identity-recovery-and-retention]] |
| 接口、CLI、退出码 | [[API__store-contract-and-cli]] |
| 重构前的缺陷清单 | [[design-audit-2026-11-10]] |
| v1 设计与实现的再次形式化审计 | [[design-audit-v1-2026-11-10]] |
| v1 修复后的复核 | [[design-audit-v1-2026-11-10-recheck]] |
| v1 全量逐模块审计（2026-11-11） | [[design-audit-v1-2026-11-11]] |

## 3. 架构

```text
crates/
├── bakon-core/        库：无终端、无 CLI 依赖
│   ├── store/         打开、标记、锁、恢复
│   ├── object/        对象读写、压缩、校验
│   ├── manifest/      清单结构、原子替换、缓存
│   ├── version/       编号、裁剪、head
│   ├── apply/         目标文件写回策略
│   └── verify/        一致性检查与修复
├── bakon-cli/         命令解析、输出格式、退出码映射
└── bakon/             可执行入口（main）
```

依赖方向：`bakon` 依赖 `bakon-cli` 与 `bakon-core`；`bakon-cli` 依赖 `bakon-core`；`bakon-core` 不依赖任何上层，也不依赖终端类型。核心层可直接作为库使用，测试全部在无 TTY 环境下运行。

依赖清单（全部纯 Rust）：`clap`（derive）、`serde`/`serde_json`、`toml`（配置）、`sha2`、`flate2`（Rust 后端）、标准库文件锁（`File::lock`，Rust 1.89+）、`rand`、`time`、`similar`（diff）。

## 4. 数据布局

```text
store/
├── BAKON_STORE              标记：schema、uuid、created
├── .lock                    排他/共享锁
├── .pending                 待写回标记（存在时才有）
├── index.json               派生缓存：路径 → id
├── files/<id>.json          权威清单
├── objects/aa/<rest>        内容寻址对象（deflate）
├── objects/tmp/             写入暂存
└── trash/                   待删除对象
```

路径默认 `~/.bakon/store`；优先级为 `--store` > `BAKON_STORE` > 配置 `store` > `BAKON_HOME/store` > `~/.bakon/store`，见 [[OPS__store-identity-recovery-and-retention]] §1。0.x 的 `repo/` 不被读取也不被修改。

Unix 上新建的存储根与布局目录为 `0700`，清单、对象、索引、`.pending`、`.lock` 与原子写临时文件为 `0600`；`open` 不修改已存在目录的权限。存储不加密，机密性边界依赖这些权限与宿主机的访问控制。

格式定义与字段语义见 [[DATA__version-manifest-and-numbering]]；写入顺序见 [[DATA__blob-store-and-durability]]。

## 5. 关键算法

### 5.1 纳管

1. 规范化路径，查缓存；未命中则扫描清单，仍未命中则新建。
2. 生成 `FileId`（16 字节随机），检查未与现有 id 冲突；提交时再次检查路径键未被任何清单占用。
3. 读取目标内容，`put_object` 得 `BlobHash`。
4. 构造清单：`versions = [v0: baseline]`，`retention = Inherit`，`hook = 空`。
5. 原子写入 `files/<id>.json`，更新缓存。

### 5.2 追加版本

1. 持锁读清单，记 `head`。
2. `head` 与调用方记录的期望值不同则返回 `HeadChanged`（退出 4）。
3. `put_object` 新内容。
4. 构造新清单：`V' = V ++ [(next, blob, size, now, source, pending?)]`。
5. 原子替换清单。
6. 若生效上限为 $m > 0$ 且 $|V'| > m$，进入裁剪。裁剪提交清单后，在同一写锁内回收不再被任何清单引用的对象；回收失败只留下孤儿对象，由 `verify --fix` 或下一次 `prune` 收敛。

### 5.3 裁剪

1. 计算保留后缀：新清单 `V' = V[|V|-m..]`。
2. 被移除条目引用的对象集合减去仍被引用的集合，得候选删除集。
3. 候选 `rename` 到 `trash/`。
4. 原子替换清单。
5. 清空 `trash/`。

步骤 3 可以先于 4 执行，因为被移入 `trash/` 的对象在清单替换前仍通过旧清单可达，替换后即不可达；任何时刻读取都不会命中「清单在、对象已被删」的状态。

### 5.4 回退应用

1. 分歧检查：目标存在且内容不等于 head 时，无 `--force` 则退出 4。
2. `--force` 时先追加 `salvage` 版本（记录当前磁盘内容），使覆盖可回退。
3. 追加 `revert` 版本，`pending = true`。
4. 按目标类型写回（见 [[SEQUENCE__write-protocol-and-failure-paths]] 第 4 节）。
5. `pending = false`。

### 5.5 diff

两个 blob 解码为字节序列；含 `\0` 或非法 UTF-8 时输出「二进制文件不同」并返回 1。文本走 `similar` 的 Myers 实现，输出统一 diff。库的选择理由：自己实现 hunk 合并与上下文行需要可观的测试语料，而该库是纯 Rust、无外部依赖。

### 5.6 verify

检查项与退出码：

| 检查 | 修复 |
|---|---|
| 标记存在、schema 受支持 | 不可自动修复 |
| 清单可解析、字段合法 | 不可自动修复 |
| `V` 非空、版本号严格递增 | 不可自动修复 |
| 每个 `blob` 存在、可解压、哈希一致 | 不可自动修复（需备份） |
| `size` 与解码长度一致 | 修正字段 |
| 索引与清单双向一致 | 重建索引 |
| 无两份清单共享同一 id | 不可自动修复，报冲突 |
| `trash/`、`objects/tmp/` 残留 | 清空 |
| `objects/` 中未被任何清单引用的对象 | 移入 `trash/` 后删除 |
| `.pending` 或 `pending_apply` | 报告；`--fix` 先执行恢复 |

损坏清单只形成报告项，不中断其余检查。只要有一份清单无法解析，引用集就不完整：`verify` 报告 `orphan_scan`（不可修复），`--fix` 跳过孤儿回收与索引重建，避免把损坏清单仍可能引用的对象当孤儿删除。

`--size` 额外输出占用明细；`--fix` 只做表中标记为可修复的项，其余只报告。

## 6. 并发与锁

| 操作 | 锁 |
|---|---|
| 提交、裁剪、改名、钩子配置、`verify --fix` | 排他 |
| `log`、`show`、`diff`、`ls`、`verify` | 共享 |
| 编辑器会话 | 无 |

锁由两个不互通的具体类型承载：`begin_write`/`begin_verify` 返回 `WriteLock`，`lock_shared` 返回 `ReadLock`；所有写接口都要求 `&WriteLock`。标准库的 `File::try_lock` / `try_lock_shared` 是尝试调用；超时用「尝试获取 + 退避」实现，默认 2 秒。锁文件内容记录 pid，仅用于错误消息。

`edit` 的冲突检测：进入编辑器前记录 `(id, head_ver, content_hash)`，提交前在锁内比较 `head_ver`；不一致返回 `HeadChanged`，保留磁盘上的编辑结果，提示重新执行。这一机制替代了 0.x「整段会话持锁」的做法，使第二个写者不会无限阻塞。

## 7. 类型级不变量

| 不变量 | 类型手段 |
|---|---|
| 身份不混用 | `FileId`、`Ver`、`AbsPath`、`BlobHash` 四个 newtype，无 `From` 互通 |
| 编号只能递增 | `Ver::next(head)` 为唯一构造器；清单反序列化时校验严格递增 |
| 破坏性写后置 | `apply` 仅定义在 `Committed` 上；`Prepared` 无法触达目标文件 |
| 写锁不可绕过 | `ReadLock`/`WriteLock` 为不互通类型，写接口要求 `&WriteLock`；字段私有，不能伪造 |
| 保留三态 | `Retention::{Inherit, Unlimited, Limit}`，JSON 映射为 缺省/0/n |
| 版本事实唯一来源 | 版本只能从 `Manifest` 读取；`Ver` 无 `FromStr` |
| 错误到退出码 | `Error::exit_code`，穷尽匹配 |

## 8. 测试策略

| 层级 | 内容 |
|---|---|
| 单元 | 编号、裁剪后缀、保留三态、路径规范化、diff 边界 |
| 属性测试 | 生成任意操作序列（纳管、提交、裁剪、回退、改名、强制覆盖），断言 $\mathbf{S_1}$ 至 $\mathbf{S_8}$ |
| 崩溃注入 | 持久化动作在 fsync/rename 之后经过崩溃点；debug 构建读取 `BAKON_TEST_CRASH_AFTER=n` 并在第 $n$ 个点终止进程。测试枚举全部点，重开后断言「可读、可 verify、恢复幂等」 |
| 并发 | 两个进程同时提交同一文件，断言恰好一个成功、另一个退出 4；编辑器会话期间其他命令不被阻塞 |
| 数据损坏 | 截断对象、改单字节、删清单、清空索引，断言退出码与提示 |
| 兼容 | 高/低 schema、损坏标记、非空非存储目录 |
| 端到端 | 用假编辑器脚本覆盖真实 CLI，断言 stdout/stderr/退出码 |

验收标准：审计的 13 项缺陷每一项都有一个失败的旧行为用例在 v1 中通过；崩溃矩阵的 6 个位置各至少一个测试。

## 9. 性能预算

| 操作 | 预算 |
|---|---|
| 提交 1 MB 文件 | 端到端 60 ms（含进程启动与编辑器；2026-11-10 本机实测约 50 ms） |
| `log` 1000 版本 | 20 ms（只读一个清单） |
| 裁剪 100 版本 | 50 ms（一次清单写 + 100 次删除） |
| `verify` 1000 文件 × 100 版本 | 5 s（以哈希为准） |
| 冷启动 | 5 ms |
| 静态二进制 | 6 MB 以内 |

预算按「单机配置文件」规模设定；不针对大文件或海量版本优化。

## 10. 实施阶段

| 阶段 | 范围 | 完成判据 |
|---|---|---|
| M0 | 工作区、crate 划分、CLI 骨架、错误类型与退出码、标记与存储打开 | `bakon version`、`bakon init` 可用；错误码测试通过 |
| M1 | 对象层、清单层、纳管、`edit`、`log`、`show`、`diff` | 单文件全流程可用；属性测试覆盖编号与裁剪 |
| M2 | `revert` 与 `apply`、`prune`、`mv`、`hook`、`config`、`verify` | 13 项缺陷的回归用例全部通过 |
| M3 | 崩溃注入、并发、Windows 语义、交叉编译、性能测量、文档同步 | 崩溃矩阵与并发用例通过；7 个目标产物可用 |

每阶段结束更新 [[project_state_now]] 与任务板，不在阶段内维护第二份状态。

## 11. 与 0.x 的行为差异

| 项 | 0.x | v1 |
|---|---|---|
| 存储位置 | `repo/`（git） | `store/`（自有格式） |
| 运行时依赖 | 需要 git | 无 |
| `diff` 退出码 1 | 差异与错误都可能 | 只有差异 |
| `revert` 遇到磁盘偏离 | 静默覆盖 | 默认退出 4，`--force` 先记 `salvage` |
| `max_versions = 0` | 回退到全局 | 不限制 |
| 钩子执行位置 | 锁内 | 锁外，`BAKON_HOOK=1` 防递归 |
| 锁等待 | 无限阻塞无提示 | 2 秒后退出 4 并给出持有者 |
| 裁剪 | 重写 git 历史 | 截断清单后缀 |
| `index.json` | 事实来源之一 | 纯缓存，可丢 |
| 退出码 | 1/2/3/4/5/6/70 | 增 7（未写回）、8（IO/权限）、9（已提交但持久性未确认），1 仅用于 diff |
| 旧历史 | — | 不自动迁移，旧目录保留 |

## 12. 风险与未决

- **交叉编译**：v1.0 需要 7 个目标；macOS 与 Windows 的产物必须在对应 runner 上构建或交叉工具链验证。风险在于工具链维护，不在代码。
- **自建格式缺少外部兜底**：没有 `fsck` 之类的第三方工具。缓解手段是 `verify` 的检查表与「不做宽松解析」的门禁。
- **Windows 原子替换**：`std::fs::rename` 在 Windows 上以 `MoveFileEx` 实现可替换目标，但目录 `fsync` 无对应语义；崩溃恢复退化为「可能丢失最后一次提交」，需在文档中声明。
- **钩子语义变化**：从锁内移到锁外，去掉了「钩子与其他写操作互斥」的保证，换来可重入与不阻塞。这是取舍，不是缺陷。
- **未决**：旧存储迁移是否值得以 feature 形式提供，取决于用户是否真的需要在同一台机器上保留旧历史。

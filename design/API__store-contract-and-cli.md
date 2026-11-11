# API__store-contract-and-cli

本报告是实现的接口契约：Rust 类型、存储接口的前置与后置条件、命令行行为与输出格式。模型依据见 [[STATE__store-state-machine-and-invariants]]；实现结构见 [[MODULE__bakon-v1-design]]。

## 1. 核心类型

```rust
pub struct AbsPath(PathBuf);      // 规范化绝对路径，构造即校验
pub struct FileId([u8; 16]);      // 随机身份，构造仅由纳管路径产生
pub struct Ver(u32);              // 单调版本号，只能由 Ver::next 产生
pub struct BlobHash([u8; 32]);    // 内容哈希
pub struct Note(String);          // 单行备注：1..=500 字符，无控制字符
```

五条约束：

- `FileId` 不能从路径构造，只能由 `FileId::generate()` 产生并通过唯一性检查写入。
- `Ver::next(head)` 是唯一的递增方式；不存在「从字符串解析版本号」的入口。
- `AbsPath` 在构造时绝对化、拒绝任何 `..`，并清理 `.`；最终目标的符号链接保留。`..` 的词法折叠与内核解析在普通文件或缺失前缀后不一致，因此必须在进入存储前由用户规范化。大小写不敏感平台用路径键比较身份。
- `Note` 只能由 `Note::parse` 从用户输入构造，或在反序列化时按同一规则校验；空串、超过 500 字符、含控制字符或首尾空白都不是合法持久值。
- 四个类型之间没有隐式转换，编译期阻止混用。

```rust
pub enum Retention { Inherit, Unlimited, Limit(NonZeroU32) }  // JSON: 字段缺省 / {"max_versions":0} / {"max_versions":n}
pub enum Source { Baseline, Edit, Revert, Salvage }           // JSON: 小写字符串
pub enum Codec { Deflate }
```

## 2. 写协议的类型表达

```rust
pub struct Prepared { manifest: Manifest, content: Vec<u8> }
pub struct Committed {
    id: FileId,
    version: Ver,
    trim_failure: Option<TrimFailure>,
}
pub struct TrimFailure { hash: Option<BlobHash>, detail: String }

impl Store {
    /// 构造纳管提交：生成身份与基线版本。
    pub fn prepare_track(&self, lock: &WriteLock, path: &AbsPath, content: &[u8]) -> Result<Prepared>;

    /// 构造追加提交：版本号与时间在这里生成。
    pub fn prepare_commit(&self, lock: &WriteLock, id: FileId, content: &[u8], source: Source) -> Result<Prepared>;

    /// 构造带备注的追加提交；备注已由 Note::parse 校验。
    pub fn prepare_commit_with_note(&self, lock: &WriteLock, id: FileId, content: &[u8], source: Source, note: Option<Note>) -> Result<Prepared>;

    /// 写入对象与清单，返回提交结果。失败时持久状态或为旧值或为新值。
    /// `expected_head` 是进入编辑器前读到的 head；不一致即冲突退出。
    pub fn commit(&mut self, lock: &WriteLock, prepared: Prepared, expected_head: Option<Ver>) -> Result<Committed>;
}

impl Committed {
    /// 只有已提交的版本才能写回目标文件（M2）；必须持有写锁。
    pub fn apply(self, lock: &WriteLock, store: &Store, target: &AbsPath) -> Result<()>;
}
```

`apply` 只存在于 `Committed` 上，因此「写回先于提交」在类型层面不可表达。`ReadLock` 与 `WriteLock` 是两个不互通的具体类型，所有写接口都要求 `&WriteLock`；只持共享锁或完全未持锁的调用无法编译，对应 $\mathbf{S_6}$ 与 $\mathbf{S_{10}}$。`WriteLock` 的字段私有，只能由 `begin_write`/`begin_verify` 产生。

## 3. 存储接口

| 方法 | 前置 | 后置 | 失败 |
|---|---|---|---|
| `Store::open(root, mode)` | 目录归属符合 [[OPS__store-identity-recovery-and-retention]] | 打开并校验，不创建；不执行 `.pending` 恢复 | 6（未初始化、schema 不符）、8（权限） |
| `resolve(&AbsPath) -> Option<FileId>` | — | 命中缓存或扫描清单；结果可缓存 | 6（清单损坏） |
| `manifest(FileId) -> Manifest` | id 存在 | — | 3（不存在）、6（损坏） |
| `head(FileId) -> Option<VersionRecord>` | — | 返回最后一条版本 | 6 |
| `content(&BlobHash) -> Vec<u8>` | 对象存在 | 解压并校验哈希 | 6（缺失、损坏） |
| `versions(FileId) -> Vec<VersionRecord>` | id 存在 | 升序；等价于 `manifest(id).versions` | 3、6 |
| `begin_write() -> WriteLock` | — | 持排他锁并恢复 `.pending`；无法证明的记录保留并退出 6 | 4、8、6 |
| `begin_verify() -> WriteLock` | — | 持排他锁但不自动恢复；供 `verify --fix` 先报告再处理 `.pending` | 4、8 |
| `lock_shared() -> ReadLock` | — | 持共享锁；不能传给任何写接口 | 4、8 |
| `commit(&WriteLock, Prepared) -> Committed` | head 未变；track 时路径未占用 | 追加版本、必要时裁剪 | 4（head 变化/路径已占用）、8、9（持久性未确认）、6 |
| `prune(&WriteLock, &[FileId]) -> PruneReport` | 持写锁 | 截断后缀、回收对象 | 8 |
| `prune_dry_run(&[FileId]) -> PruneReport` | 持共享锁 | 只报告，不改变持久状态 | 6 |
| `rename(&WriteLock, FileId, AbsPath)` | 持写锁 | 更新清单 `path` 与缓存 | 4（目标已存在） |
| `set_hook(&WriteLock, FileId, Option<String>)` | 持写锁 | 写入清单 | 8、9 |
| `set_note(&WriteLock, FileId, Ver, Option<Note>)` | 持写锁；版本存在 | 只改该版本的 `note`，必要时调整清单 schema | 3、8、9 |
| `begin_apply(&WriteLock, …, Ver)` / `finish_apply(&WriteLock, FileId, Ver)` | 持写锁；版本为当前 head 的后继 / 指定 pending 版本 | 校验 head、blob 与唯一 pending，再建立/清除 `.pending` 与 pending 标记 | 8、9、6 |
| `committed(&WriteLock, FileId, Ver) -> Committed` | 持写锁，版本存在 | 构造写回句柄 | 3、6 |
| `verify(options) -> Report` | 持共享锁 | 只读检查 | 6 |
| `verify_fix(&WriteLock, options) -> Report` | 持写锁 | 检查并修复派生数据 | 6、8、9（修复时持久性未确认） |

读取接口持共享锁（`Store::open` 的读模式），写接口持排他锁；`edit` 的编辑器阶段不持锁，提交时用 `expected_head` 比较交换。Unix 上新建的存储根与布局目录权限为 `0700`，清单、对象、索引、`.pending`、`.lock` 与原子写临时文件为 `0600`；已存在的目录不会被 `open` 改权限。

`verify` 遇到损坏清单时仍返回已收集的报告；只有无法读取目录等 I/O 错误才整体失败。存在损坏清单时引用集不完整，报告 `orphan_scan`（不可修复），`verify --fix` 不回收孤儿对象也不重建索引，避免删除仍可能被损坏清单引用的对象。契约细节见 [[SEQUENCE__write-protocol-and-failure-paths]]。

## 4. 错误模型

```rust
pub enum Error {
    NotManaged(AbsPath),
    NotFound(AbsPath),
    NotAFile { path: AbsPath },
    NoSuchVersion { ver: Ver, latest: Ver },
    NoPreviousVersion { ver: Ver },
    Pruned { ver: Ver, oldest: Ver },
    LockTimeout { holder: Option<u32>, waited: Duration },
    Diverged { path: AbsPath, hint: &'static str },
    HeadChanged { expected: Ver, actual: Ver },
    PathChanged(AbsPath),
    AlreadyManaged { path: AbsPath },
    Corrupt { what: &'static str, detail: String },
    ApplyFailed { ver: Ver, source: io::Error },
    DurabilityUncertain {
        id: FileId,
        version: Option<Ver>,
        source: io::Error,
    },
    Io(io::Error),
    HookFailed { ver: Ver, status: i32 },
    EditorFailed { status: i32 },
    NotAStore { root: AbsPath },
    SchemaTooNew { found: u32, supported: u32 },
    SchemaTooOld { found: u32, supported: u32 },
    Internal(String),
}
```

`Error::exit_code` 是唯一的映射入口（0/1/2/3/4/5/6/7/8/9/70）。禁止在调用点拼错误字符串再匹配（0.x 的 F10 即由此产生）。退出码 9 表示「清单 `rename` 已成功、版本可见，但父目录 `fsync` 失败，持久性未确认」；它不同于 8（未提交的 IO/权限失败），见 [[ADR__exit-9-when-durability-uncertain]]。

## 5. 命令行

| 命令 | 参数 | 行为 |
|---|---|---|
| `bakon init` | `--store` | 创建存储目录与标记；已存在且合法则无操作 |
| `bakon edit <file>` | `--editor <cmd>`、`-m/--note <text>`（全局 `--no-hook`） | 首次自动纳管；编辑器退出后按需提交；备注只写入新版本 |
| `bakon log <file>` | `--format` | 最新在前；大小取自清单；显示备注 |
| `bakon diff <file> [v1] [v2]` | — | 0 个参数比较最近两版；1 个参数与它的前一版比较；2 个参数中 v1 为旧、v2 为新 |
| `bakon show <file> <v>` | — | 原始字节输出 |
| `bakon dump <file> <v>` | — | 与 `show` 相同，保留为管道命令名 |
| `bakon revert <file> <v>` | `--force`、`-m/--note <text>`（全局 `--no-hook`） | 分歧默认拒绝；`--force` 先记 `salvage`；备注只写入新版本 |
| `bakon note <file> <v> [text]` | `--clear` | 省略 text 显示备注；给 text 设置或替换；`--clear` 删除 |
| `bakon ls` | — | 排序后的路径列表 |
| `bakon mv <old> <new>` | — | 只改映射，不动磁盘文件 |
| `bakon prune [<file>]` | `--dry-run` | 默认按保留上限；不重写历史 |
| `bakon hook <set\|unset\|show> <file> [cmd]` | — | 写入清单 |
| `bakon path <file>` | `--format json` | 输出路径、id、清单路径 |
| `bakon verify` | `--fix`、`--size` | 一致性检查与修复 |
| `bakon config <show\|init>` | — | 配置与生效值 |
| `bakon version` | `--format json` | 版本与目标三元组 |
| `bakon store <path\|size>` | — | 存储位置与占用汇总 |

全局：`--store`、`--config`、`--format human|plain|json`、`--no-hook`、`--lock-timeout`。输出不使用颜色，因此没有 `--no-color`。

`edit` 的编辑器取 `--editor` → `$VISUAL` → `$EDITOR` → 配置 `editor` → `vi`；诊断与进度走 stderr，stdout 只放数据。

配置文件位置：`--config` > `$BAKON_CONFIG` > `$BAKON_HOME/config.toml` > `$XDG_CONFIG_HOME/bakon/config.toml` > `~/.config/bakon/config.toml`。v1 不读取 0.x 的 `~/.bakon/config.toml`，避免沿用指向旧 `repo/` 的 `store`。即使显式给出 `--store`，配置文件仍会被解析以取得 retention 与 editor；配置损坏时退出 2。配置文件写入使用临时文件 + `fsync` + `rename`，并拒绝未知字段。

## 6. 输出格式

`log --format json` 每行一个对象：

```json
{"ver":3,"time":"2026-11-10T12:03:11Z","size":428,"source":"edit","note":"把端口改成 8843"}
```

没有备注时 `note` 为 `null`。`plain` 在原有 `ver`、`time`、`size`、`source` 之后追加第五个字段 `note`，无备注时为空字符串。

`verify --format json`：

```json
{"ok":false,"items":[{"kind":"missing_blob","path":"/etc/hosts","ver":2,"detail":"…"}]}
```

`plain` 用制表符分隔固定字段；`human` 允许演进，不作为脚本接口。stdout 只放数据，诊断与进度一律走 stderr。

## 7. 兼容承诺

| 面 | 承诺 |
|---|---|
| 清单与标记 `schema` | 主版本内向后兼容；升级需迁移命令。清单 schema 1 无备注、2 有备注；读取器接受 1 和 2 |
| `log --format json` 字段 | 只增不删，语义不变；`note` 为新增字段 |
| `log --format plain` 字段 | 只在末尾追加；第五列为 `note` |
| 退出码 | 永久稳定；新增码不复用旧含义 |
| 命令语法 | 主版本内兼容，弃用需一个次版本 |
| 错误消息文本 | **不承诺**；脚本只能依赖退出码与 JSON |

最后一行是有意的：0.x 的退出码依赖消息文本，任何措辞调整都会改变脚本行为。

## 8. 明确不做

- 不做目录级管理、不做文件内容合并、不做跨机同步。
- 不提供库 API 的稳定承诺：`bakon-core` 随主版本变化。
- 不提供钩子重试与事务回滚；钩子不是事务的一部分。

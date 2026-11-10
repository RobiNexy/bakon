# API__store-contract-and-cli

本报告是实现的接口契约：Rust 类型、存储接口的前置与后置条件、命令行行为与输出格式。模型依据见 [[STATE__store-state-machine-and-invariants]]；实现结构见 [[MODULE__bakon-v1-design]]。

## 1. 核心类型

```rust
pub struct AbsPath(PathBuf);      // 规范化绝对路径，构造即校验
pub struct FileId([u8; 16]);      // 随机身份，构造仅由纳管路径产生
pub struct Ver(u32);              // 单调版本号，只能由 Ver::next 产生
pub struct BlobHash([u8; 32]);    // 内容哈希
```

四条约束：

- `FileId` 不能从路径构造，只能由 `FileId::generate()` 产生并通过唯一性检查写入。
- `Ver::next(head)` 是唯一的递增方式；不存在「从字符串解析版本号」的入口。
- `AbsPath` 在构造时绝对化、拒绝经过符号链接的 `..`，并做词法清理；最终目标的符号链接保留。大小写不敏感平台用路径键比较身份。
- 四个类型之间没有隐式转换，编译期阻止混用。

```rust
pub enum Retention { Inherit, Unlimited, Limit(NonZeroU32) }  // JSON: 字段缺省 / {"max_versions":0} / {"max_versions":n}
pub enum Source { Baseline, Edit, Revert, Salvage }           // JSON: 小写字符串
pub enum Codec { Deflate }
```

## 2. 写协议的类型表达

```rust
pub struct Prepared { manifest: Manifest, content: Vec<u8> }
pub struct Committed { id: FileId, version: Ver }

impl Store {
    /// 构造纳管提交：生成身份与基线版本。
    pub fn prepare_track(&self, path: &AbsPath, content: &[u8]) -> Result<Prepared>;

    /// 构造追加提交：版本号与时间在这里生成。
    pub fn prepare_commit(&self, id: FileId, content: &[u8], source: Source) -> Result<Prepared>;

    /// 写入对象与清单，返回提交结果。失败时持久状态或为旧值或为新值。
    /// `expected_head` 是进入编辑器前读到的 head；不一致即冲突退出。
    pub fn commit(&mut self, prepared: Prepared, expected_head: Option<Ver>) -> Result<Committed>;
}

impl Committed {
    /// 只有已提交的版本才能写回目标文件（M2）。
    pub fn apply(self, store: &Store, target: &AbsPath) -> Result<()>;
}
```

`apply` 只存在于 `Committed` 上，因此「写回先于提交」在类型层面不可表达，对应 $\mathbf{S_6}$。

## 3. 存储接口

| 方法 | 前置 | 后置 | 失败 |
|---|---|---|---|
| `Store::open(root, mode)` | 目录归属符合 [[OPS__store-identity-recovery-and-retention]] | 完成 `.pending` 恢复 | 6（未初始化、schema 不符）、8（权限） |
| `resolve(&AbsPath) -> Option<FileId>` | — | 命中缓存或扫描清单；结果可缓存 | 6（清单损坏） |
| `manifest(FileId) -> Manifest` | id 存在 | — | 3（不存在）、6（损坏） |
| `head(FileId) -> Option<VersionRecord>` | — | 返回最后一条版本 | 6 |
| `content(&BlobHash) -> Vec<u8>` | 对象存在 | 解压并校验哈希 | 6（缺失、损坏） |
| `versions(FileId) -> &[VersionRecord]` | — | 升序 | 6 |
| `begin_write() -> WriteGuard` | — | 持排他锁，超时退出 4 | 4、8 |
| `commit(Prepared) -> Committed` | 持有 `WriteGuard`，head 未变 | 追加版本、必要时裁剪 | 4（head 变化）、8、6 |
| `prune(&[FileId]) -> PruneReport` | 持有 `WriteGuard` | 截断后缀、回收对象 | 8 |
| `rename(FileId, AbsPath)` | 持有 `WriteGuard` | 更新清单 `path` 与缓存 | 4（目标已存在） |
| `verify(options) -> Report` | — | 只读；`--fix` 时持排他锁 | 6 |

读取接口持共享锁（`Store::open` 的读模式），写接口持排他锁；`edit` 的编辑器阶段不持锁，提交时用 `expected_head` 比较交换。契约细节见 [[SEQUENCE__write-protocol-and-failure-paths]]。

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
    AlreadyManaged { path: AbsPath },
    Corrupt { what: &'static str, detail: String },
    ApplyFailed { ver: Ver, source: io::Error },
    Io(io::Error),
    HookFailed { ver: Ver, status: i32 },
    EditorFailed { status: i32 },
    NotAStore { root: AbsPath },
    SchemaTooNew { found: u32, supported: u32 },
    SchemaTooOld { found: u32, supported: u32 },
    Internal(String),
}
```

`Error::exit_code` 是唯一的映射入口（0/1/2/3/4/5/6/7/8/70）。禁止在调用点拼错误字符串再匹配（0.x 的 F10 即由此产生）。

## 5. 命令行

| 命令 | 参数 | 行为 |
|---|---|---|
| `bakon init` | `--store` | 创建存储目录与标记；已存在且合法则无操作 |
| `bakon edit <file>` | `--no-hook` | 首次自动纳管；编辑器退出后按需提交 |
| `bakon log <file>` | `--format` | 最新在前；大小取自清单 |
| `bakon diff <file> [v1] [v2]` | — | 0 个参数比较最近两版；1 个参数与其前一版比较 |
| `bakon show <file> <v>` | — | 原始字节输出 |
| `bakon dump <file> <v>` | — | 与 `show` 相同，保留为管道命令名 |
| `bakon revert <file> <v>` | `--force`、`--no-hook` | 分歧默认拒绝；`--force` 先记 `salvage` |
| `bakon ls` | — | 排序后的路径列表 |
| `bakon mv <old> <new>` | — | 只改映射，不动磁盘文件 |
| `bakon prune [<file>]` | `--dry-run` | 默认按保留上限；不重写历史 |
| `bakon hook <set\|unset\|show> <file> [cmd]` | — | 写入清单 |
| `bakon path <file>` | `--format json` | 输出路径、id、清单路径 |
| `bakon verify` | `--fix`、`--size` | 一致性检查与修复 |
| `bakon config <show\|init>` | — | 配置与生效值 |
| `bakon version` | `--format json` | 版本与目标三元组 |
| `bakon store <path\|size>` | — | 存储位置与占用汇总 |

全局：`--store`、`--config`、`--format human|plain|json`、`--no-color`、`--no-hook`、`--lock-timeout`。

`edit` 的编辑器取 `$VISUAL` → `$EDITOR` → 配置 `editor` → `vi`；诊断与进度走 stderr，stdout 只放数据。

配置文件位置：`--config` > `$BAKON_CONFIG` > `$BAKON_HOME/config.toml` > `$XDG_CONFIG_HOME/bakon/config.toml` > `~/.config/bakon/config.toml`。v1 不读取 0.x 的 `~/.bakon/config.toml`，避免沿用指向旧 `repo/` 的 `store`。

## 6. 输出格式

`log --format json` 每行一个对象：

```json
{"ver":3,"time":"2026-11-10T12:03:11Z","size":428,"source":"edit"}
```

`verify --format json`：

```json
{"ok":false,"items":[{"kind":"missing_blob","path":"/etc/hosts","ver":2,"detail":"…"}]}
```

`plain` 用制表符分隔固定字段；`human` 允许演进，不作为脚本接口。stdout 只放数据，诊断与进度一律走 stderr。

## 7. 兼容承诺

| 面 | 承诺 |
|---|---|
| 清单与标记 `schema` | 主版本内向后兼容；升级需迁移命令 |
| `log --format json` 字段 | 只增不删，语义不变 |
| 退出码 | 永久稳定；新增码不复用旧含义 |
| 命令语法 | 主版本内兼容，弃用需一个次版本 |
| 错误消息文本 | **不承诺**；脚本只能依赖退出码与 JSON |

最后一行是有意的：0.x 的退出码依赖消息文本，任何措辞调整都会改变脚本行为。

## 8. 明确不做

- 不做目录级管理、不做文件内容合并、不做跨机同步。
- 不提供库 API 的稳定承诺：`bakon-core` 随主版本变化。
- 不提供钩子重试与事务回滚；钩子不是事务的一部分。

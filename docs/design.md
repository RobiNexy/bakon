# Bakon — 单机文件版本管理工具 · 设计文档

> **历史文档。** 本文描述 0.x（Go + Git）。v1 的设计见 [`design/`](design/)，入口为 [`design/MODULE__bakon-v1-design.md`](design/MODULE__bakon-v1-design.md)。

## 1. 定位


Bakon 是一个单机命令行工具，用于在编辑关键文件时**无感地保留版本历史**，支持查看变更、导出历史版本、回退到任意版本。


- 目标场景：单机、单人、无协作、无分布式
- 底层依赖：`git`（作为版本存储引擎）
- 不涉及：加密、签名、权限加固、跨机同步


---


## 2. 功能范围


### 命令一览


| 命令 | 作用 |
|---|---|
| `bakon edit <file>` | 打开编辑器；无改动则静默退出；有改动则提交为新版本 |
| `bakon log <file>` | 列出该文件的历史版本（序号、时间、大小） |
| `bakon diff <file> [v1] [v2]` | 比较两个版本的差异；缺省比较最近两个 |
| `bakon show <file> <v>` | 输出某版本的完整内容 |
| `bakon revert <file> <v>` | 将目标文件恢复为某版本内容，提交为新版本 |
| `bakon ls` | 列出所有被管理的文件 |
| `bakon mv <old> <new>` | 更新文件路径映射，保留历史 |
| `bakon prune [<file>]` | 按最大版本数裁剪历史；缺省裁剪所有文件 |
| `bakon hook set <file> "<cmd>"` | 设置某文件的变更钩子命令 |
| `bakon hook unset <file>` | 清除某文件的变更钩子命令 |
| `bakon hook show <file>` | 查看某文件的变更钩子命令 |


---


## 3. 目录结构


```text
~/.bakon/
├── config.toml          # 全局配置
└── repo/                # git 仓库，存放所有版本
    ├── .git/
    ├── files/
    │   └── <id>/        # 每个被管理文件一个哈希 ID 目录
    └── index.json       # 目标文件路径 → 记录
```


### 全局配置 `~/.bakon/config.toml`


```toml
editor = "vim"              # 默认编辑器，缺省 vi
store  = "~/.bakon/repo"    # 版本仓库位置


[retention]
max_versions = 100          # 每个文件保留的最大版本数；0 = 不限制
```


### 路径索引 `index.json`


```json
{
  "/etc/nginx/nginx.conf": {
    "id": "a1b2c3d4",
    "max_versions": 200,
    "hook": "systemctl reload nginx"
  },
  "/etc/hosts": {
    "id": "e5f6a7b8"
  }
}
```


字段说明：


| 字段 | 必填 | 说明 |
|---|---|---|
| `id` | 是 | 内部哈希目录名，稳定标识，与路径解耦 |
| `max_versions` | 否 | 覆盖全局 `retention.max_versions` |
| `hook` | 否 | 该文件**内容发生变更**（edit 提交或 revert 提交）成功后执行的 shell 命令 |


按文件存储的配置（`max_versions`、`hook`）是文件的属性，不放入全局配置。


---


## 4. 核心行为：`bakon edit`


```text
1. 读取 config、index
2. 校验 <file> 存在；不存在则报错退出
3. 计算 <file> 当前内容的 sha256
4. 加文件锁 ~/.bakon/repo/.lock
5. 启动 $EDITOR <file>（前台等待退出）
6. 计算 <file> 新内容的 sha256
7. 新旧 hash 相同 → 静默退出，不产生新版本，不触发钩子
8. 新旧 hash 不同 →
     - 分配/沿用内部 ID
     - 将新内容写入 repo/files/<id>
     - git add + git commit（提交信息含真实路径、时间、ver 序号）
     - 若超过该文件 max_versions 限制 → 触发 prune
     - 若该文件配置了 hook → 执行 hook
9. 释放文件锁
```


关键约束：


- 编辑器**原地编辑**目标文件，不复制、不覆盖。
- Bakon 只在仓库内保存副本，不污染目标文件所在目录。
- 并发编辑通过文件锁串行化。


---


## 5. 版本标识


- 内部存储：git commit hash。
- 用户可见：**递增整数序号**（ver），首次分配后不再回收。
- 序号记录在 commit message 中，便于稳定映射。
- 用户操作只使用 ver，不暴露 commit hash。


示例 `bakon log` 输出：


```text
#  ver   time                  size
  3     2025-01-15 10:23:41   1.2K
  2     2025-01-10 09:01:12   1.1K
  1     2025-01-03 18:47:00   1.0K
```


---


## 6. revert 语义


`bakon revert <file> <v>`：


1. 从仓库读出 ver v 的内容
2. 写回目标文件
3. 在仓库中新建一次提交（ver = 当前最大 ver + 1）
4. 若该文件配置了 hook，在提交成功后执行 hook


- 历史为 **append-only**：回退不修改既有历史，只新增版本。


---


## 7. 变更钩子


### 触发时机


钩子与该文件绑定，在**该文件产生新版本时**执行。触发场景统一为：


- `bakon edit` 提交新版本后
- `bakon revert` 提交新版本后


即：**只要产生了新版本，钩子就执行**。


### 行为约定


- 钩子在文件锁**未释放**时执行，避免并发操作期间的钩子冲突。
- 钩子通过系统 shell 执行（`sh -c "<hook>"`）。
- 钩子失败**不影响**版本提交结果（文件已正确保存/恢复），但会反馈非零退出码与错误输出。
- 无变更时（edit 未修改）不触发钩子。


### 管理方式


- 使用 `bakon hook set <file> "<cmd>"` 写入 `index.json` 的 `hook` 字段。
- `bakon hook unset <file>` 清除。
- `bakon hook show <file>` 查看。


---


## 8. 版本裁剪：`prune`


- 触发时机：
  - `bakon edit` 提交后，若该文件版本数超过 `max_versions`，自动裁剪最旧版本。
  - `bakon prune [<file>]` 手动触发全量 / 单文件裁剪。
- 裁剪策略：保留最新的 N 个版本，移除更旧的。
- 实现方式：重写 git 历史，从仓库移除被裁剪的旧对象。
- **已分配的 ver 序号不回收**，裁剪后 `log` 从最旧保留版本开始显示。
- 被裁剪的版本不可恢复——这是由 `max_versions` 显式声明的预期行为。


---


## 9. 与 git 的映射


| Bakon 命令 | 底层 git 操作 |
|---|---|
| `edit`（有改动） | `git add files/<id> && git commit` |
| `log` | `git log -- files/<id>` |
| `diff v1 v2` | `git diff <c1>:files/<id> <c2>:files/<id>` |
| `show v` | `git show <c>:files/<id>` |
| `revert v` | 读旧内容 → 写目标文件 → 新建 commit |
| `prune` | 重写历史，移除该文件 `<id>` 下的旧对象 |
| `ls` | 读 index.json |
| `mv` | 修改 index.json，无需改动 git 历史 |
| `hook set/unset/show` | 修改 index.json 的 `hook` 字段 |


---


## 10. 实现建议


### 语言与依赖


- 语言：**Go**，编译为单文件二进制，跨平台。
- 允许使用现成 Go 库以减少自写代码：


| 用途 | 建议库 |
|---|---|
| 命令行框架（子命令、帮助、补全） | `spf13/cobra` + `spf13/pflag`（或 `urfave/cli`） |
| 配置文件解析（TOML） | `BurntSushi/toml` 或 `pelletier/go-toml/v2` |
| JSON 索引读写 | 标准库 `encoding/json`（足够） |
| Git 操作 | `go-git/go-git`（纯 Go，无需外部 `git` 进程）；或直接 `os/exec` 调用系统 `git` |
| 文件锁 | `gofrs/flock` |
| 内容哈希 | 标准库 `crypto/sha256` |


### git 交互方式的选择


两种方式都可行，按部署偏好择优：


- **调用系统 `git`（`os/exec`）**：行为与用户手动操作一致，`prune` 的 filter-history 有官方工具可用；要求运行环境安装 `git`。
- **`go-git`（纯 Go）**：无外部 `git` 依赖，单二进制即可运行；但历史重写的灵活性略低于官方 `git`，`prune` 需自行实现对象清理。


`[基于常见模式推断]` 若追求"开箱即用"的单二进制体验，选 `go-git`；若追求行为与标准 `git` 完全一致并简化 `prune` 实现，调用系统 `git`。


### 规模预期


借助现成库，核心逻辑（命令分发、index 读写、edit 流程、log/diff/show/revert、hook、prune）通常可在几百行内完成，不含第三方代码。


---


## 11. 版本边界


**做**：


- 上述全部命令
- per-file 的 `max_versions` 与变更钩子（edit / revert 均触发）


**不做**：


- 加密、签名、权限加固
- 跨机同步、协作
- GUI
- 钩子的失败重试 / 事务回滚


---


## 12. 实现补充：并发、崩溃与平台语义


本节记录实现阶段对前述设计的裁决与补充，与正文冲突时以本节为准。


### 实现裁决


- **git 交互**：选定调用系统 `git`（`os/exec`）。prune 需重写历史并清理对象，官方 git 的 `commit-tree` 重放 + `gc --prune=now` 是现成链路；`go-git` 无 gc、无法删除对象。代价：运行环境必须安装 git。
- **edit 的旧内容 hash 在取锁之后计算**（正文的步骤 3/4 顺序在实现中调换）：消除"等锁期间目标文件被上一操作改变 → 幽灵提交"的竞态。
- **prune 实现**：逐提交重放——`diff-tree` 提取每提交的 (路径, blob)，过滤待裁版本后用临时索引 + `commit-tree` 重建线性链（保留原提交信息与日期），`update-ref` 后 `gc --prune=now` 清对象。前提不变量：每个 bakon 提交恰好触碰一个 `files/<id>` 路径（由锁串行化保证）。
- **revert 空变化不提交**：目标内容与最新版本相同时（如 revert 到当前版）不产生新提交——git 拒绝空提交，且空提交会破坏上述不变量。此时仍写回目标文件，不触发钩子。
- **首次纳管基线版本 0**：edit 首次接触某文件时，先提交纳管时刻的内容为版本 0，再启动编辑器；后续变更从版本 1 起，`revert <file> 0` 即恢复纳管前状态。基线提交不触发钩子（钩子语义是"内容变更"，基线未改变内容）。本机制引入前已纳管的文件没有版本 0，其历史从版本 1 起，不做回溯补建。
- **edit 的文件缺失分流**：目标文件不存在时报错且不创建（避免路径打错字时凭空纳管空文件）。错误按状态区分：已跟踪文件被删 → 提示 "tracked file was deleted" 并指向 `bakon revert` 恢复；从未存在的未跟踪文件 → 提示 "no such file"。
- **配置命令**：`bakon config show` 打印配置路径与生效值（含优先级合成后的 editor）；`bakon config init` 写入带注释的默认模板，已存在时不覆盖。配置文件缺失时所有命令以默认值工作，不做首次运行交互式询问——保持 CLI 可脚本化。


### 只读命令与锁


写操作（edit / revert / prune / mv / hook set、unset）全部持有 `repo/.lock`。只读命令（ls / log / diff / show）**不取锁**：index.json 经临时文件 + rename 原子写，读者不会看到半截状态；git 对象不可变，历史读操作自洽。代价：读到的快照可能落后一次正在进行的提交，对只读展示可接受；收益：log/diff 不会被他人挂起的编辑会话阻塞。


### edit 期间的崩溃


编辑器退出后、commit 完成前进程中断：目标文件已改、仓库无该版本。**不做启动时检测**——该状态与"用户绕过 bakon 手动修改过文件"不可区分，且 append-only 语义下下一次 `bakon edit` 会把当前内容补提交为新版本，无数据丢失。其他崩溃窗口：index.json 半截写入由原子写消除；锁残留由 flock 的内核级进程死亡自动释放消除。


### 被裁剪版本的错误提示


`show` / `revert` / `diff` 引用一个不存在的版本号时，错误信息区分：


- `version <v> has been pruned (oldest retained version is <k>)`：v 早于最旧保留版本，是 `max_versions` 的预期结果，不可恢复；
- `no version <v> (latest is <k>)`：v 大于最新版本，从未存在。


### 平台差异


- 编辑器与钩子经 shell 启动：unix 为 `sh -c`，Windows 为 `cmd /C`。钩子脚本作者需自行保证其在目标平台可执行，bakon 不做脚本翻译。
- Windows 上 `cmd` 对 `%`、`^`、`&` 等元字符的转义不完整，含此类字符的路径在编辑器拼接时可能出错。
- Android Termux 使用 `linux/arm64`（或 `linux/arm`）静态二进制，直接在 Termux shell 中运行。


### 钩子与提权（sudo）


钩子是任意 shell 命令，含 `sudo` 时同样原样执行；bakon 不拦截、不解释、不代为提权，也不缓存凭证——属于"不做权限加固"的边界。


- bakon 以 root 运行（编辑 `/etc` 文件的典型场景）时，钩子已是 root 身份，不应再套 `sudo`：部分 sudoers 配置（`requiretty` 等）下 root 执行 sudo 反而失败。
- 非 root 运行、钩子需提权时：钩子的 stdin 接 `/dev/null`（`exec.Cmd` 零值行为）且无 TTY，sudo 无法读取密码会直接非零退出而非挂起 [基于模式推断]，错误输出透传，bakon 以退出码 2 提示，版本提交不受影响。正确解法是 sudoers 精确白名单（`NOPASSWD` 限定单条命令）或 polkit；不建议全量放行。
- 钩子与版本提交的失败隔离不变：提权失败只影响钩子事务，不影响已完成的版本。

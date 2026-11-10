# Bakon

> 编辑关键文件时，无感地保留版本历史。

[English](README.en.md) | 中文

[![CI](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml/badge.svg)](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml)

Bakon 是面向个人开发者的单机命令行文件版本管理工具：用熟悉的编辑器原地修改 `nginx.conf`、`hosts`、crontab 等关键文件，同时把每次变更保存进独立的本地存储，支持查看历史、比较差异、导出指定版本与安全回退。

v1 使用 Rust 实现，自建内容寻址存储，不调用 git 或任何外部命令。

- 单机、单人，不提供远端同步、多人协作或分支合并
- 版本号是递增整数；回退只追加新版本，旧版本始终可追溯
- 目标文件由用户自己的编辑器原地修改，Bakon 只保存副本
- 全局保留上限、变更钩子、原子写入、写锁与崩溃恢复
- 单文件二进制，零运行时依赖

当前分支 `v1-rust` 是 v1 主线；`main` 保留 0.x（Go + Git）历史。

## 构建与安装

工具链由 `rust-toolchain.toml` 固定：

```sh
cargo build --release -p bakon
cargo test --workspace --all-targets

target/release/bakon version
cargo install --path crates/bakon
```

v1 的发布流程尚未接入，当前从源码构建。

## 快速开始

```sh
bakon init
bakon edit /etc/nginx/nginx.conf

bakon log /etc/nginx/nginx.conf
bakon diff /etc/nginx/nginx.conf
bakon show /etc/nginx/nginx.conf 1
bakon revert /etc/nginx/nginx.conf 1

bakon hook set /etc/nginx/nginx.conf "systemctl reload nginx"
bakon verify
```

首次 `edit` 会自动纳管文件，并把编辑前的内容保存为版本 0。

## 命令

| 命令 | 作用 |
|---|---|
| `bakon init` | 创建存储，或校验已有存储 |
| `bakon version` | 输出程序版本与构建目标 |
| `bakon edit <file>` | 打开编辑器；内容有变化时追加版本 |
| `bakon log <file>` | 列出历史版本，最新在前 |
| `bakon show <file> <ver>` | 输出指定版本的原始内容 |
| `bakon dump <file> <ver>` | 同 `show`，供管道使用 |
| `bakon diff <file> [v1] [v2]` | 比较版本；0 个参数比较最近两版，1 个参数与其前一版比较 |
| `bakon revert <file> <ver>` | 恢复版本，并把恢复动作追加为新版本；`--force` 先记录当前内容为 `salvage` |
| `bakon prune [<file>]` | 按保留上限裁剪；`--dry-run` 只报告 |
| `bakon mv <old> <new>` | 只更新路径映射，不移动磁盘文件 |
| `bakon hook set\|unset\|show <file> [cmd]` | 管理每文件变更钩子 |
| `bakon config show\|init` | 查看生效配置或写入默认配置 |
| `bakon verify` | 检查一致性；`--fix` 修复派生数据，`--size` 报告占用 |

全局选项：`--store <dir>`、`--config <file>`、`--format human|plain|json`、`--lock-timeout <ms>`（默认 2000）。

`stdout` 只输出数据，诊断与进度输出到 `stderr`。

## 存储

默认位置是 `~/.bakon/store`。优先级：

1. `--store`
2. `BAKON_STORE`
3. 配置文件的 `store`
4. `$BAKON_HOME/store`
5. `~/.bakon/store`

```text
store/
├── BAKON_STORE          归属与 schema 标记
├── .lock                排他/共享锁
├── .pending             待写回标记（存在时才有）
├── index.json           路径到身份的派生缓存，可重建
├── files/<id>.json      权威版本清单
├── objects/aa/<rest>    内容寻址对象（deflate）
├── objects/tmp/         写入暂存
└── trash/               待删除对象
```

清单是版本事实的唯一来源，对象按内容哈希命名并去重。备份就是复制整个存储目录；恢复后执行 `bakon verify`。`verify` 报不一致时退出码为 6，`--fix` 只修复尺寸字段、索引与残留目录，不猜测历史。

v1 不读取 0.x 的 `~/.bakon/repo`，也不自动迁移。旧目录与旧二进制可以并存，需要旧历史时先用 0.x 导出。

## 配置

配置文件位置，优先级从高到低：

1. `--config`
2. `BAKON_CONFIG`
3. `$BAKON_HOME/config.toml`
4. `$XDG_CONFIG_HOME/bakon/config.toml`
5. `~/.config/bakon/config.toml`

`bakon config init` 写入默认配置：

```toml
# store = "/absolute/path/to/store"
# editor = "vi"

[retention]
max_versions = 100   # 0 = 不限制
```

编辑器按 `$VISUAL` → `$EDITOR` → 配置 `editor` → `vi` 的顺序选择。

## 钩子

钩子在版本提交之后、锁释放之后执行，至多执行一次。执行环境包含：

```text
BAKON_FILE       目标文件绝对路径
BAKON_VERSION    新版本号
BAKON_SOURCE     edit 或 revert
BAKON_PREV       上一版本号；首个版本为空
BAKON_HOOK=1     防止钩子递归调用
```

钩子失败不会回滚已保存的版本，命令退出码为 5。`--no-hook` 可跳过钩子。

## 平台说明

- Windows 没有目录 `fsync` 的等价语义，崩溃恢复可能丢失最后一次提交，但不会产生清单与对象混合的状态。
- 编辑器与钩子在 Unix 上经 `sh -c` 执行，在 Windows 上经 `cmd /C` 执行。
- 存储只支持本地文件系统；网络文件系统上的锁与原子替换不在保证范围内。

## 退出码

| 代码 | 含义 |
|---:|---|
| 0 | 成功、无改动，或 `verify` 一致 |
| 1 | `diff` 发现差异；只有这一种情况使用 1 |
| 2 | 参数、配置或命令用法错误 |
| 3 | 文件或版本不存在，或版本已被裁剪 |
| 4 | 锁超时、并发冲突或路径状态冲突 |
| 5 | 版本已保存，但钩子或编辑器失败 |
| 6 | 存储损坏、不一致、非 bakon 存储或 schema 不符 |
| 7 | 版本已记录，但写回目标文件失败 |
| 8 | 权限或一般 IO 错误 |
| 70 | 内部错误 |

## 设计文档

- 设计入口：[`design/MODULE__bakon-v1-design.md`](design/MODULE__bakon-v1-design.md)
- 状态机、数据格式、写入协议与 CLI 契约见 [`design/`](design/)
- v1 设计与实现的再次形式化审计：[`docs/design-audit-v1-2026-11-10.md`](docs/design-audit-v1-2026-11-10.md)
- 0.x 的形式化审计（历史证据）：[`docs/design-audit-2026-11-10.md`](docs/design-audit-2026-11-10.md)

## License

[MIT](LICENSE)

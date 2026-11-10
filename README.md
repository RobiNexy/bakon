# bakon

bakon 是一个单机命令行文件版本管理工具：用编辑器原地修改关键文件，把每次变更保存为追加式历史，支持查看、比较、导出和回退。版本号只增不减，回退会新增版本。

## 构建与安装

需要 Rust，工具链由 `rust-toolchain.toml` 固定。

```sh
cargo build --release -p bakon
target/release/bakon version

# 或安装到 cargo 的 bin 目录
cargo install --path crates/bakon
```

也可以直接把 `target/release/bakon` 复制到 `PATH` 中的目录。

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
| `bakon version` | 输出版本与构建目标 |
| `bakon edit <file>` | 打开编辑器；内容有变化时追加版本 |
| `bakon log <file>` | 列出历史版本，最新在前 |
| `bakon ls` | 列出全部被管理路径 |
| `bakon path <file>` | 输出路径、身份与清单文件位置 |
| `bakon store <path\|size>` | 输出存储位置或磁盘占用 |
| `bakon show <file> <ver>` | 输出指定版本的原始内容 |
| `bakon dump <file> <ver>` | 同 `show`，供管道使用 |
| `bakon diff <file> [v1] [v2]` | 比较版本；有差异时退出 1 |
| `bakon revert <file> <ver>` | 恢复版本并追加新版本；`--force` 先记录当前内容为 `salvage` |
| `bakon prune [<file>]` | 按保留上限裁剪；`--dry-run` 只报告 |
| `bakon mv <old> <new>` | 只更新路径映射，不移动磁盘文件 |
| `bakon hook set\|unset\|show <file> [cmd]` | 管理每文件钩子 |
| `bakon config show\|init` | 查看生效配置或写入默认配置 |
| `bakon verify` | 检查一致性；`--fix` 修复，`--size` 报告占用 |

全局选项：`--store <dir>`、`--config <file>`、`--format human|plain|json`、`--lock-timeout <ms>`。

`stdout` 只输出数据，诊断与进度输出到 `stderr`。

## 存储与配置

默认存储位置是 `~/.bakon/store`。优先级：

1. `--store`
2. `BAKON_STORE`
3. 配置文件的 `store`
4. `$BAKON_HOME/store`
5. `~/.bakon/store`

配置文件位置：

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

备份时复制整个存储目录；恢复后执行 `bakon verify`。`verify` 发现不一致时退出 6。

## 钩子

钩子在版本提交之后、锁释放之后执行，至多一次。执行环境包含：

```text
BAKON_FILE       目标文件绝对路径
BAKON_VERSION    新版本号
BAKON_SOURCE     edit 或 revert
BAKON_PREV       上一版本号；首个版本为空
BAKON_HOOK=1     防止钩子递归调用
```

钩子失败不会回滚已保存的版本，命令退出 5。`--no-hook` 可跳过钩子。

## 退出码

| 代码 | 含义 |
|---:|---|
| 0 | 成功、无改动，或 `verify` 一致 |
| 1 | `diff` 发现差异 |
| 2 | 参数、配置或命令用法错误 |
| 3 | 文件或版本不存在，或版本已被裁剪 |
| 4 | 锁超时、并发冲突或路径状态冲突 |
| 5 | 版本已保存，但钩子或编辑器失败 |
| 6 | 存储损坏、不一致、非 bakon 存储或 schema 不符 |
| 7 | 版本已记录，但写回目标文件失败 |
| 8 | 权限或一般 IO 错误 |
| 70 | 内部错误 |

## License

[MIT](LICENSE)

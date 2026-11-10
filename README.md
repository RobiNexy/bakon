# Bakon

> 编辑关键文件时，无感地保留版本历史。

> **状态：v1 重写进行中。** `main` 上的 0.x 使用 Go + Git；`v1-rust` 分支用 Rust 重写并自建内容寻址存储，已完成 M0–M2。下文安装与命令说明仍描述 0.x；v1 的构建方式见「v1（开发中）」，设计见 [`design/`](design/)。

[English](README.en.md) | 中文

[![CI](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml/badge.svg)](https://github.com/RobiNexy/Bakon/actions/workflows/ci.yml)

Bakon 是一个单机命令行文件版本管理工具。它使用熟悉的编辑器原地修改 `nginx.conf`、`hosts`、crontab 等关键文件，同时把每次变更保存到独立的 Git 仓库，支持查看历史、比较差异、导出版本和安全回退。

- 单机、单人、无协作，不提供远端同步或分支合并
- 文件历史使用递增整数版本号，回退只追加新版本，不覆盖旧历史
- 编辑器原地操作目标文件，Bakon 只在仓库中保存副本
- 支持每文件保留上限、变更钩子、原子写入和并发锁

## v1（开发中）

Rust 工作区位于 `crates/`，不依赖 git，存储默认 `~/.bakon/store`：

```sh
cargo build --release -p bakon
cargo test --workspace
target/release/bakon edit /etc/nginx/nginx.conf
```

设计入口：[`design/MODULE__bakon-v1-design.md`](design/MODULE__bakon-v1-design.md)。发布产物与 Windows 语义尚未完成，见任务板。

## 安装

从 [Releases](../../releases) 下载对应平台的压缩包。校验值位于 `checksums.txt`。

| 压缩包 | 平台 |
|---|---|
| `bakon_<ver>_linux-amd64.tar.gz` | Linux x86-64 |
| `bakon_<ver>_linux-arm64.tar.gz` | Linux arm64 |
| `bakon_<ver>_termux-arm64.tar.gz` | Android Termux aarch64 |
| `bakon_<ver>_linux-armv7.tar.gz` | Linux armv7 / Android Termux 32 位 |
| `bakon_<ver>_darwin-amd64.tar.gz` | macOS Intel |
| `bakon_<ver>_darwin-arm64.tar.gz` | macOS Apple Silicon |
| `bakon_<ver>_windows-amd64.zip` | Windows x86-64 |
| `bakon_<ver>_windows-arm64.zip` | Windows arm64 |

### Linux / macOS

```sh
BAKON_VER=v0.1.0
curl -fLo /tmp/bakon.tar.gz \
  "https://github.com/RobiNexy/Bakon/releases/download/${BAKON_VER}/bakon_${BAKON_VER}_linux-amd64.tar.gz"
tar -xzf /tmp/bakon.tar.gz -C /tmp
install /tmp/bakon_${BAKON_VER}_linux-amd64/bakon ~/.local/bin/bakon
```

### Android Termux

Termux 使用单独发布的 `termux-arm64` 归档；运行时仍需安装 `git`。

```sh
pkg install git
BAKON_VER=v0.1.0
curl -fLo /tmp/bakon.tar.gz \
  "https://github.com/RobiNexy/Bakon/releases/download/${BAKON_VER}/bakon_${BAKON_VER}_termux-arm64.tar.gz"
tar -xzf /tmp/bakon.tar.gz -C /tmp
mkdir -p "$PREFIX/bin"
install /tmp/bakon_${BAKON_VER}_termux-arm64/bakon "$PREFIX/bin/bakon"
```

### 从源码安装

```sh
go install github.com/RobiNexy/Bakon@latest
```

或：

```sh
git clone https://github.com/RobiNexy/Bakon && cd Bakon
go build .
```

运行时依赖：系统 `git`；编辑器和钩子在 Unix 上经 `sh` 执行，在 Windows 上经 `cmd /C` 执行。

## 快速开始

```sh
# 首次编辑会保存纳管前内容为版本 0
bakon edit /etc/nginx/nginx.conf

bakon log /etc/nginx/nginx.conf
bakon diff /etc/nginx/nginx.conf
bakon show /etc/nginx/nginx.conf 2
bakon revert /etc/nginx/nginx.conf 2
bakon hook set /etc/nginx/nginx.conf "systemctl reload nginx"
```

## 命令

| 命令 | 作用 |
|---|---|
| `bakon edit <file>` | 打开编辑器；有变更时提交新版本 |
| `bakon log <file>` | 列出历史版本；`--format json` 输出 JSON Lines |
| `bakon diff <file> [v1] [v2]` | 比较版本；有差异时退出码为 1 |
| `bakon show <file> <v>` | 输出指定版本内容 |
| `bakon dump <file> <v>` | 二进制原样输出指定版本，供管道使用 |
| `bakon revert <file> <v>` | 恢复版本并追加一次新版本 |
| `bakon ls` | 列出所有被管理文件 |
| `bakon mv <old> <new>` | 更新路径映射并保留历史 |
| `bakon prune [<file>]` | 按保留上限裁剪历史 |
| `bakon hook set/unset/show` | 管理文件变更钩子 |
| `bakon path <file>` | 输出文件的稳定内部映射 |
| `bakon verify` | 检查仓库与索引一致性 |
| `bakon config show/init` | 查看或初始化配置 |
| `bakon version` | 输出版本与构建信息 |

全局选项：`--config`、`--store`、`--format human|plain|json`、`--no-color`、`--no-hook`。

## 配置

默认配置使用 XDG 路径：

- 配置：`$XDG_CONFIG_HOME/bakon/config.toml`
- 数据仓库：`$XDG_DATA_HOME/bakon/repo/`
- 未设置 XDG 变量时兼容使用 `~/.bakon/`
- `BAKON_HOME` 可将配置和仓库放到指定目录，适合测试与隔离运行

```toml
editor = ""                 # 优先级：配置 > BAKON_EDITOR > VISUAL > EDITOR > vi
store = "~/.bakon/repo"

[retention]
max_versions = 100           # 0 = 不限制

[output]
color = "auto"              # auto | always | never
format = "human"             # human | plain | json
```

环境变量覆盖包括 `BAKON_EDITOR`、`BAKON_STORE`、`BAKON_FORMAT`、`BAKON_COLOR` 和 `BAKON_RETENTION_MAX_VERSIONS`。每文件的 `hook` 与 `max_versions` 保存在仓库的 `index.json` 中。

## 钩子

钩子在 `edit` 或 `revert` 产生新版本后、释放锁前执行。执行环境包含：

```text
BAKON_FILE       目标文件绝对路径
BAKON_VERSION    新版本号
BAKON_SOURCE     edit 或 revert
BAKON_PREV       上一个版本号；首次为空
```

钩子失败不会回滚已经保存的版本，命令退出码为 5。紧急场景可使用 `--no-hook` 跳过钩子。

## 退出码

| 代码 | 含义 |
|---:|---|
| 0 | 成功 |
| 1 | diff 发现差异或一般失败 |
| 2 | 参数或命令用法错误 |
| 3 | 文件或版本不存在，或版本已被裁剪 |
| 4 | 锁竞争或状态冲突 |
| 5 | 版本已保存，但钩子失败 |
| 6 | 仓库损坏或不一致 |
| 70 | 内部错误 |

版本号不会因 `prune` 回收；被裁剪的版本不可恢复。`revert` 到当前内容时不会创建空版本。

设计文档：[docs/design.md](docs/design.md) · [docs/new_design.md](docs/new_design.md)

## License

[MIT](LICENSE)

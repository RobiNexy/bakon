# Project purpose and boundary

> Owned by the human. The agent may draft; write only after explicit confirmation. Mark unknowns as "Unconfirmed". The content language follows the project.

## One-line positioning

Bakon 是面向个人开发者的单机命令行文件版本管理工具：用熟悉的编辑器原地修改 `nginx.conf`、`hosts`、crontab 等关键文件，同时把每次变更保存进独立的自建内容寻址存储（2026-11-10 人工确认；v1 不依赖 Git）。

## Goals and success criteria

- 编辑关键文件时无感地保留版本历史；事后能查看历史、比较差异、导出指定版本并安全回退。
- 回退不破坏历史：回退只追加新版本，旧版本始终可追溯。
- 不打断既有习惯：目标文件仍由用户自己的编辑器原地修改，Bakon 只在仓库中保存副本。

## Explicitly out of scope

- 协作与共享：单机、单人使用，不提供远端同步、多人协作或分支合并。

## Key constraints

- People: 单开发者，无协作
- Stack: Rust；工具链由 `rust-toolchain.toml` 固定；纯 Rust 依赖；单文件二进制，零运行时依赖
- Deployment target: 以 GitHub Releases 分发的单文件二进制，覆盖 linux（amd64、arm64、armv7）、macOS（amd64、arm64）与 Windows（amd64、arm64）
- Budget and compliance: Unconfirmed

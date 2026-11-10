# ADR__replace-git-with-own-content-addressed-store

## Metadata

- Status: Accepted
- Proposed: 2026-11-10
- Accepted: 2026-11-10
- Supersedes: none

## Context and decision

Bakon 0.x 把 git 同时用作对象存储、历史模型与裁剪机制：`internal/gitx` 调用 17 类 git 子命令，prune 通过 `commit-tree` 重放重建历史，再用 `gc --prune=now` 回收对象。2026-11-10 的形式化审计（[[design-audit-2026-11-10]]）给出 13 项缺陷，其中四项直接来自这一选择：F1（prune 重写无关仓库并丢弃文件）、F2（提交路径随工作树偏移，自己读不回来）、F12（稳态 edit 成本 $O(|\mathcal{G}|)$）、F13（`GIT_DIR` 劫持）。

决定：移除对系统 git 的运行时依赖，自建内容寻址对象库与版本清单。版本号由清单分配，不再有「提交」概念，裁剪不再重写任何历史。

## Rationale

- git 提供了三件事：内容寻址、不可变提交链、可达性回收。前两件自建约需数百行；第三件在清单模型下退化为「引用即存活，其余可回收」，比重写历史简单。
- 裁剪的代价从 $O(|\mathcal{G}|)$ 降到 $O(k)$（$k$ 为被删版本数），且不再需要「仓库中只有 bakon 提交」这个无法由锁保证的前提。
- 备选方案：(a) 保留提交模型、只加守卫——F12 与前提问题仍在；(b) 把 git 降为对象存储——仍依赖 git 二进制，且对象库格式与 gc 语义仍外置；(c) 完全自建。选定 (c)。
- 依赖数量与安装体验：单文件二进制、无外部命令，安装与容器镜像都更简单。

## Consequences

- 运行环境只需 bakon 本体；不再要求 git，也不再受 `GIT_*` 环境变量影响。
- 失去 `git log` / `git show` 的直接可读性。`bakon log` 成为唯一历史接口，故障排查依赖 `bakon verify`。
- 备份与恢复变成「拷贝一个目录」；不再有 packfile、refs、reflog、fsck。
- 自建部分必须承担：原子写、完整性校验、空间回收、格式版本化。四项见 [[DATA__blob-store-and-durability]] 与 [[OPS__store-identity-recovery-and-retention]]。
- 与 0.x 存储不互通。旧 `repo/` 保持原样，v1.0 不提供自动迁移，见 [[OPS__store-identity-recovery-and-retention]]。
- 放弃 git 的 delta 压缩：同一文件的相邻版本各自压缩。对 KB 级配置文本，压缩后体积仍远小于原文件的线性累积；换取随机读与实现简单。

## Revisit when

- 需要跨机共享历史、多人协作或审计签名时，自建模型的正确性成本会超过复用 git 的价值。
- 存储规模增长到单文件历史压缩率成为主要瓶颈时，可重新评估 delta 或 pack 格式。

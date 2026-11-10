# ADR__rewrite-in-rust

## Metadata

- Status: Accepted
- Proposed: 2026-11-10
- Accepted: 2026-11-10
- Supersedes: none

## Context and decision

审计的 13 项缺陷没有一项来自语言、运行时或性能：它们全部是状态模型、前提边界与破坏性写顺序的问题。但其中两类恰好可以由类型系统在编译期排除：身份混用（F8，同一个字符串既当路径派生键又当稳定身份），以及「未提交状态被当作已提交」（F3，写回先于持久化）。

决定：用 Rust 重写，并把不变量编码进类型。`FileId`、`Ver`、`AbsPath` 为互不兼容的 newtype；写协议用 typestate 表达，只有 `Committed` 状态下才存在「应用到目标文件」这一操作。

## Rationale

- 候选：Go 加 newtype（成本最低）、Rust 重写、Zig。选 Rust 的理由是枚举与所有权让状态机成为单一事实：非法状态不可构造，错误路径由 `Result` 强制处理，而不是靠约定。
- 本次审计的证据支持这一取舍：F8 在 Go 里只需两个同类型字符串就会发生，Rust 的 newtype 会让它编译失败；F3 的顺序错误可以用「破坏性写只对 `Committed` 可见」的签名消除。
- 成本：约 2500 行重写、31 个测试重写；交叉编译从 `GOOS/GOARCH` 一条命令变为每个目标一套工具链。
- 收益边界：启动时间、内存占用、二进制体积都不构成理由。本次决策只换取编译期约束。

## Consequences

- 模型文档（[[STATE__store-state-machine-and-invariants]]、[[DATA__blob-store-and-durability]]、[[DATA__version-manifest-and-numbering]]、[[SEQUENCE__write-protocol-and-failure-paths]]、[[OPS__store-identity-recovery-and-retention]]、[[API__store-contract-and-cli]]）保持语言无关。若撤销本决策，模型与契约仍然有效，只有 [[MODULE__bakon-v1-design]] 的模块与依赖章节需要重写。
- 交叉编译成为 CI 的主要成本：需要为 linux amd64/arm64/armv7、darwin amd64/arm64、windows amd64/arm64 各备工具链，并逐个校验产物。
- 依赖选择受静态链接约束：优先纯 Rust 实现，避免需要 C 工具链的 crate。`flate2` 使用其 Rust 后端，`similar` 用于 diff，`sha2` 用于内容寻址。
- 初期开发速度低于 Go；以类型安全换取的确定性要在重构完成后才体现。

## Revisit when

- 交叉编译或工具链维护成本持续超过收益。
- 现有维护者不再熟悉 Rust，导致修复延迟。

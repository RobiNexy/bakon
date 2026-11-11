# Project state now

> What is true now that is not obvious from code or Git and that the next session needs. Normally updated during compaction ([[AGENT__procedure__compaction]]). Delete empty sections; the file may be empty.

## Current state

- v1（Rust）是当前实现，已位于 `main`；0.x 的 Go 实现、`go.mod`、`internal/`、Go 发布脚本与 Go CI 已从代码树删除，只保留在历史提交与旧标签中。依据：[[ADR__replace-git-with-own-content-addressed-store]]、[[ADR__rewrite-in-rust]]。
- v1 形式论证与详细设计已完成；M0–M2 已落地在 `crates/`：对象层、清单层、`init`/`edit`/`log`/`ls`/`path`/`show`/`diff`/`revert`/`prune`/`mv`/`hook`/`config`/`verify`/`store`、写回与崩溃恢复。13 项审计缺陷的回归用例在 `crates/bakon/tests/regressions.rs`。
- M3 已完成崩溃注入、并发用例、性能实测、7 个目标的 `cargo check`，以及 Windows/macOS runner 冒烟（build、`init`、`verify`；Windows 另含 `edit`/`log`/`revert`）。cargo Release 工作流已在 `v1.0.0` 与 `v1.0.1` 上成功运行，7 个归档与 `checksums.txt` 已校验；Windows 钩子与并发用例未验证。设计入口：[[MODULE__bakon-v1-design]]。
- 0.x 的 `docs/design.md` 与 `docs/new_design.md` 已删除；`docs/` 保留审计报告（[[design-audit-2026-11-10]]、[[design-audit-v1-2026-11-10]]、[[design-audit-v1-2026-11-10-recheck]] 与 [[design-audit-v1-2026-11-11]]）。`README.md`、`README.en.md` 与项目文档只描述 v1。
- 第二次形式化审计的 P1 已修复：裁剪与 `prune` 的孤儿对象会回收，`init` 能清理崩溃留下的标记临时文件，`verify` 会报告 pending 与 `.pending`，恢复会先 `salvage` 崩溃后的外部编辑；`ls`/`path`/`store` 已补齐。复核见 [[design-audit-v1-2026-11-10-recheck]]，剩余 P2/P3 见任务板。
- 第三次全量审计（[[design-audit-v1-2026-11-11]]）覆盖 21 个 Rust 文件，审计时 120 个测试；结论为 2 项 P1（`init` 删除 `BAKON_STORE.*.tmp` 形状的用户文件、恢复对不可判定 pending 假成功并删除 `.pending`）、11 项 P2、7 项主 P3 与 13 项次要观察。正常 CLI 主路径未发现新的数据损坏反例；外部评审 10 项中 7 项成立、2 项部分成立、1 项仅为可读性。两项 P1 已在下一条中修复。
- A-01/A-02 已修复：`init` 只忽略严格数字形状的 `BAKON_STORE.<pid>.<seq>.<rand>.tmp`，匹配项也不删除；非匹配用户文件仍拒绝初始化。恢复现在校验 `entry.version`/`new_blob`，清单缺失、版本被裁剪或不匹配时保留 `.pending` 并退出 6；只有可证明未提交的记录才丢弃，`verify --fix` 报告 `pending_stale`。
- A-03/A-04/A-05/A-06 已修复：`revert` 目标已匹配且存在时 no-op，不截断重写；`commit` 的 track 分支检查路径唯一；`prune` 对 `FileId` 去重并使用 checked 计数；`Store::open` 只打开已有存储，`init` 命令与首次 `edit` 才创建。
- A-07/A-08/A-09 已修复：`AbsPath::new` 拒绝任何 `..`（退出 2），`join` 收紧为 `pub(crate)`；Unix 新建目录为 `0700`、新建文件为 `0600`；锁拆为 `ReadLock`/`WriteLock`，所有写接口要求 `&WriteLock`，并有一个 `compile_fail,E0308` 文档用例。
- A-11 已修复：`verify` 只用成功解析的清单构造引用集；损坏清单只形成报告项，不再中断。存在损坏清单时报告 `orphan_scan`，`--fix` 跳过孤儿回收与索引修复，避免删除仍可能被引用的对象。
- A-12 已修复：`hook set/unset` 与 `config init` 支持 human/plain/json；`--no-hook` 提升为全局参数，子命令前后均可使用；API 文档删除未实现的 `--no-color`。
- A-20 已按 [[ADR__exit-9-when-durability-uncertain]] 实现：清单 `rename` 为提交点，父目录 `fsync` 失败返回退出码 9；`edit` 的可见版本、`revert` 的 `.pending` 保留与后续恢复有故障注入用例；对象、`.pending`、索引与标记的写入仍严格返回 8；当前测试总数 142，另有 1 个 `compile_fail` 文档用例。

## Known limitations

- v1 的正确性依赖 POSIX `rename` 与目录 `fsync`；Windows 无目录 `fsync` 等价语义，崩溃恢复退化为「可能丢失最后一次提交」。
- 7 个交叉编译目标的工具链尚未验证；本机只有宿主默认目标 `x86_64-unknown-linux-gnu`（Rust stable 1.99.1 已装，`~/.cargo/bin` 在 PATH）。
- 旧存储迁移不在 v1.0 范围；旧 `repo/` 不被读取也不被删除。
- `edit` 通过 `sh -c` 启动编辑器，Windows 语义属 M3。
- 复审剩余：`commit`/`prune`/`rename` 的写锁仍是调用点约定；Windows/macOS 大小写路径键未在 runner 验证；实际裁剪仍需全局引用扫描。`..` 路径、类型化写回、`codec` 分派与 CLI 命令已修复。
- 全量审计新增限制：只读存储因 `.lock` 以读写方式打开而无法读取；Windows 全量测试未跑通（测试夹具依赖 `sh` 与 `/tmp` 断言）；初始化标记临时文件只忽略不删除，根目录临时文件仍无自动清理（A-15）。
- 人工验收流程见 [[OPS__manual-testing]]；`README.md`/`README.en.md` 只保留介绍与使用说明。仓库与本地目录已改为小写 `bakon`，`v1.0.1` 已打 tag 并完成 Release；`edit --editor` 与详细使用说明已包含在发布中。
- 端到端 1 MB 文本提交实测约 50 ms（本机 ext4/NVMe），验收预算已调整为 60 ms；成本主要是进程启动、编辑器与 fsync。

## Important deviations

- 复审剩余项：`commit`/`prune`/`rename` 的写锁仍是调用点约定；Windows/macOS 大小写路径键未在真实 runner 验证；实际裁剪仍需全局引用扫描。
- 0.x 的 `docs/design.md` 与 `docs/new_design.md` 已删除，旧审计只作为历史证据保留；当前设计以 `design/` 为准。

# Open questions for the human

> Only questions that block work or affect safety, compatibility or lasting decisions. Write a blocking question the moment it arises. When resolved: write the lasting decision in its authoritative document, remove task-board references, then remove the question here.
> IDs are `Q-<topic>`, unique within the project, stable once created, not numbered.

（无未决问题。2026-11-10 的四项裁决已写入 [[ADR__replace-git-with-own-content-addressed-store]]、[[ADR__rewrite-in-rust]] 与 [[MODULE__bakon-v1-design]]。）

## Q-v1-perf-budget

设计预算为 1 MB 提交 5 ms（SSD），实测端到端约 50 ms；固定成本来自进程启动、编辑器与两次 fsync（本机单次 fsync 约 3 ms）。默认：接受现状，把验收预算改为端到端 60 ms 并在 v1.0 前复测；若要求 5 ms，需要把提交做成常驻进程或放宽 fsync 策略。

## Q-v1-release-artifacts

用 cargo 产物替换 `scripts/build.sh`、`dist/` 与 Release 工作流需要改 CI 与发布路径，并决定 macOS/Windows 产物在哪些 runner 上构建。默认：M3 完成后再动，先保持 0.x 发布路径不变。

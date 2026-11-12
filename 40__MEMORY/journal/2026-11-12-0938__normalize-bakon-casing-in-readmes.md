# 2026-11-12-0938__normalize-bakon-casing-in-readmes

- Mode: Copilot
- Task: 统一 README 中项目名的大小写，命令行示例除外
- Did: `README.md` 与 `README.en.md` 的正文中 `Bakon` 全部改为 `bakon`；命令、路径与环境变量示例保持不变
- Verified: `rg -n "Bakon" README.md README.en.md` 无匹配；`git diff --check` 通过；纯文档改动，未跑无关代码测试
- Touched: 无
- Next: 无

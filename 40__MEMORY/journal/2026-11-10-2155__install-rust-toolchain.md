# 2026-11-10-2155__install-rust-toolchain

- Mode: Copilot
- Task: 安装 Rust 工具链，解除 v1 M0 的环境阻塞
- Did: 用户经官方 rustup 安装 stable 1.99.1（宿主 `x86_64-unknown-linux-gnu`）；`~/.cargo/bin` 已由 `~/.zshenv`、`~/.profile`、`~/.bashrc` 注入 PATH；`rustc`/`cargo`/`rustup`/`clippy`/`rustfmt`/`rust-analyzer` 可用
- Verified: `rustc --version` 1.99.1、`cargo --version` 1.99.1、`rustup show` 仅宿主目标；`cargo new` + `cargo build` + 运行 + `cargo clippy` + `cargo fmt --check` 全部通过（`cc` 13.3.0、`ld` 2.42 在位）
- Touched: [[project_state_now]]
- Next: v1 M0（Rust 工作区与 crate 骨架）；7 个交叉编译目标仍未安装，属 M3

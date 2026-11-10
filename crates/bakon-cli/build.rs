//! 把目标三元组编译进二进制，供 `bakon version` 输出。
fn main() {
    let target = std::env::var("TARGET").expect("cargo sets TARGET for build scripts");
    println!("cargo:rustc-env=BAKON_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}

//! 构建钩子：注入产品版本环境变量 + 调 tauri_build。
//!
//! workspace `version` 是 0.1.0（crate 内部），用户可见的 Desktop 版本在
//! `tauri.conf.json` / `package.json`。自更新与 DesktopSelf detect 必须读
//! 产品版本，不能读 CARGO_PKG_VERSION。

fn main() {
    // 主线程栈：Windows 下由 PE 头的 /STACK 决定，默认 1 MB。启动路径（Tauri 装配 +
    // 命令分发链，generate_handler! 现在 322 条）嵌套很深——1 MB 会在启动约 3.7 秒时
    // 栈溢出，报 0xc00000fd / "thread 'main' has overflowed its stack"，窗口都来不及出现。
    // 给到 16 MB 后实测稳定（同一构建方式：之前 4/4 必崩，之后 4/4 正常）。
    //
    // 这是**绕开上游深递归**的兜底，不是根治：真因（嵌套深度）在上游，那边修掉后可以删。
    // 只对 MSVC 工具链有效；GNU 目标对应的是 -Wl,--stack。
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/STACK:16777216");
    }
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    inject_product_version();
    tauri_build::build();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // tauri-build 的资源只链接正式 bin；原生 example 也需要 Common Controls v6。
        let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("examples/chat-window-smoke.manifest");
        println!("cargo:rerun-if-changed=examples/chat-window-smoke.manifest");
        println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-examples=/MANIFESTINPUT:{}", manifest.display());
    }
}

fn inject_product_version() {
    // 取运行时的变量而不是 env!：env! 把编译 build script 那一刻的路径写死，
    // 同一个 target 目录被别的 checkout（worktree）编过时，会拿着已经不在的路径去读
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let conf_path = std::path::Path::new(&manifest_dir).join("tauri.conf.json");
    let Ok(raw) = std::fs::read_to_string(&conf_path) else {
        println!("cargo:warning=cannot read tauri.conf.json for NCD_PRODUCT_VERSION");
        return;
    };
    // 轻量解析 "version": "x.y.z"，避免给 build-dependencies 再加 serde_json
    let Some(version) = extract_json_string_field(&raw, "version") else {
        println!("cargo:warning=tauri.conf.json missing version field");
        return;
    };
    let plain = version.trim().trim_start_matches(['v', 'V']);
    println!("cargo:rustc-env=NCD_PRODUCT_VERSION={plain}");
}

fn extract_json_string_field(raw: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut rest = raw;
    while let Some(idx) = rest.find(&needle) {
        rest = &rest[idx + needle.len()..];
        let rest = rest.trim_start();
        if !rest.starts_with(':') {
            continue;
        }
        let rest = rest[1..].trim_start();
        if !rest.starts_with('"') {
            continue;
        }
        let rest = &rest[1..];
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }
    None
}

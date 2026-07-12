//! 构建钩子：注入产品版本环境变量 + 调 tauri_build。
//!
//! workspace `version` 是 0.1.0（crate 内部），用户可见的 Desktop 版本在
//! `tauri.conf.json` / `package.json`。自更新与 DesktopSelf detect 必须读
//! 产品版本，不能读 CARGO_PKG_VERSION。

fn main() {
    println!("cargo:rerun-if-changed=tauri.conf.json");
    inject_product_version();
    tauri_build::build();
}

fn inject_product_version() {
    let conf_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
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

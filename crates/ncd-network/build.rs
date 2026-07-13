//! 编译期注入中转代理常量(base url + HMAC secret)
//!
//! 读环境变量生成 src/proxy_constants.rs(.gitignore 忽略):
//! - NCD_PROXY_BASE_URL
//! - NCD_PROXY_SECRET(首选)或 NCD_PROXY_SHARED_SECRET(仓库 secrets 历史名)
//! 都缺失时拷贝 template.rs 占位,运行时 is_configured() 返 false 走 GitHub 直连
//! 变量来源:系统环境变量优先,其次 workspace 根 .env仓库 clone 拿不到真实 secret,
//! 必须用官方构建产物或本地配 .env / CI secrets

// build.rs 是构建脚本:main() 返 () 无法用 ?,panic 是中止构建的标准方式;
// env::set_var 在 Rust 2024 要求 unsafe,单线程构建期无并发风险两者语义
// 不同于运行时生产代码,整体豁免
#![allow(clippy::panic, unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;

const ENV_BASE_URL: &str = "NCD_PROXY_BASE_URL";
const ENV_SECRET: &str = "NCD_PROXY_SECRET";
/// 仓库 secrets 历史命名,与 Worker SHARED_SECRET 对齐;CI 常只配这一项
const ENV_SECRET_LEGACY: &str = "NCD_PROXY_SHARED_SECRET";

const TEMPLATE_FILE: &str = "src/proxy_constants.template.rs";
const OUT_FILE: &str = "src/proxy_constants.rs";

fn main() {
    println!("cargo:rerun-if-env-changed={ENV_BASE_URL}");
    println!("cargo:rerun-if-env-changed={ENV_SECRET}");
    println!("cargo:rerun-if-env-changed={ENV_SECRET_LEGACY}");
    println!("cargo:rerun-if-changed={TEMPLATE_FILE}");
    println!("cargo:rerun-if-changed=build.rs");

    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let out_path = crate_dir.join(OUT_FILE);

    // 尝试从 workspace 根目录的 .env 加载(不覆盖已有系统环境变量)
    // workspace root = crate_dir/../../(crates/ncd-network → 项目根)
    let workspace_root = crate_dir.parent().and_then(|p| p.parent());
    if let Some(root) = workspace_root {
        let dotenv_path = root.join(".env");
        println!("cargo:rerun-if-changed={}", dotenv_path.display());
        load_dotenv(&dotenv_path);
    }

    let base_url = env::var(ENV_BASE_URL).ok();
    // 首选 NCD_PROXY_SECRET;空则回退仓库 secrets 名 NCD_PROXY_SHARED_SECRET
    let secret = env::var(ENV_SECRET)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            env::var(ENV_SECRET_LEGACY)
                .ok()
                .filter(|s| !s.trim().is_empty())
        });

    // 两个都没注入 → 用模板占位(is_configured 返回 false,走 GitHub 直连)
    if base_url.as_deref().map(str::trim).unwrap_or("").is_empty() || secret.is_none() {
        let template = crate_dir.join(TEMPLATE_FILE);
        let content = fs::read_to_string(&template).unwrap_or_else(|err| {
            panic!(
                "ncd-network build.rs: 读取模板 {} 失败: {err}",
                template.display()
            )
        });
        write_if_changed(&out_path, &content);
        println!(
            "cargo:warning=ncd-network: proxy constants not injected (missing env); GitHub direct only"
        );
        return;
    }

    // 已注入:生成真实常量文件
    let base_url = escape_rust_str(base_url.as_deref().unwrap_or("").trim());
    let secret = escape_rust_str(secret.as_deref().unwrap_or("").trim());
    let generated = format!(
        "// 由 build.rs 在编译期生成,请勿手工编辑;改动请改 build.rs / 模板\n\
         // 真实中转代理常量已通过 NCD_PROXY_BASE_URL + SECRET 注入\n\
         \n\
         pub const PROXY_BASE_URL: &str = {base_url};\n\
         pub const PROXY_SHARED_SECRET: &str = {secret};\n"
    );
    write_if_changed(&out_path, &generated);
    println!(
        "cargo:warning=ncd-network: proxy constants injected (base_url set, secret len hidden)"
    );
}

/// 简易 .env 文件解析器逐行读 KEY=VALUE(忽略注释,空行,引号包裹),
/// 仅在对应环境变量**不存在**时才设置(系统环境变量优先)
fn load_dotenv(path: &PathBuf) {
    let Ok(content) = fs::read_to_string(path) else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        // 去掉可选的包裹引号("..." 或 '...')
        let value = strip_quotes(value);
        // 仅在系统环境变量未设置时注入
        if env::var(key).is_err() {
            // SAFETY: build.rs 是单线程执行,不存在并发读取环境变量的 UB 风险
            unsafe {
                env::set_var(key, value);
            }
        }
    }
}

fn strip_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return &s[1..s.len() - 1];
    }
    s
}

fn write_if_changed(path: &PathBuf, content: &str) {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == content {
            return;
        }
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, content)
        .unwrap_or_else(|err| panic!("ncd-network build.rs: 写入 {} 失败: {err}", path.display()));
}

/// 把任意字符串转成合法的 Rust 字面量(双引号包裹 + 转义)
fn escape_rust_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

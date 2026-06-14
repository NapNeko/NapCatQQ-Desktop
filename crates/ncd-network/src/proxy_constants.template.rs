// 编译期注入的中转代理常量 —— 模板文件（入库）。
//
// 构建时 build.rs 会根据环境变量 NCD_PROXY_BASE_URL / NCD_PROXY_SECRET 生成
// 真实的 src/proxy_constants.rs（被 .gitignore 忽略，仓库 clone 拿不到真实值）。
// 本模板仅作为「缺失环境变量时」的占位 fallback，保证开发环境可编译、运行
// 时自动降级到 GitHub 直连（见 ProxySigner::is_configured）。
//
// 修改本文件后，build.rs 仍会用环境变量覆盖生成；本文件本身不会被编译（文件名
// 不在 mod 树里）。

/// 中转代理根地址。占位为空串，表示「未注入，走 GitHub 直连」。
pub const PROXY_BASE_URL: &str = "";

/// HMAC-SHA256 签名密钥。占位串带 PLACEHOLDER 标记，运行时据此判定未注入。
pub const PROXY_SHARED_SECRET: &str = "PLACEHOLDER-NOT-INJECTED";

//! `NapCatComponent`:NapCat.Shell 注入式组件。
//!
//! 对齐 NapCat-Installer-main 官方一键脚本(install.sh L456-L770)。
//!
//! 安装流程:
//! 1. 下载 `NapCat.Shell.zip`(默认
//!    `https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip`)
//! 2. 上传到远端 `<tmp>/`
//! 3. 解压到 `<tmp>/NapCat/`(中间 staging,匹配官方 `unzip -d ./NapCat`)
//! 4. 拷贝到 `<install_base>/opt/QQ/resources/app/app_launcher/napcat/`(target_folder)
//! 5. 赋权 `chmod -R +x`
//! 6. 写 loadNapCat.js(官方 install.sh L741)
//! 7. 改 QQ package.json 的 `main` 字段为 `./loadNapCat.js`
//!
//! 探测:检查 `<install_base>/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs`
//! 是否存在。版本号从 `napcat.mjs` 内容里 grep `napCatVersion = ... "<x.y.z>"` 拿
//! (legacy 的做法,见 `legacy-python/src/core/remote/deployment.py`
//! `_NAPCAT_VERSION_PATTERN`)。

use async_trait::async_trait;

use ncd_host::{Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 默认 NapCat.Shell.zip 下载源(GitHub Release 永久 latest 链接)。
pub const DEFAULT_NAPCAT_URL: &str =
    "https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip";

/// NapCat component 配置。
#[derive(Debug, Clone)]
pub struct NapCatComponent {
    /// 安装根目录(对齐官方 `$HOME/Napcat`)。NapCat 注入到此目录下的 QQ 子树。
    pub install_base_dir: HostPath,
    /// 下载 URL(默认 GitHub latest)
    pub download_url: String,
    /// 期望 SHA256(可选,GitHub 不提供官方 SHA256,通常为 None)
    pub expected_sha256: Option<String>,
    /// 临时目录(下载/解压用)
    pub tmp_dir: HostPath,
}

impl NapCatComponent {
    /// 创建一个 NapCat component 描述。
    pub fn new(install_base_dir: HostPath) -> Self {
        Self {
            install_base_dir,
            download_url: DEFAULT_NAPCAT_URL.to_string(),
            expected_sha256: None,
            tmp_dir: HostPath::from_posix("/tmp"),
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.download_url = url.into();
        self
    }

    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.expected_sha256 = Some(sha256.into());
        self
    }

    pub fn with_tmp_dir(mut self, tmp: HostPath) -> Self {
        self.tmp_dir = tmp;
        self
    }

    // ===== 路径 helpers(对齐官方 install.sh 路径常量)=====

    /// QQ_BASE_PATH = `$INSTALL_BASE_DIR/opt/QQ`
    fn qq_base_path(&self) -> HostPath {
        self.install_base_dir.join("opt/QQ")
    }

    /// TARGET_FOLDER = `$QQ_BASE_PATH/resources/app/app_launcher`
    fn target_folder(&self) -> HostPath {
        self.qq_base_path().join("resources/app/app_launcher")
    }

    /// NapCat 注入的根目录:`$TARGET_FOLDER/napcat/`
    fn napcat_dir(&self) -> HostPath {
        self.target_folder().join("napcat")
    }

    /// NapCat 入口文件:`$TARGET_FOLDER/napcat/napcat.mjs`
    fn napcat_mjs(&self) -> HostPath {
        self.napcat_dir().join("napcat.mjs")
    }

    /// loadNapCat.js 路径(官方 install.sh L741)
    fn load_script_path(&self) -> HostPath {
        self.qq_base_path().join("resources/app/loadNapCat.js")
    }

    /// QQ package.json 路径(官方 L21)
    fn qq_package_json(&self) -> HostPath {
        self.qq_base_path().join("resources/app/package.json")
    }
}


/// 从 `napcat.mjs` 内容 grep 版本号字符串。
///
/// 真实 napcat.mjs 中的形态(legacy `_NAPCAT_VERSION_PATTERN` 实测验证):
/// ```text
/// const napCatVersion = typeof (__vite_import_meta_env__) !== "undefined" && "4.18.1" || "1.0.0-dev";
/// ```
///
/// 关键点:等号到目标版本之间隔了 `"undefined"` 字符串字面量,必须用非贪婪匹配。
/// 使用纯字符串扫描(不依赖 regex crate),避免引入新依赖。
fn parse_napcat_version(content: &str) -> Option<String> {
    // 找到 `napCatVersion` 关键字
    let key_idx = content.find("napCatVersion")?;
    let after_key = &content[key_idx + "napCatVersion".len()..];

    // 限制扫描范围在 200 字符内,防止 mjs 文件混入其他 napCatVersion 字面量
    let scan = &after_key[..after_key.len().min(500)];

    // 找到第一个 `=` 之后第一个不是 `=`(为了跳过 `===` / `==`)
    let eq_idx = scan.find('=')?;
    let mut after_eq = &scan[eq_idx + 1..];
    while after_eq.starts_with('=') {
        after_eq = &after_eq[1..];
    }

    // 现在 after_eq 类似 ` typeof (__vite_import_meta_env__) !== "undefined" && "4.18.1" || ...`
    // 跳过第一个 `"undefined"` 字面量(如果存在),拿下一个字面量
    // 简化:扫所有 "x.y.z" 形式的字面量,取第一个看起来是版本号的
    let mut search = after_eq;
    while let Some(start) = search.find('"') {
        let after_quote = &search[start + 1..];
        let end = after_quote.find('"')?;
        let candidate = &after_quote[..end];
        // 排除 "undefined" / "object" / 空串等,要求形如 X.Y.Z[-+suffix]
        if candidate.len() >= 5
            && candidate.chars().next().is_some_and(|c| c.is_ascii_digit())
            && candidate.contains('.')
            && !candidate.starts_with("0.0.")  // dev fallback
        {
            return Some(candidate.to_string());
        }
        search = &after_quote[end + 1..];
    }
    None
}

#[async_trait]
impl Component for NapCatComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NapCat
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        // 当前实装范围:NapCat 注入 LinuxQQ,只在 Linux 上有意义。
        // Windows 版 NapCat 通过 NapCatWinBootMain.exe 走完全不同的注入路径,
        // 由 ncd-backend-napcat 自己处理,不走本 component。
        &[
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let mjs = self.napcat_mjs();
        if !host.exists(&mjs).await? {
            return Ok(None);
        }

        let bytes = match host.read_file(&mjs).await {
            Ok(b) => b,
            Err(HostError::PathNotFound { .. }) => return Ok(None),
            Err(e) => return Err(ActionError::Host(e)),
        };
        let content = std::str::from_utf8(&bytes).map_err(|e| {
            ActionError::detect_failed("napcat", format!("napcat.mjs not utf-8: {e}"))
        })?;

        match parse_napcat_version(content) {
            Some(v) => Ok(Some(DetectedVersion {
                version: v,
                source: format!("{mjs}"),
            })),
            None => Ok(Some(DetectedVersion {
                // mjs 存在但没解析出版本,标 unknown 让上层知道"装了但没版本号"
                version: "unknown".to_string(),
                source: format!("{mjs} (version pattern not matched)"),
            })),
        }
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.install_inner(host, ctx).await
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let mjs = self.napcat_mjs();
        let load_js = self.load_script_path();
        let mjs_exists = host.exists(&mjs).await?;
        let load_exists = host.exists(&load_js).await?;

        let mut report = VerifyReport::ok()
            .with_check("napcat.mjs exists", mjs_exists, Some(format!("{mjs}")))
            .with_check(
                "loadNapCat.js exists",
                load_exists,
                Some(format!("{load_js}")),
            );

        // 检查 QQ package.json 的 main 字段是否被 patch
        let pkg_json = self.qq_package_json();
        if host.exists(&pkg_json).await? {
            if let Ok(bytes) = host.read_file(&pkg_json).await {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    let main = v.get("main").and_then(|m| m.as_str()).unwrap_or("");
                    let patched = main == "./loadNapCat.js";
                    report = report.with_check(
                        "QQ package.json main patched",
                        patched,
                        Some(format!("main={main}")),
                    );
                }
            }
        }

        if let Ok(Some(v)) = self.detect(host).await {
            report = report.with_check(
                "napcat version detected",
                v.version != "unknown",
                Some(format!("version={}", v.version)),
            );
        }
        Ok(report)
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // NapCat 通过 LinuxQQ 启动:`<install_base>/opt/QQ/qq <extra_args>`
        // backend 一般会再加 `--no-sandbox -q <qqid>` 等参数,这里只给基础命令
        let mut cmd = HostCommand::new(self.qq_base_path().join("qq").as_posix());
        for a in &args.extra_args {
            cmd = cmd.arg(a);
        }
        for (k, v) in &args.extra_env {
            cmd = cmd.env(k, v);
        }
        if let Some(wd) = &args.working_dir {
            cmd = cmd.working_dir(wd.clone());
        }
        Ok(cmd)
    }
}


// ============================================================
// install 实装(独立 impl block,复用上面 trait 的字段)
// ============================================================

impl NapCatComponent {
    /// 完整 install 流程,被 Component::install 调用。拆出来方便阅读。
    async fn install_inner(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 6 }).await;

        // ===== Step 1:下载 NapCat.Shell.zip 到本地 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "download NapCat.Shell.zip".into(),
        })
        .await;
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-napcat-{}-{}.zip",
            std::process::id(),
            chrono_ms()
        ));
        let helper = DownloadHelper::new()?;
        helper
            .download_to_file(
                &self.download_url,
                &local_tmp,
                self.expected_sha256.as_deref(),
                ctx,
                1,
            )
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // ===== Step 2:上传到远端 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "upload to host".into(),
        })
        .await;
        host.create_dir_all(&self.tmp_dir).await?;
        let remote_zip = self.tmp_dir.join(format!(
            "ncd-napcat-{}.zip",
            std::process::id()
        ));
        host.upload(&local_tmp, &remote_zip).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // ===== Step 3:解压到 staging =====
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "extract zip to staging".into(),
        })
        .await;
        let stage_dir = self.tmp_dir.join(format!(
            "ncd-napcat-stage-{}",
            std::process::id()
        ));
        let _ = host.remove_dir_all(&stage_dir).await;
        host.create_dir_all(&stage_dir).await?;
        host.extract_archive(&remote_zip, &stage_dir, ncd_host::ArchiveKind::Zip)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // ===== Step 4:拷贝到 napcat_dir + chmod +x =====
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "install to target_folder/napcat".into(),
        })
        .await;
        let napcat_dir = self.napcat_dir();
        host.create_dir_all(&napcat_dir).await?;
        // 官方 install.sh L730:`cp -r -f ./NapCat/* TARGET_FOLDER/napcat/`
        // 先尝试 stage_dir 直接 copy(NapCat.Shell.zip 顶层就是 napcat 文件,无 NapCat/ 子目录)
        // 但 legacy + 官方都用 `unzip -d ./NapCat NapCat.Shell.zip` 让解压有 NapCat/ 包装
        // 我们 extract_archive 直接到 stage_dir,所以 stage_dir 下直接是 napcat.mjs 等
        let cp_cmd = HostCommand::new("sh").arg("-c").arg(format!(
            "cp -r -f {}/* {}/ && chmod -R +x {}/",
            stage_dir.as_posix(),
            napcat_dir.as_posix(),
            napcat_dir.as_posix(),
        ));
        let out = host.run_to_string(cp_cmd).await?;
        if !out.success() {
            return Err(ActionError::install_step(
                "cp_napcat",
                format!("exit={:?} stderr={}", out.exit_code, out.stderr.trim()),
            ));
        }
        let _ = host.remove_dir_all(&stage_dir).await;
        let _ = host.remove_file(&remote_zip).await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;

        // ===== Step 5:写 loadNapCat.js =====
        ctx.emit(ProgressKind::StepBegin {
            step: 5,
            message: "write loadNapCat.js".into(),
        })
        .await;
        let load_script = format!(
            "(async () => {{await import('file://{}/napcat.mjs');}})();\n",
            napcat_dir.as_posix()
        );
        host.write_file(&self.load_script_path(), load_script.as_bytes())
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 5, ok: true }).await;

        // ===== Step 6:改 QQ package.json 的 main 字段 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 6,
            message: "patch QQ package.json main field".into(),
        })
        .await;
        self.patch_qq_main(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 6, ok: true }).await;

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 把 QQ 的 `package.json` 中 `main` 字段改成 `./loadNapCat.js`。
    /// 使用纯文本读 + serde_json 改 + 写回(不依赖远端 jq,提升跨发行版兼容性)。
    async fn patch_qq_main(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.qq_package_json();
        let bytes = host.read_file(&path).await?;
        let mut json: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| {
                ActionError::install_step(
                    "parse_qq_package_json",
                    format!("{e}"),
                )
            })?;
        if let Some(obj) = json.as_object_mut() {
            obj.insert(
                "main".to_string(),
                serde_json::Value::String("./loadNapCat.js".to_string()),
            );
        } else {
            return Err(ActionError::install_step(
                "patch_qq_main",
                "package.json root is not an object",
            ));
        }
        let new_bytes = serde_json::to_vec_pretty(&json).map_err(|e| {
            ActionError::install_step("serialize_qq_package_json", format!("{e}"))
        })?;
        host.write_file(&path, &new_bytes).await?;
        Ok(())
    }
}

/// 当前 unix 毫秒(用于临时文件名),失败返回 0。
fn chrono_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn comp() -> NapCatComponent {
        NapCatComponent::new(HostPath::from_posix("/home/test/Napcat"))
    }

    #[test]
    fn paths_align_with_official_install_layout() {
        let c = comp();
        // 官方 install.sh L13-L21
        assert_eq!(c.qq_base_path().as_posix(), "/home/test/Napcat/opt/QQ");
        assert_eq!(
            c.target_folder().as_posix(),
            "/home/test/Napcat/opt/QQ/resources/app/app_launcher"
        );
        assert_eq!(
            c.napcat_dir().as_posix(),
            "/home/test/Napcat/opt/QQ/resources/app/app_launcher/napcat"
        );
        assert_eq!(
            c.napcat_mjs().as_posix(),
            "/home/test/Napcat/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs"
        );
        assert_eq!(
            c.load_script_path().as_posix(),
            "/home/test/Napcat/opt/QQ/resources/app/loadNapCat.js"
        );
    }

    #[test]
    fn default_url_points_to_github_latest() {
        let c = comp();
        assert!(c.download_url.contains("github.com/NapNeko/NapCatQQ"));
        assert!(c.download_url.contains("NapCat.Shell.zip"));
    }

    #[test]
    fn id_returns_napcat() {
        assert_eq!(comp().id(), ComponentId::NapCat);
    }

    #[test]
    fn supported_targets_only_linux() {
        let c = comp();
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Local)));
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Remote)));
        assert!(!c.supported_targets().contains(&(Os::Windows, Locality::Local)));
    }

    #[test]
    fn parse_version_extracts_real_napcat_mjs_format() {
        // 真实 napcat.mjs 的形态(从 _NAPCAT_VERSION_PATTERN docstring 抄)
        let content = r#"const napCatVersion = typeof (__vite_import_meta_env__) !== "undefined" && "4.18.1" || "1.0.0-dev";"#;
        assert_eq!(parse_napcat_version(content), Some("4.18.1".to_string()));
    }

    #[test]
    fn parse_version_handles_minified_format() {
        // 压缩后可能没空格
        let content = r#"const napCatVersion=typeof(__vite_import_meta_env__)!=="undefined"&&"4.20.5"||"0.0.1-dev";"#;
        assert_eq!(parse_napcat_version(content), Some("4.20.5".to_string()));
    }

    #[test]
    fn parse_version_returns_none_for_missing_pattern() {
        let content = "// no napCatVersion at all";
        assert_eq!(parse_napcat_version(content), None);
    }

    #[test]
    fn parse_version_handles_prerelease_suffix() {
        let content = r#"const napCatVersion = "4.21.0-beta.3";"#;
        assert_eq!(parse_napcat_version(content), Some("4.21.0-beta.3".to_string()));
    }

    #[test]
    fn parse_version_skips_dev_fallback() {
        // 0.0.1-dev / 1.0.0-dev 不该被当版本号(legacy 验证过)
        let content = r#"const napCatVersion = typeof x !== "undefined" && "0.0.0-dev" || "real";"#;
        // 0.0. 开头被排除,继续找 "real",不是数字开头 → None
        assert_eq!(parse_napcat_version(content), None);
    }

    #[test]
    fn load_script_content_uses_correct_napcat_dir() {
        let c = NapCatComponent::new(HostPath::from_posix("/opt/foo"));
        let napcat_dir = c.napcat_dir();
        let expected = format!(
            "(async () => {{await import('file://{}/napcat.mjs');}})();\n",
            napcat_dir.as_posix()
        );
        // 验证 install_inner 里写入 loadNapCat.js 的字符串生成逻辑等价
        assert_eq!(
            expected,
            "(async () => {await import('file:///opt/foo/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs');})();\n"
        );
    }

    #[test]
    fn build_with_url_overrides_default() {
        let c = NapCatComponent::new(HostPath::from_posix("/x"))
            .with_url("https://mirror.example.com/NapCat.Shell.zip");
        assert_eq!(c.download_url, "https://mirror.example.com/NapCat.Shell.zip");
    }

    #[test]
    fn chrono_ms_returns_positive() {
        let v = chrono_ms();
        // 2026 之后总是大于 0
        assert!(v > 1_000_000_000_000);
    }
}

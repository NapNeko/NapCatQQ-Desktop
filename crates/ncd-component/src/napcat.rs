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
use ncd_network::build_mirror_urls;

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 默认 NapCat.Shell.zip 下载源(GitHub Release 永久 latest 链接)。
pub const DEFAULT_NAPCAT_URL: &str =
    "https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip";

/// NapCat 部署模式。
///
/// Linux 走"注入式":NapCat 装到 LinuxQQ 的 `resources/app/app_launcher/napcat/`,
/// 入口 mjs 嵌套在 QQ 安装根之下;还要写 loadNapCat.js + patch package.json。
///
/// Windows 走"扁平 zip 解压":安装目录是 `<data_root>/napcat/`,napcat.mjs 直接落
/// 在根下,不存在嵌套结构。配合 NapCatWinBootMain.exe 注入(注入器是 backend 关心
/// 的事,不在本 Component 边界)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformMode {
    Linux,
    Windows,
}

/// NapCat component 配置。
#[derive(Debug, Clone)]
pub struct NapCatComponent {
    /// 安装根目录。
    ///
    /// Linux 模式:对齐官方 `$HOME/Napcat`,NapCat 注入到此目录下的 QQ 子树。
    /// Windows 模式:扁平 zip 解压根,典型为 `<data_root>/napcat/`。
    pub install_base_dir: HostPath,
    /// 下载 URL(默认 GitHub latest)
    pub download_url: String,
    /// 期望 SHA256(可选,GitHub 不提供官方 SHA256,通常为 None)
    pub expected_sha256: Option<String>,
    /// 临时目录(下载/解压用)
    pub tmp_dir: HostPath,
    /// 平台模式。Linux 走注入式,Windows 走扁平 zip 解压。
    mode: PlatformMode,
}

impl NapCatComponent {
    /// 创建一个 Linux 注入式 NapCat component(对齐官方 install.sh)。
    ///
    /// `install_base_dir` 是 LinuxQQ 安装根(典型 `$HOME/Napcat`),NapCat
    /// 会注入到 `<install_base_dir>/opt/QQ/resources/app/app_launcher/napcat/`。
    pub fn new(install_base_dir: HostPath) -> Self {
        Self {
            install_base_dir,
            download_url: DEFAULT_NAPCAT_URL.to_string(),
            expected_sha256: None,
            tmp_dir: HostPath::from_posix("/tmp"),
            mode: PlatformMode::Linux,
        }
    }

    /// 创建一个 Windows 扁平 zip 部署的 NapCat component。
    ///
    /// `install_dir` 是扁平解压根(典型 `<data_root>/napcat/`),napcat.mjs
    /// 直接落在该目录之下。Windows 没有 LinuxQQ 注入这层语义,所以 install
    /// 不写 loadNapCat.js / 不 patch QQ package.json,只做"下载 → 清旧 →
    /// 解压"三步。
    ///
    /// 临时目录默认 `<install_dir>/_tmp`,与 legacy
    /// `PathFunc.tmp_path = runtime_path/tmp` 同源(legacy 在 ProgramData
    /// 下也是与 napcat_path 同级 runtime/ 的子目录)。
    pub fn for_windows(install_dir: HostPath) -> Self {
        let tmp_dir = install_dir.join("_tmp");
        Self {
            install_base_dir: install_dir,
            download_url: DEFAULT_NAPCAT_URL.to_string(),
            expected_sha256: None,
            tmp_dir,
            mode: PlatformMode::Windows,
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

    // Windows 扁平模式下,napcat.mjs 直接落在 install_base_dir 根下;
    // 没有 opt/QQ/... 嵌套层级。

    /// Windows 模式入口文件:`<install_base_dir>/napcat.mjs`
    fn windows_napcat_mjs(&self) -> HostPath {
        self.install_base_dir.join("napcat.mjs")
    }

    /// 当前模式下 napcat.mjs 的实际位置。
    fn napcat_mjs_for_mode(&self) -> HostPath {
        match self.mode {
            PlatformMode::Linux => self.napcat_mjs(),
            PlatformMode::Windows => self.windows_napcat_mjs(),
        }
    }

    /// 组件元数据，给 `list_components` Tauri command 使用。
    ///
    /// `supported_targets` 必须与 `Component::supported_targets` 返回值一致；
    /// 单测里有断言锁定。Windows 走扁平 zip 解压（与 legacy NapCatInstall
    /// 同款），Linux 走 NapCat 注入 LinuxQQ resources/app 的官方一键脚本路径。
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::NapCat,
            display_name: "NapCat".to_string(),
            description: "Hook QQ 实现的 OneBot 11 协议端，运行时关闭客户端窗口".to_string(),
            repo_url: Some("https://github.com/NapNeko/NapCatQQ".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Windows, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
            ],
            category: crate::types::ComponentCategory::Framework,
        }
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
pub fn parse_napcat_version(content: &str) -> Option<String> {
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
        // Linux 注入式(local + remote)+ Windows 本机扁平 zip 部署。
        // Windows remote 由 backend 自己的 SSH 注入逻辑处理,不走本 component。
        &[
            (Os::Windows, Locality::Local),
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let mjs = self.napcat_mjs_for_mode();
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
        match host.os() {
            Os::Windows => self.install_windows(host, ctx).await,
            _ => self.install_inner(host, ctx).await,
        }
    }

    async fn uninstall(
        &self,
        host: &dyn Host,
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        // 只在 Windows 模式上实装 uninstall(对齐 legacy NapCatInstall::remove_old_file
        // 的语义:删 install_base_dir 下除 config/ log/ 外所有文件)。Linux 注入式
        // 安装会污染 LinuxQQ 自身,uninstall 是 LinuxQQ 卸载的事,不在 NapCat 边界。
        match host.os() {
            Os::Windows => self.uninstall_windows(host).await,
            _ => Err(ActionError::other(
                "NapCat uninstall is only implemented for Windows local; Linux 注入式 uninstall 由 LinuxQQ 卸载承担"
            )),
        }
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        match host.os() {
            Os::Windows => self.verify_windows(host).await,
            _ => self.verify_linux(host).await,
        }
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
    /// Linux verify(原 verify 实装,挪到独立方法以便 trait verify 按 mode 分发)。
    async fn verify_linux(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
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

    /// Windows verify:扁平 zip 部署只校验 napcat.mjs 是否存在 + 版本是否解析到。
    async fn verify_windows(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let mjs = self.windows_napcat_mjs();
        let mjs_exists = host.exists(&mjs).await?;
        let mut report = VerifyReport::ok().with_check(
            "napcat.mjs exists",
            mjs_exists,
            Some(format!("{mjs}")),
        );
        if let Ok(Some(v)) = self.detect(host).await {
            report = report.with_check(
                "napcat version detected",
                v.version != "unknown",
                Some(format!("version={}", v.version)),
            );
        }
        Ok(report)
    }

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
        let mirrors = build_mirror_urls(&self.download_url, None);
        helper
            .download_with_mirrors(
                &mirrors,
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

    /// Windows 扁平 zip 部署。对齐 legacy `NapCatInstall`(installers.py):
    /// 1) 下载 NapCat.Shell.zip 到本地临时目录
    /// 2) 上传到目标 host 的 tmp_dir(本地等同 copy)
    /// 3) `remove_old_file`:删 install_base_dir 下除 config/ log/ 外所有
    ///    内容(避免新版残留旧文件,但保留用户配置和日志)
    /// 4) extract_archive 直接解压到 install_base_dir(无 strip-components)
    /// 5) 删临时 zip
    ///
    /// 与 Linux 路径的区别:不写 loadNapCat.js,不 patch QQ package.json,
    /// 不 chmod。Windows NapCat 由 NapCatWinBootMain.exe 注入,启动注入是
    /// backend 的事,本 component 只负责"把 zip 摊到目录里"。
    async fn install_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        // ===== Step 1:下载 zip 到本地 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "download NapCat.Shell.zip".into(),
        })
        .await;
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-napcat-win-{}-{}.zip",
            std::process::id(),
            chrono_ms()
        ));
        let helper = DownloadHelper::new()?;
        let mirrors = build_mirror_urls(&self.download_url, None);
        helper
            .download_with_mirrors(
                &mirrors,
                &local_tmp,
                self.expected_sha256.as_deref(),
                ctx,
                1,
            )
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // ===== Step 2:上传(本机即 copy)到 install_base_dir 旁的 tmp =====
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "stage zip on host".into(),
        })
        .await;
        host.create_dir_all(&self.tmp_dir).await?;
        let remote_zip = self.tmp_dir.join(format!(
            "ncd-napcat-win-{}.zip",
            std::process::id()
        ));
        host.upload(&local_tmp, &remote_zip).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // ===== Step 3:清旧文件,保留 config/ log/ =====
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "remove old files (preserve config/ log/)".into(),
        })
        .await;
        host.create_dir_all(&self.install_base_dir).await?;
        self.remove_old_files_windows(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // ===== Step 4:解压到 install_base_dir(扁平) =====
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "extract zip".into(),
        })
        .await;
        host.extract_archive(&remote_zip, &self.install_base_dir, ncd_host::ArchiveKind::Zip)
            .await?;
        let _ = host.remove_file(&remote_zip).await;
        // tmp_dir 自身也清掉(legacy 没保留),失败忽略
        let _ = host.remove_dir_all(&self.tmp_dir).await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 对齐 legacy `NapCatInstall.remove_old_file`:遍历 install_base_dir,
    /// 子目录里只放过 `config` 和 `log` 的保留(用户运行期改的配置 / 日志),
    /// 其余文件和子目录全删。tmp_dir 名单保留(本次 install 流程刚把 zip
    /// 落在那里,如果它就是 install_base_dir 下的 _tmp 子目录)。
    async fn remove_old_files_windows(&self, host: &dyn Host) -> Result<(), ActionError> {
        let entries = match host.list_dir(&self.install_base_dir).await {
            Ok(es) => es,
            // 目录不存在等同于"已清空",直接返回(install_windows 之前已 create_dir_all)
            Err(HostError::PathNotFound { .. }) => return Ok(()),
            Err(e) => return Err(ActionError::Host(e)),
        };

        // tmp_dir 的最后一段(file_name)用来在 list_dir 结果里识别该子目录,
        // 防止误删 install 流程暂存的 zip。
        let tmp_keep = self
            .tmp_dir
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_default();

        for entry in entries {
            // 保留:config / log(legacy 同款)+ 本次 install 临时目录
            if entry.is_dir
                && (entry.name == "config" || entry.name == "log" || entry.name == tmp_keep)
            {
                continue;
            }
            let target = self.install_base_dir.join(&entry.name);
            let result = if entry.is_dir {
                host.remove_dir_all(&target).await
            } else {
                host.remove_file(&target).await
            };
            if let Err(HostError::PathNotFound { .. }) = result {
                continue;
            }
            result.map_err(ActionError::Host)?;
        }
        Ok(())
    }

    /// Windows uninstall:对齐 legacy 行为,删 install_base_dir 下除 config/
    /// log/ 外的所有文件和目录。不删 install_base_dir 自身,允许下次 install
    /// 复用同一目录。
    async fn uninstall_windows(&self, host: &dyn Host) -> Result<(), ActionError> {
        if !host.exists(&self.install_base_dir).await? {
            return Ok(());
        }
        self.remove_old_files_windows(host).await
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
        assert!(c.supported_targets().contains(&(Os::Windows, Locality::Local)));
    }

    #[test]
    fn windows_constructor_uses_flat_install_base() {
        let c = NapCatComponent::for_windows(HostPath::from_windows(
            r"C:\ProgramData\NapCatQQ Desktop\runtime\NapCatQQ",
        ));
        assert!(matches!(c.mode, PlatformMode::Windows));
        // 扁平模式下 napcat.mjs 直接落在 install_base_dir 根下,
        // 不走 opt/QQ/resources/app/app_launcher/napcat 嵌套。
        assert_eq!(
            c.napcat_mjs_for_mode().render(ncd_host::PathStyle::Windows),
            r"C:\ProgramData\NapCatQQ Desktop\runtime\NapCatQQ\napcat.mjs"
        );
        // tmp_dir 默认在 install_dir 下子目录,不污染 /tmp。
        assert!(c.tmp_dir.as_posix().ends_with("/_tmp"));
    }

    #[test]
    fn linux_constructor_keeps_injection_layout() {
        // 回归:Linux 默认走注入式,napcat_mjs_for_mode 必须命中
        // opt/QQ/resources/app/app_launcher/napcat/napcat.mjs。
        let c = NapCatComponent::new(HostPath::from_posix("/home/test/Napcat"));
        assert!(matches!(c.mode, PlatformMode::Linux));
        assert_eq!(
            c.napcat_mjs_for_mode().as_posix(),
            "/home/test/Napcat/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs"
        );
    }

    #[test]
    fn info_lists_windows_local_in_supported_targets() {
        // ComponentInfo 里 supported_targets 必须与 trait 同步,Components 页
        // 才能在 Windows 上把 NapCat 卡显示为"支持当前平台"。
        let info = NapCatComponent::info();
        assert!(info.supported_targets.iter().any(|t| {
            t.os == Os::Windows && t.locality == Locality::Local
        }));
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

    // ============================================================
    // Windows 本机端到端测试(只在 Windows 上编译)
    //
    // 用真 LocalWindowsHost + tempdir 模拟"用户已装 NapCat / 未装 / 残留旧
    // 文件 / 保留 config|log"四种场景。不涉及网络下载;install 本身的
    // 端到端在真机 tauri:dev 跑。
    // ============================================================

    #[cfg(windows)]
    mod windows_e2e {
        use super::*;
        use ncd_host::local::LocalWindowsHost;
        use ncd_host::PathStyle;

        fn windows_path(workspace: &tempfile::TempDir, sub: &str) -> HostPath {
            let full = workspace.path().join(sub);
            HostPath::from_windows(full.to_str().unwrap())
        }

        async fn write_file(host: &LocalWindowsHost, path: &HostPath, body: &[u8]) {
            host.write_file(path, body).await.expect("write_file");
        }

        #[tokio::test]
        async fn detect_windows_returns_none_when_mjs_missing() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "napcat");
            let comp = NapCatComponent::for_windows(install);

            let detected = comp.detect(&host).await.expect("detect");
            assert!(detected.is_none(), "未装 NapCat 时 detect 必须返回 None");
        }

        #[tokio::test]
        async fn detect_windows_finds_mjs_at_install_base() {
            // 模拟 legacy zip 解压后的扁平结构:napcat.mjs 直接落在 install_base
            // 根下,版本号通过 parse_napcat_version 命中。
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "napcat");
            host.create_dir_all(&install).await.unwrap();
            let mjs = install.join("napcat.mjs");
            let body = br#"const napCatVersion = typeof (__vite_import_meta_env__) !== "undefined" && "4.18.1" || "1.0.0-dev";"#;
            write_file(&host, &mjs, body).await;

            let comp = NapCatComponent::for_windows(install.clone());
            let detected = comp.detect(&host).await.expect("detect");
            let v = detected.expect("应当解析到版本号");
            assert_eq!(v.version, "4.18.1");
            // source 字段必须指向真实文件路径,UI 调试时能定位。
            assert!(v.source.contains("napcat.mjs"));
        }

        #[tokio::test]
        async fn check_target_accepts_windows_local() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let comp = NapCatComponent::for_windows(windows_path(&ws, "napcat"));
            assert!(comp.check_target(&host).is_ok());
        }

        #[tokio::test]
        async fn uninstall_windows_preserves_config_and_log() {
            // 装好 NapCat 后用户在 config/ 和 log/ 下放了运行期文件;uninstall
            // 必须把根下其他文件清掉,但 config/log 保持原样。
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "napcat");
            host.create_dir_all(&install).await.unwrap();

            // 制造旧装产物:napcat.mjs + 一些子目录
            write_file(&host, &install.join("napcat.mjs"), b"old").await;
            write_file(&host, &install.join("README.md"), b"old").await;
            write_file(&host, &install.join("native/some.dll"), b"old").await;
            // 用户运行期数据
            write_file(&host, &install.join("config/onebot.json"), b"user").await;
            write_file(&host, &install.join("log/2026.log"), b"line").await;

            let comp = NapCatComponent::for_windows(install.clone());
            let (mut ctx, _rx) = ActionCtx::new();
            comp.uninstall(&host, &mut ctx).await.expect("uninstall");

            // 旧 mjs / dll / readme 已被清理
            assert!(!host.exists(&install.join("napcat.mjs")).await.unwrap());
            assert!(!host.exists(&install.join("README.md")).await.unwrap());
            assert!(!host.exists(&install.join("native")).await.unwrap());
            // 用户配置 / 日志原样保留
            let cfg = host
                .read_file(&install.join("config/onebot.json"))
                .await
                .unwrap();
            assert_eq!(cfg.as_ref(), b"user");
            let log = host.read_file(&install.join("log/2026.log")).await.unwrap();
            assert_eq!(log.as_ref(), b"line");
        }

        #[tokio::test]
        async fn uninstall_windows_is_noop_when_install_dir_missing() {
            // install_base_dir 不存在时,uninstall 必须直接成功(legacy 同款,
            // 用户从未装过的状态下也不应抛错)。
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "never-installed");

            let comp = NapCatComponent::for_windows(install);
            let (mut ctx, _rx) = ActionCtx::new();
            comp.uninstall(&host, &mut ctx)
                .await
                .expect("uninstall on missing dir 必须成功");
        }

        #[tokio::test]
        async fn verify_windows_reports_mjs_and_version() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "napcat");
            host.create_dir_all(&install).await.unwrap();
            write_file(
                &host,
                &install.join("napcat.mjs"),
                br#"const napCatVersion = "4.18.1";"#,
            )
            .await;

            let comp = NapCatComponent::for_windows(install);
            let report = comp.verify(&host).await.expect("verify");
            assert!(report.ok);
            // 应当包含两条 check:mjs exists + version detected
            assert!(report.checks.iter().any(|c| c.name.contains("napcat.mjs")));
            assert!(
                report
                    .checks
                    .iter()
                    .any(|c| c.name.contains("version") && c.passed)
            );
        }

        #[tokio::test]
        async fn windows_path_renders_back_to_real_filesystem() {
            // 防回归:HostPath::from_windows 之后 render(Windows) 必须还原成
            // C:\... 形式,LocalWindowsHost 才能落到正确文件系统位置。
            let p = HostPath::from_windows(r"C:\ProgramData\NapCatQQ Desktop\runtime\NapCatQQ");
            assert_eq!(
                p.render(PathStyle::Windows),
                r"C:\ProgramData\NapCatQQ Desktop\runtime\NapCatQQ"
            );
        }
    }
}

//! `QQComponent`：QQ runtime 组件。
//!
//! 跨平台：
//! - Linux 本地/远端：rootless 安装，对齐 NapCat-Installer 官方一键脚本。
//!   下载 `linuxqq_<ver>_<arch>.{deb,rpm}` 解压到 `<install_base_dir>/opt/QQ/`。
//! - Windows 本地：detect 走注册表
//!   `HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT\Install` 拿安装根。新版 QQNT
//!   按版本分目录，版本号在 `versions/config.json` 的 `curVersion`；旧版是
//!   扁平的 `resources/app/package.json`。安装走官方 pcConfig.json 拿 NSIS
//!   安装包跑 `installer.exe /s` 静默安装。
//!
//! Linux 安装路径(rootless):
//! - `$INSTALL_BASE_DIR/opt/QQ/`：QQ 解压根
//! - `$INSTALL_BASE_DIR/opt/QQ/qq`：QQ 可执行
//! - `$INSTALL_BASE_DIR/opt/QQ/resources/app/package.json`：版本探测点
//!
//! 版本号说明：腾讯 Linux QQ 没有 "latest" 端点，版本号 + hash segment 都是
//! 硬编码，改版时手动同步。当前(2026-05)锁定 `3.2.25-45758`(hash `7516007c`)。
//!
//! Windows 版本号通过 pcConfig.json 实时拉取，不固化。
//!
//! Linux 安装流程(rootless):
//! 1. 探测 dpkg-deb 或 rpm2cpio 哪个可用(用 `which` 或 `command -v`)
//! 2. 下载对应 deb/rpm 包 → 上传到远端 `<tmp>/`
//! 3. dpkg-deb -x 或 rpm2cpio | cpio -idm 解压到 `<install_base_dir>`
//! 4. 删除安装包,清理临时文件
//!
//! Windows 安装流程：
//! 1. HTTP GET `https://cdn-go.cn/qq-web/im.qq.com_new/latest/rainbow/pcConfig.json`
//! 2. 取 `Windows.ntDownloadX64Url` → 下载 NSIS 安装包到本地临时目录
//! 3. 跑 `installer.exe /s` 静默安装，等待退出码 0
//! 4. 删除本地临时安装包

use async_trait::async_trait;

use ncd_host::{Arch, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 腾讯 QQ Windows 版实时版本 / 下载地址清单(legacy `Urls.QQ_Version`)。
const QQ_PCCONFIG_URL: &str =
    "https://cdn-go.cn/qq-web/im.qq.com_new/latest/rainbow/pcConfig.json";

/// Windows QQNT 安装信息所在注册表子键(legacy `PathFunc.get_qq_path`)。
const QQ_REGISTRY_SUBKEY: &str = r"SOFTWARE\WOW6432Node\Tencent\QQNT";

/// 包格式(rootless 模式只需要 dpkg / rpm 两种)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageFormat {
    Deb,
    Rpm,
}

/// QQ component 配置。
#[derive(Debug, Clone)]
pub struct QQComponent {
    /// 版本号(如 "3.2.25-45758")
    pub version: String,
    /// 腾讯 CDN URL 中的 hash 段(如 "7516007c")
    pub url_hash_segment: String,
    /// 安装根目录(对齐官方 `$HOME/Napcat`)
    pub install_base_dir: HostPath,
    /// 期望 SHA256(可选,腾讯不提供官方 SHA256,通常为 None)
    pub expected_sha256: Option<String>,
    /// 临时目录
    pub tmp_dir: HostPath,
}

impl QQComponent {
    /// 创建一个 QQ component 描述(自定义所有字段)。
    pub fn new(
        version: impl Into<String>,
        url_hash_segment: impl Into<String>,
        install_base_dir: HostPath,
    ) -> Self {
        Self {
            version: version.into(),
            url_hash_segment: url_hash_segment.into(),
            install_base_dir,
            expected_sha256: None,
            tmp_dir: HostPath::from_posix("/tmp"),
        }
    }

    /// 默认配置:锁定已知版本 v3.2.25-45758,安装到 `$HOME/Napcat`。
    /// 腾讯改版后通过更新本默认值即可,业务代码不改。
    pub fn default_v3_2_25(install_base_dir: HostPath) -> Self {
        Self::new("3.2.25-45758", "7516007c", install_base_dir)
    }

    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.expected_sha256 = Some(sha256.into());
        self
    }

    pub fn with_tmp_dir(mut self, tmp: HostPath) -> Self {
        self.tmp_dir = tmp;
        self
    }

    fn package_filename(&self, pkg: PackageFormat, arch: Arch) -> Result<String, ActionError> {
        let arch_str = match (pkg, arch) {
            (PackageFormat::Deb, Arch::X86_64) => "amd64",
            (PackageFormat::Deb, Arch::Aarch64) => "arm64",
            (PackageFormat::Rpm, Arch::X86_64) => "x86_64",
            (PackageFormat::Rpm, Arch::Aarch64) => "aarch64",
            _ => {
                return Err(ActionError::UnsupportedTarget {
                    component: "qq".into(),
                    os: Os::Linux,
                    locality: Locality::Remote,
                });
            }
        };
        let ext = match pkg {
            PackageFormat::Deb => "deb",
            PackageFormat::Rpm => "rpm",
        };
        Ok(format!(
            "linuxqq_{}_{arch_str}.{ext}",
            self.version
        ))
    }

    fn build_download_url(&self, pkg: PackageFormat, arch: Arch) -> Result<String, ActionError> {
        let filename = self.package_filename(pkg, arch)?;
        Ok(format!(
            "https://dldir1.qq.com/qqfile/qq/QQNT/{}/{filename}",
            self.url_hash_segment
        ))
    }

    /// 探测远端有 dpkg-deb 还是 rpm2cpio。dpkg 优先(deb 更普遍)。
    async fn detect_package_format(&self, host: &dyn Host) -> Result<PackageFormat, ActionError> {
        // 用 sh -c "command -v X" 探测,退出码 0 = 存在
        for (binary, fmt) in &[
            ("dpkg-deb", PackageFormat::Deb),
            ("rpm2cpio", PackageFormat::Rpm),
        ] {
            let cmd = HostCommand::new("sh")
                .arg("-c")
                .arg(format!("command -v {binary}"));
            if let Ok(out) = host.run_to_string(cmd).await {
                if out.success() && !out.stdout.trim().is_empty() {
                    if *fmt == PackageFormat::Rpm && !self.command_available(host, "cpio").await {
                        return Err(ActionError::install_step(
                            "detect_pkg_format",
                            "已找到 rpm2cpio，但缺少 cpio，无法解包 LinuxQQ rpm。请在远端执行 sudo dnf install -y rpm2cpio cpio（或 yum 同名包）后重试",
                        ));
                    }
                    return Ok(*fmt);
                }
            }
        }
        Err(ActionError::install_step(
            "detect_pkg_format",
            "远端缺少 LinuxQQ 解包工具：既没有 dpkg-deb，也没有 rpm2cpio + cpio。请先安装 dpkg 或执行 sudo dnf install -y rpm2cpio cpio 后重试",
        ))
    }

    async fn command_available(&self, host: &dyn Host, binary: &str) -> bool {
        let cmd = HostCommand::new("sh")
            .arg("-c")
            .arg(format!("command -v {binary}"));
        matches!(host.run_to_string(cmd).await, Ok(out) if out.success() && !out.stdout.trim().is_empty())
    }

    fn qq_base_path(&self) -> HostPath {
        self.install_base_dir.join("opt/QQ")
    }

    fn qq_executable(&self) -> HostPath {
        self.qq_base_path().join("qq")
    }

    fn qq_package_json(&self) -> HostPath {
        self.qq_base_path().join("resources/app/package.json")
    }

    /// 组件元数据，给 `list_components` Tauri command 使用。
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::Qq,
            display_name: "QQ".to_string(),
            description: "腾讯 QQ 客户端，NapCat / SnowLuma 注入运行所需的宿主程序".to_string(),
            repo_url: Some("https://im.qq.com/".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Windows, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
            ],
            category: crate::types::ComponentCategory::RuntimeDep,
        }
    }
}

#[async_trait]
impl Component for QQComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Qq
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        // Windows 本机 + Linux 本地 / 远端。Windows 远端由 backend 的 SSH 逻辑
        // 处理,不走本 component。
        &[
            (Os::Windows, Locality::Local),
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        match host.os() {
            Os::Windows => self.detect_windows(host).await,
            _ => self.detect_linux(host).await,
        }
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        match host.os() {
            Os::Windows => self.install_windows(host, ctx).await,
            _ => self.install_linux(host, ctx).await,
        }
    }

    async fn uninstall(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        match host.os() {
            Os::Windows => self.uninstall_windows(host, ctx).await,
            _ => self.uninstall_linux(host, ctx).await,
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
        // QQ 启动命令:`<install_base>/opt/QQ/qq <extra_args>`,backend 再拼
        // `--no-sandbox -q <qqid>` 等参数,不在 Component 这层。
        let mut cmd = HostCommand::new(self.qq_executable().as_posix());
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
// 平台分发实装：Linux rootless 解包 + Windows 官方静默安装
// ============================================================

impl QQComponent {
    /// Linux detect：读 `<install_base>/opt/QQ/resources/app/package.json`
    /// 的 version 字段。
    async fn detect_linux(
        &self,
        host: &dyn Host,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        let pkg_json = self.qq_package_json();
        if !host.exists(&pkg_json).await? {
            return Ok(None);
        }

        let bytes = match host.read_file(&pkg_json).await {
            Ok(b) => b,
            Err(HostError::PathNotFound { .. }) => return Ok(None),
            Err(e) => return Err(ActionError::Host(e)),
        };

        let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
            ActionError::detect_failed("qq", format!("parse package.json: {e}"))
        })?;

        let ver = json
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ActionError::detect_failed("qq", "missing version field in package.json")
            })?;

        Ok(Some(DetectedVersion {
            version: ver.to_string(),
            source: format!("{pkg_json}"),
        }))
    }

    /// Linux install：rootless 下载 deb/rpm → 上传 → dpkg-deb -x / rpm2cpio 解包。
    async fn install_linux(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        // ===== Step 1:探测包格式 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "detect package format".into(),
        })
        .await;
        let pkg_format = self.detect_package_format(host).await?;
        ctx.info(format!("package format: {pkg_format:?}")).await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // ===== Step 2:下载 QQ 包到本地 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "download QQ package".into(),
        })
        .await;
        let url = self.build_download_url(pkg_format, host.arch())?;
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-qq-{}-{}.{}",
            self.version,
            std::process::id(),
            match pkg_format {
                PackageFormat::Deb => "deb",
                PackageFormat::Rpm => "rpm",
            }
        ));

        let helper = DownloadHelper::new()?;
        helper
            .download_to_file(&url, &local_tmp, self.expected_sha256.as_deref(), ctx, 2)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // ===== Step 3:上传到 host =====
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "upload package to host".into(),
        })
        .await;
        host.create_dir_all(&self.tmp_dir).await?;
        let remote_pkg = self.tmp_dir.join(format!(
            "ncd-qq-{}.{}",
            std::process::id(),
            match pkg_format {
                PackageFormat::Deb => "deb",
                PackageFormat::Rpm => "rpm",
            }
        ));
        host.upload(&local_tmp, &remote_pkg).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // ===== Step 4:rootless 解压 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "extract QQ".into(),
        })
        .await;
        host.create_dir_all(&self.install_base_dir).await?;
        let install_base = self.install_base_dir.as_posix();
        let pkg_path = remote_pkg.as_posix();

        let extract_cmd = match pkg_format {
            PackageFormat::Deb => HostCommand::new("dpkg-deb")
                .arg("-x")
                .arg(pkg_path)
                .arg(install_base),
            PackageFormat::Rpm => {
                // rpm2cpio <pkg> | (cd <install_base> && cpio -idm)
                HostCommand::new("sh").arg("-c").arg(format!(
                    "rpm2cpio {} | (cd {} && cpio -idm)",
                    pkg_path, install_base
                ))
            }
        };
        let out = host.run_to_string(extract_cmd).await?;
        if !out.success() {
            return Err(ActionError::install_step(
                "extract_qq",
                format!(
                    "exit={:?} stderr={}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            ));
        }
        // 清理远端安装包
        let _ = host.remove_file(&remote_pkg).await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall_linux(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        // rootless 卸载：删 <install_base>/opt/QQ 整棵子树。
        // System 布局（/opt/QQ）需要 sudo，但 rootless 是当前默认布局；
        // 如果 install_base = "/" 删 /opt/QQ 会因权限失败，那时让用户用
        // 系统包管理器（dpkg -P linuxqq / apt remove linuxqq）卸载。
        let qq_root = self.qq_base_path();
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("remove {}", qq_root.as_posix()),
        })
        .await;
        if host.exists(&qq_root).await? {
            host.remove_dir_all(&qq_root).await?;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify_linux(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let qq_bin = self.qq_executable();
        let pkg_json = self.qq_package_json();
        let bin_exists = host.exists(&qq_bin).await?;
        let json_exists = host.exists(&pkg_json).await?;

        let mut report = VerifyReport::ok()
            .with_check("qq executable exists", bin_exists, Some(format!("{qq_bin}")))
            .with_check(
                "package.json exists",
                json_exists,
                Some(format!("{pkg_json}")),
            );

        if json_exists {
            match self.detect(host).await {
                Ok(Some(v)) => {
                    let matches = v.version == self.version;
                    report = report.with_check(
                        "version matches",
                        matches,
                        Some(format!("expected={} actual={}", self.version, v.version)),
                    );
                }
                Ok(None) => {
                    report = report.with_check(
                        "version detect",
                        false,
                        Some("detect returned None despite package.json existing".into()),
                    );
                }
                Err(e) => {
                    report = report.with_check(
                        "version detect",
                        false,
                        Some(format!("{e}")),
                    );
                }
            }
        }
        Ok(report)
    }

    // ===== Windows 本机实装 =====

    /// Windows detect：注册表 `HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT` 的
    /// `Install` 值拿安装根。新版 QQNT 把客户端按版本分目录放在
    /// `versions/<curVersion>/` 下，版本号写在 `versions/config.json` 的
    /// `curVersion`。旧版 QQ 是扁平的 `resources/app/package.json`。两种布局
    /// 都试一遍。
    async fn detect_windows(
        &self,
        host: &dyn Host,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        let install_root = match self.query_windows_install_root(host).await? {
            Some(p) => p,
            None => return Ok(None),
        };

        // 新布局优先：versions/config.json 的 curVersion 就是版本号。
        let config_json = install_root.join("versions/config.json");
        if host.exists(&config_json).await? {
            if let Ok(bytes) = host.read_file(&config_json).await {
                if let Some(ver) = parse_qqnt_cur_version(&bytes) {
                    return Ok(Some(DetectedVersion {
                        version: ver,
                        source: format!("{config_json}"),
                    }));
                }
            }
        }

        // 回退旧布局：扁平 resources/app/package.json。
        let pkg_json = install_root.join("resources/app/package.json");
        if host.exists(&pkg_json).await? {
            if let Ok(bytes) = host.read_file(&pkg_json).await {
                if let Some(ver) = parse_qq_package_version(&bytes) {
                    return Ok(Some(DetectedVersion {
                        version: ver,
                        source: format!("{pkg_json}"),
                    }));
                }
            }
        }

        // 注册表有 Install 但两种布局都没拿到版本号：QQ 装过但结构不认识，
        // 标 unknown 让 UI 显示"已安装"而不是"未安装"。
        Ok(Some(DetectedVersion {
            version: "unknown".to_string(),
            source: format!("{install_root} (no recognizable version source)"),
        }))
    }

    /// 跑 `reg query` 拿 QQNT 的 Install 值，转成 HostPath。注册表项不存在
    /// （未装 QQ）时返回 Ok(None)。
    async fn query_windows_install_root(
        &self,
        host: &dyn Host,
    ) -> Result<Option<HostPath>, ActionError> {
        let cmd = HostCommand::new("reg")
            .arg("query")
            .arg(format!(r"HKLM\{QQ_REGISTRY_SUBKEY}"))
            .arg("/v")
            .arg("Install");
        let out = match host.run_to_string(cmd).await {
            Ok(o) => o,
            // reg query 在键不存在时退出码非 0；run_to_string 不会因非 0 报
            // Err，但若 reg 自身起不来（PATH 缺失等）才会到这里，按未装处理。
            Err(_) => return Ok(None),
        };
        if !out.success() {
            return Ok(None);
        }
        match parse_reg_install_value(&out.stdout) {
            Some(path) => Ok(Some(HostPath::from_windows(&path))),
            None => Ok(None),
        }
    }

    /// Windows install：拉 pcConfig.json 拿 NSIS 安装包地址 → 下载 → 跑
    /// `installer.exe /s` 静默安装（对齐 legacy `QQInstall`）。
    async fn install_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;

        // ===== Step 1:拉 pcConfig.json 解析下载地址 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "fetch QQ pcConfig.json".into(),
        })
        .await;
        let (qq_version, download_url) = fetch_windows_qq_release().await?;
        ctx.info(format!("QQ Windows {qq_version} -> {download_url}"))
            .await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // ===== Step 2:下载 NSIS 安装包到本地临时目录 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "download QQ installer".into(),
        })
        .await;
        let local_exe = std::env::temp_dir().join(format!(
            "ncd-qq-setup-{}-{}.exe",
            qq_version,
            std::process::id()
        ));
        let helper = DownloadHelper::new()?;
        helper
            .download_to_file(&download_url, &local_exe, None, ctx, 2)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // ===== Step 3:静默安装 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "run installer /s".into(),
        })
        .await;
        let installer = local_exe.to_string_lossy().to_string();
        let cmd = HostCommand::new(installer)
            .arg("/s")
            .timeout(std::time::Duration::from_secs(600));
        let run_result = host.run_to_string(cmd).await;
        // 不管装成功与否都尽量清掉安装包，避免占 %TEMP%。
        let _ = tokio::fs::remove_file(&local_exe).await;
        let out = run_result?;
        if !out.success() {
            return Err(ActionError::install_step(
                "qq_silent_install",
                format!(
                    "installer exit={:?} stderr={}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// Windows uninstall：QQ 官方安装包没有可靠的静默卸载入口，让用户走
    /// 系统“应用和功能”卸载，这里直接拒绝而不是装作成功。
    async fn uninstall_windows(
        &self,
        _host: &dyn Host,
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        Err(ActionError::other(
            "Windows QQ 请在系统“应用和功能”里卸载；本工程不接管 QQ 官方卸载流程",
        ))
    }

    /// Windows verify：注册表能查到 Install，且 detect 能解析出真实版本号
    /// （新布局 versions/config.json 或旧布局 package.json 任一命中）。
    async fn verify_windows(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let install_root = self.query_windows_install_root(host).await?;
        let mut report = VerifyReport::ok().with_check(
            "registry Install value present",
            install_root.is_some(),
            install_root.as_ref().map(|p| format!("{p}")),
        );
        if let Ok(Some(v)) = self.detect(host).await {
            report = report.with_check(
                "qq version detected",
                v.version != "unknown",
                Some(format!("version={}", v.version)),
            );
        }
        Ok(report)
    }
}

/// 从 QQNT 新布局 `versions/config.json` 解析 `curVersion` 字段。
fn parse_qqnt_cur_version(bytes: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let v = json.get("curVersion").and_then(|v| v.as_str())?;
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// 从旧布局 `resources/app/package.json` 解析 `version` 字段。
fn parse_qq_package_version(bytes: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let v = json.get("version").and_then(|v| v.as_str())?;
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// 解析 `reg query ... /v Install` 的 stdout，抽出 `Install REG_SZ <path>`
/// 里的 path。形如：
///
/// ```text
/// HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Tencent\QQNT
///     Install    REG_SZ    C:\Program Files\Tencent\QQNT
/// ```
fn parse_reg_install_value(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Install") {
            continue;
        }
        // 值类型固定 REG_SZ；按它切分，右侧即注册表里的路径字符串。
        if let Some((_, rest)) = trimmed.split_once("REG_SZ") {
            let path = rest.trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// 拉 pcConfig.json 解析 Windows 段的 (version, ntDownloadX64Url)。
async fn fetch_windows_qq_release() -> Result<(String, String), ActionError> {
    let resp = ncd_network::shared_client()
        .get(QQ_PCCONFIG_URL)
        .send()
        .await
        .map_err(|e| ActionError::DownloadFailed {
            url: QQ_PCCONFIG_URL.to_string(),
            reason: e.to_string(),
        })?;
    let body = resp
        .text()
        .await
        .map_err(|e| ActionError::DownloadFailed {
            url: QQ_PCCONFIG_URL.to_string(),
            reason: e.to_string(),
        })?;
    parse_windows_qq_release(&body)
}

/// 从 pcConfig.json 文本解析 Windows 段的 version 与 x64 NSIS 安装包地址。
fn parse_windows_qq_release(body: &str) -> Result<(String, String), ActionError> {
    let json: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        ActionError::install_step("parse_pcconfig", format!("invalid json: {e}"))
    })?;
    let win = json.get("Windows").ok_or_else(|| {
        ActionError::install_step("parse_pcconfig", "missing Windows section")
    })?;
    let version = win
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let url = win
        .get("ntDownloadX64Url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ActionError::install_step("parse_pcconfig", "missing Windows.ntDownloadX64Url")
        })?
        .to_string();
    Ok((version, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp() -> QQComponent {
        QQComponent::default_v3_2_25(HostPath::from_posix("/home/test/Napcat"))
    }

    #[test]
    fn default_locks_known_version() {
        let c = comp();
        assert_eq!(c.version, "3.2.25-45758");
        assert_eq!(c.url_hash_segment, "7516007c");
    }

    #[test]
    fn package_filename_amd64_deb() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Deb, Arch::X86_64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_amd64.deb");
    }

    #[test]
    fn package_filename_arm64_deb() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Deb, Arch::Aarch64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_arm64.deb");
    }

    #[test]
    fn package_filename_x86_64_rpm() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Rpm, Arch::X86_64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_x86_64.rpm");
    }

    #[test]
    fn package_filename_aarch64_rpm() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Rpm, Arch::Aarch64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_aarch64.rpm");
    }

    #[test]
    fn package_filename_unsupported_arch_returns_error() {
        let c = comp();
        let err = c.package_filename(PackageFormat::Deb, Arch::X86).unwrap_err();
        assert!(matches!(err, ActionError::UnsupportedTarget { .. }));
    }

    #[test]
    fn download_url_format_matches_official() {
        let c = comp();
        let url = c.build_download_url(PackageFormat::Deb, Arch::X86_64).unwrap();
        // 与官方 install.sh L640 完全一致
        assert_eq!(
            url,
            "https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_amd64.deb"
        );
    }

    #[test]
    fn download_url_arm64_deb() {
        let c = comp();
        let url = c.build_download_url(PackageFormat::Deb, Arch::Aarch64).unwrap();
        assert_eq!(
            url,
            "https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_arm64.deb"
        );
    }

    #[test]
    fn paths_align_with_official_install_layout() {
        let c = comp();
        // 官方 install.sh L15 / L19 / L21
        assert_eq!(c.qq_base_path().as_posix(), "/home/test/Napcat/opt/QQ");
        assert_eq!(c.qq_executable().as_posix(), "/home/test/Napcat/opt/QQ/qq");
        assert_eq!(
            c.qq_package_json().as_posix(),
            "/home/test/Napcat/opt/QQ/resources/app/package.json"
        );
    }

    #[test]
    fn supported_targets_include_windows_and_linux() {
        let c = comp();
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Local)));
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Remote)));
        assert!(c.supported_targets().contains(&(Os::Windows, Locality::Local)));
    }

    #[test]
    fn info_lists_windows_local_in_supported_targets() {
        let info = QQComponent::info();
        assert!(info.supported_targets.iter().any(|t| {
            t.os == Os::Windows && t.locality == Locality::Local
        }));
    }

    #[test]
    fn parse_reg_install_value_extracts_path() {
        let stdout = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Tencent\\QQNT\r\n    Install    REG_SZ    C:\\Program Files\\Tencent\\QQNT\r\n\r\n";
        assert_eq!(
            parse_reg_install_value(stdout),
            Some(r"C:\Program Files\Tencent\QQNT".to_string())
        );
    }

    #[test]
    fn parse_reg_install_value_returns_none_when_absent() {
        let stdout = "ERROR: The system was unable to find the specified registry key or value.";
        assert_eq!(parse_reg_install_value(stdout), None);
    }

    #[test]
    fn parse_windows_qq_release_picks_x64_url() {
        // 截取自真实 pcConfig.json 结构（Windows 段）。
        let body = r#"{"Windows":{"version":"9.9.31","ntDownloadX64Url":"https://qqdl.gtimg.cn/qqfile/QQNT/9.9.31/release/092069d7/QQ_9.9.31_260528_x64_01.exe"},"Linux":{"version":"3.2.29"}}"#;
        let (ver, url) = parse_windows_qq_release(body).unwrap();
        assert_eq!(ver, "9.9.31");
        assert!(url.ends_with("_x64_01.exe"));
    }

    #[test]
    fn parse_windows_qq_release_errors_without_x64_url() {
        let body = r#"{"Windows":{"version":"9.9.31"}}"#;
        assert!(parse_windows_qq_release(body).is_err());
    }

    #[test]
    fn parse_qqnt_cur_version_reads_config_json() {
        // 真实 versions/config.json 结构。
        let body = br#"{"baseVersion":"9.9.26-44343","curVersion":"9.9.26-44343","buildId":"44343"}"#;
        assert_eq!(
            parse_qqnt_cur_version(body),
            Some("9.9.26-44343".to_string())
        );
    }

    #[test]
    fn parse_qqnt_cur_version_none_when_empty_or_missing() {
        assert_eq!(parse_qqnt_cur_version(br#"{"curVersion":""}"#), None);
        assert_eq!(parse_qqnt_cur_version(br#"{"baseVersion":"x"}"#), None);
        assert_eq!(parse_qqnt_cur_version(b"not json"), None);
    }

    #[test]
    fn parse_qq_package_version_reads_old_layout() {
        let body = br#"{"name":"qq","version":"9.9.15-32869"}"#;
        assert_eq!(
            parse_qq_package_version(body),
            Some("9.9.15-32869".to_string())
        );
    }

    #[test]
    fn id_returns_qq() {
        assert_eq!(comp().id(), ComponentId::Qq);
    }

    #[test]
    fn install_base_dir_can_be_custom() {
        let c = QQComponent::default_v3_2_25(HostPath::from_posix("/opt/napcat"));
        assert_eq!(c.qq_executable().as_posix(), "/opt/napcat/opt/QQ/qq");
    }
}

//! QQComponent:QQ runtime 组件
//!
//! 跨平台:
//! - Linux 本地/远端:rootless 安装,对齐 NapCat-Installer 官方一键脚本
//!   下载 deb/rpm 解压到 <install_base_dir>/opt/QQ/
//! - Windows 本地:detect 走注册表
//!   HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT\Install 拿安装根,新版 QQNT
//!   按版本分目录,版本号在 versions/config.json 的 curVersion;旧版是
//!   扁平的 resources/app/package.json,安装走官方 pcConfig.json 拿 NSIS
//!   安装包跑 installer.exe /s 静默安装
//!
//! Linux 安装路径(rootless):
//! - $INSTALL_BASE_DIR/opt/QQ/:QQ 解压根
//! - $INSTALL_BASE_DIR/opt/QQ/qq:QQ 可执行
//! - $INSTALL_BASE_DIR/opt/QQ/resources/app/package.json:版本探测点
//!
//! Linux 版本发现(官方为主,社区为辅,pin 兜底):
//! 1. 官方 pcConfig.json 的 Linux.x64/arm*DownloadUrl.{deb,rpm} 完整直链
//! 2. 社区 nclatest get_qq_ver (linuxVersion + linuxVerHash → 旧 dldir1 拼法)
//! 3. QQComponent 上 pin 的 version + url_hash_segment(离线可装)
//!
//! Windows 版本号通过 pcConfig.json 实时拉取,不固化
//!
//! Linux 安装流程(rootless):
//! 1. 探测 dpkg-deb 或 rpm2cpio 哪个可用(用 which 或 command -v)
//! 2. 解析下载 URL(见上) → 下载对应 deb/rpm 包 → 上传到远端 <tmp>/
//! 3. dpkg-deb -x 或 rpm2cpio | cpio -idm 解压到 <install_base_dir>
//! 4. 删除安装包,清理临时文件
//!
//! Windows 安装流程:
//! 1. HTTP GET https://cdn-go.cn/qq-web/im.qq.com_new/latest/rainbow/pcConfig.json
//! 2. 取 Windows.ntDownloadX64Url → 下载 NSIS 安装包到本地临时目录
//! 3. 跑 installer.exe /s 静默安装,等待退出码 0
//! 4. 删除本地临时安装包

use async_trait::async_trait;

use ncd_host::{Arch, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::shell_quote;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 腾讯 QQ 实时版本 / 下载地址清单(Win + Linux 同源,legacy Urls.QQ_Version)
pub const QQ_PCCONFIG_URL: &str =
    "https://cdn-go.cn/qq-web/im.qq.com_new/latest/rainbow/pcConfig.json";

/// NapCat 社区推荐 QQ 版本(辅路;挂掉则跳过)
pub const NCLATEST_QQ_VER_URL: &str = "https://nclatest.znin.net/get_qq_ver";

/// 离线 pin:旧版 dldir1 拼法仍可用时的最后兜底
const PIN_LINUX_QQ_VERSION: &str = "3.2.25-45758";
const PIN_LINUX_QQ_HASH: &str = "7516007c";

/// Windows QQNT 安装信息所在注册表子键(legacy PathFunc.get_qq_path)
const QQ_REGISTRY_SUBKEY: &str = r"SOFTWARE\WOW6432Node\Tencent\QQNT";

/// 包格式(rootless 模式只需要 dpkg / rpm 两种)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageFormat {
    Deb,
    Rpm,
}

/// Linux QQ 一次安装解析到的发布信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxQqRelease {
    pub version: String,
    pub download_url: String,
    /// 来源标签,进安装日志便于排障
    pub source: &'static str,
}

/// QQ component 配置
#[derive(Debug, Clone)]
pub struct QQComponent {
    /// pin 版本号(如 "3.2.25-45758");动态发现失败时拼旧 CDN 用
    pub version: String,
    /// pin 的腾讯 CDN hash 段(如 "7516007c")
    pub url_hash_segment: String,
    /// 安装根目录(对齐官方 $HOME/Napcat)
    pub install_base_dir: HostPath,
    /// 期望 SHA256(可选,腾讯不提供官方 SHA256,通常为 None)
    pub expected_sha256: Option<String>,
    /// 临时目录
    pub tmp_dir: HostPath,
}

impl QQComponent {
    /// 创建一个 QQ component 描述(自定义所有字段)
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

    /// 默认 pin:已知可装版本 v3.2.25-45758,安装到 $HOME/Napcat
    /// 动态发现失败时才用这组 version/hash 拼旧 CDN;业务代码不改
    pub fn default_v3_2_25(install_base_dir: HostPath) -> Self {
        Self::new(PIN_LINUX_QQ_VERSION, PIN_LINUX_QQ_HASH, install_base_dir)
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
        Ok(format!("linuxqq_{}_{arch_str}.{ext}", self.version))
    }

    fn build_download_url(&self, pkg: PackageFormat, arch: Arch) -> Result<String, ActionError> {
        let filename = self.package_filename(pkg, arch)?;
        Ok(format!(
            "https://dldir1.qq.com/qqfile/qq/QQNT/{}/{filename}",
            self.url_hash_segment
        ))
    }

    /// pin 兜底发布信息(不发起网络)
    fn pin_linux_release(
        &self,
        pkg: PackageFormat,
        arch: Arch,
    ) -> Result<LinuxQqRelease, ActionError> {
        Ok(LinuxQqRelease {
            version: self.version.clone(),
            download_url: self.build_download_url(pkg, arch)?,
            source: "pin",
        })
    }

    /// 官方 → 社区 → pin;任一层成功即返回
    async fn resolve_linux_release(
        &self,
        pkg: PackageFormat,
        arch: Arch,
        ctx: &mut ActionCtx,
    ) -> Result<LinuxQqRelease, ActionError> {
        match fetch_linux_qq_from_pcconfig(pkg, arch).await {
            Ok(rel) => {
                ctx.info(format!(
                    "Linux QQ 发布源=pcConfig version={} url={}",
                    rel.version, rel.download_url
                ))
                .await;
                return Ok(rel);
            }
            Err(e) => {
                ctx.info(format!("pcConfig Linux 段不可用,试社区源: {e}"))
                    .await;
            }
        }

        match fetch_linux_qq_from_nclatest(pkg, arch).await {
            Ok(rel) => {
                ctx.info(format!(
                    "Linux QQ 发布源=nclatest version={} url={}",
                    rel.version, rel.download_url
                ))
                .await;
                return Ok(rel);
            }
            Err(e) => {
                ctx.info(format!("nclatest 不可用,回退 pin: {e}")).await;
            }
        }

        let pin = self.pin_linux_release(pkg, arch)?;
        ctx.info(format!(
            "Linux QQ 发布源=pin version={} url={}",
            pin.version, pin.download_url
        ))
        .await;
        Ok(pin)
    }

    /// 探测远端有 dpkg-deb 还是 rpm2cpio,dpkg 优先(deb 更普遍)
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

    /// 确保 Linux QQ 系统依赖已安装(仅 Linux)
    pub async fn ensure_linux_dependencies(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        if host.os() != Os::Linux {
            return Ok(()); // Windows 不需要
        }

        ctx.info("检测 QQ 系统依赖").await;
        let manifest = crate::qq_deps::qq_qqnt_dependencies_v3_2_25();
        let detector = crate::qq_deps::QqDependencyDetector::new(manifest);
        let report = detector.detect(host, None).await?;

        if report.missing.is_empty() {
            ctx.info("系统依赖已满足").await;
            return Ok(());
        }

        ctx.info(format!(
            "发现 {} 个缺失依赖: {}",
            report.missing.len(),
            report
                .missing
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .await;

        let installer = crate::qq_deps::QqDependencyInstaller;
        let missing_names: Vec<String> = report.missing.iter().map(|p| p.name.clone()).collect();
        // sudo_password None:期望 Host 连接建立时已从 keyring 注入密码;
        // 如果 probe 到 PasswordRequired 且 Host 也没有密码则返回 elevation_required,
        // 但 deploy path 无前端可弹窗,只能记日志让用户看到安装失败
        let result = installer.install(host, missing_names, None, ctx).await?;

        if result.elevation_required {
            return Err(ActionError::install_step(
                "install_dependencies",
                "elevation_required: 安装 QQ 系统依赖需要 sudo 密码，请在提示中输入后重试",
            ));
        }

        if !result.success {
            let failed_list: Vec<String> = result
                .failed
                .iter()
                .map(|f| format!("{}: {}", f.name, f.reason))
                .collect();
            return Err(ActionError::install_step(
                "install_dependencies",
                format!("部分依赖安装失败：{}", failed_list.join(", ")),
            ));
        }

        ctx.info(format!("成功安装 {} 个依赖", result.installed.len()))
            .await;
        Ok(())
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

    /// 组件元数据,给 list_components Tauri command 使用
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
        // Windows 本机 + Linux 本地 / 远端,Windows 远端由 backend 的 SSH 逻辑
        // 处理,不走本 component
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

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
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
    async fn ensure_dependencies(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        self.check_target(host)?;
        if host.os() != Os::Linux {
            return Err(ActionError::UnsupportedTarget {
                component: "qq".into(),
                os: host.os(),
                locality: host.locality(),
            });
        }
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "安装 QQ 系统依赖".into(),
        })
        .await;
        let run = self.ensure_linux_dependencies(host, ctx).await;
        let ok = run.is_ok();
        if let Err(ref e) = run {
            ctx.log(crate::context::ProgressLogLevel::Error, e.to_string())
                .await;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok }).await;
        ctx.emit(ProgressKind::Finished { ok }).await;
        run
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // QQ 启动命令:<install_base>/opt/QQ/qq <extra_args>,backend 再拼
        // --no-sandbox -q <qqid> 等参数,不在 Component 这层
        Ok(args.apply_to(HostCommand::new(self.qq_executable().as_posix())))
    }
}

// 平台分发实装:Linux rootless 解包 + Windows 官方静默安装

impl QQComponent {
    /// Linux detect:读 <install_base>/opt/QQ/resources/app/package.json
    /// 的 version 字段
    async fn detect_linux(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let pkg_json = self.qq_package_json();
        if !host.exists(&pkg_json).await? {
            return Ok(None);
        }

        let bytes = match host.read_file(&pkg_json).await {
            Ok(b) => b,
            Err(HostError::PathNotFound { .. }) => return Ok(None),
            Err(e) => return Err(ActionError::Host(e)),
        };

        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| ActionError::detect_failed("qq", format!("parse package.json: {e}")))?;

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

    /// Linux install:rootless 下载 deb/rpm → 上传 → dpkg-deb -x / rpm2cpio 解包
    async fn install_linux(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "check system dependencies".into(),
        })
        .await;
        self.ensure_linux_dependencies(host, ctx).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "detect package format".into(),
        })
        .await;
        let pkg_format = self.detect_package_format(host).await?;
        ctx.info(format!("QQ 安装包格式: {pkg_format:?}")).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "download QQ package".into(),
        })
        .await;
        let release = self
            .resolve_linux_release(pkg_format, host.arch(), ctx)
            .await?;
        let url = release.download_url.clone();
        ctx.info(format!(
            "准备安装 Linux QQ {} (source={}) 到 {}",
            release.version,
            release.source,
            self.install_base_dir.as_posix()
        ))
        .await;
        ctx.info(format!("QQ 安装包下载地址: {url}")).await;

        host.create_dir_all(&self.tmp_dir).await?;
        let remote_pkg = self.tmp_dir.join(format!(
            "ncd-qq-{}.{}",
            std::process::id(),
            match pkg_format {
                PackageFormat::Deb => "deb",
                PackageFormat::Rpm => "rpm",
            }
        ));

        // QQ 走腾讯 CDN(gtimg / dldir1)，没有 GitHub 反代可用；
        // build_mirror_urls 会拼出无效候选，race 全挂后报 all mirrors failed。
        let download_candidates = vec![url.clone()];
        ctx.info("准备获取 QQ 安装包（官方直链，不经 GitHub 镜像）")
            .await;
        let mut remote_download_ok = false;

        if host.locality() == ncd_host::Locality::Remote {
            if host.download_url(&url, &remote_pkg).await.is_ok() {
                ctx.info(format!("远端直接下载 QQ 安装包成功: {url}"))
                    .await;
                remote_download_ok = true;
            }
        }

        if !remote_download_ok {
            if host.locality() == ncd_host::Locality::Remote {
                ctx.info("远端直接下载不可用，改为本机下载后上传").await;
            } else {
                ctx.info("本机下载 QQ 安装包").await;
            }
            let local_tmp = std::env::temp_dir().join(format!(
                "ncd-qq-{}-{}.{}",
                release.version.replace(['/', '\\', ' '], "_"),
                std::process::id(),
                match pkg_format {
                    PackageFormat::Deb => "deb",
                    PackageFormat::Rpm => "rpm",
                }
            ));

            let helper = DownloadHelper::new()?;
            helper
                .download_with_mirrors(
                    &download_candidates,
                    &local_tmp,
                    self.expected_sha256.as_deref(),
                    ctx,
                    2,
                )
                .await?;

            host.upload(&local_tmp, &remote_pkg).await?;
            ctx.info(format!("QQ 安装包已就位 {}", remote_pkg.as_posix()))
                .await;
            let _ = tokio::fs::remove_file(&local_tmp).await;
        }

        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "extract QQ".into(),
        })
        .await;
        host.create_dir_all(&self.install_base_dir).await?;
        let install_base = self.install_base_dir.as_posix();
        let pkg_path = remote_pkg.as_posix();
        ctx.info(format!("解包 QQ 安装包到 {install_base}")).await;

        let extract_cmd = match pkg_format {
            PackageFormat::Deb => HostCommand::new("dpkg-deb")
                .arg("-x")
                .arg(pkg_path)
                .arg(install_base),
            PackageFormat::Rpm => HostCommand::new("sh").arg("-c").arg(format!(
                "rpm2cpio {} | (cd {} && cpio -idm)",
                shell_quote(pkg_path),
                shell_quote(install_base)
            )),
        };
        let out = host.run_to_string(extract_cmd).await?;
        if !out.success() {
            return Err(ActionError::install_step(
                "extract_qq",
                format!("exit={:?} stderr={}", out.exit_code, out.stderr.trim()),
            ));
        }
        let _ = host.remove_file(&remote_pkg).await;
        ctx.info(format!(
            "Linux QQ {} 已安装到 {} (source={})",
            release.version,
            self.qq_base_path().as_posix(),
            release.source
        ))
        .await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall_linux(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        // rootless 卸载:删 <install_base>/opt/QQ 整棵子树
        // System 布局(/opt/QQ)需要 sudo,但 rootless 是当前默认布局;
        // 如果 install_base = "/" 删 /opt/QQ 会因权限失败,那时让用户用
        // 系统包管理器(dpkg -P linuxqq / apt remove linuxqq)卸载
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
            .with_check(
                "qq executable exists",
                bin_exists,
                Some(format!("{qq_bin}")),
            )
            .with_check(
                "package.json exists",
                json_exists,
                Some(format!("{pkg_json}")),
            );

        if json_exists {
            match self.detect(host).await {
                Ok(Some(v)) => {
                    // 动态发现装的版本不必等于 pin;能读到真实版本即可
                    report = report.with_check(
                        "qq version detected",
                        !v.version.is_empty() && v.version != "unknown",
                        Some(format!("version={} (pin={})", v.version, self.version)),
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
                    report = report.with_check("version detect", false, Some(format!("{e}")));
                }
            }
        }
        Ok(report)
    }

    // Windows 本机实装

    /// Windows detect:注册表 HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT 的
    /// Install 值拿安装根,新版 QQNT 把客户端按版本分目录放在
    /// versions/<curVersion>/ 下,版本号写在 versions/config.json 的
    /// curVersion,旧版 QQ 是扁平的 resources/app/package.json,两种布局
    /// 都试一遍
    async fn detect_windows(
        &self,
        host: &dyn Host,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        let install_root = match self.query_windows_install_root(host).await? {
            Some(p) => p,
            None => return Ok(None),
        };

        // 新布局优先:versions/config.json 的 curVersion 就是版本号
        let config_json = install_root.join("versions/config.json");
        if host.exists(&config_json).await? {
            if let Ok(bytes) = host.read_file(&config_json).await {
                if let Some(ver) = parse_json_string_field(&bytes, "curVersion") {
                    return Ok(Some(DetectedVersion {
                        version: ver,
                        source: format!("{config_json}"),
                    }));
                }
            }
        }

        // 回退旧布局:扁平 resources/app/package.json
        let pkg_json = install_root.join("resources/app/package.json");
        if host.exists(&pkg_json).await? {
            if let Ok(bytes) = host.read_file(&pkg_json).await {
                if let Some(ver) = parse_json_string_field(&bytes, "version") {
                    return Ok(Some(DetectedVersion {
                        version: ver,
                        source: format!("{pkg_json}"),
                    }));
                }
            }
        }

        // 注册表有 Install 但两种布局都没拿到版本号:QQ 装过但结构不认识,
        // 标 unknown 让 UI 显示"已安装"而不是"未安装"
        Ok(Some(DetectedVersion {
            version: "unknown".to_string(),
            source: format!("{install_root} (no recognizable version source)"),
        }))
    }

    /// 跑 reg query 拿 QQNT 的 Install 值,转成 HostPath
    /// 注册表项不存在(未装 QQ)时返回 Ok(None)
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
            // reg query 在键不存在时退出码非 0;run_to_string 不会因非 0 报
            // Err,但若 reg 自身起不来(PATH 缺失等)才会到这里,按未装处理
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

    /// Windows install:拉 pcConfig.json 拿 NSIS 安装包地址 → 下载 → 跑
    /// installer.exe /s 静默安装(对齐 legacy QQInstall)
    async fn install_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        ctx.info("准备获取 QQ Windows 安装器").await;

        // Step 1:拉 pcConfig.json 解析下载地址
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "fetch QQ pcConfig.json".into(),
        })
        .await;
        let (qq_version, download_url) = fetch_windows_qq_release().await?;
        ctx.info(format!("QQ Windows {qq_version} -> {download_url}"))
            .await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // Step 2:下载 NSIS 安装包到本地临时目录
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
        // Windows 安装包同样是腾讯 CDN，只走官方直链
        helper
            .download_with_mirrors(&[download_url.clone()], &local_exe, None, ctx, 2)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // Step 3:静默安装
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "run installer /s".into(),
        })
        .await;
        let installer = local_exe.to_string_lossy().to_string();
        ctx.info(format!("运行 QQ 静默安装器: {installer}")).await;
        let cmd = HostCommand::new(installer)
            .arg("/s")
            .timeout(std::time::Duration::from_secs(600));
        let run_result = host.run_to_string(cmd).await;
        // 不管装成功与否都尽量清掉安装包,避免占 %TEMP%
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
        ctx.info("QQ Windows 静默安装完成").await;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// Windows uninstall:QQ 官方安装包没有可靠的静默卸载入口,让用户走
    /// 系统“应用和功能”卸载,这里直接拒绝而不是装作成功
    async fn uninstall_windows(
        &self,
        _host: &dyn Host,
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        Err(ActionError::other(
            "Windows QQ 请在系统“应用和功能”里卸载；本工程不接管 QQ 官方卸载流程",
        ))
    }

    /// Windows verify:注册表能查到 Install,且 detect 能解析出真实版本号
    /// (新布局 versions/config.json 或旧布局 package.json 任一命中)
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

// 从 JSON 对象取指定字段,trim 后空串视为缺失
// QQ 新布局 versions/config.json 用 "curVersion",旧布局 package.json 用 "version"
fn parse_json_string_field(bytes: &[u8], field: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let v = json.get(field).and_then(|v| v.as_str())?;
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// 解析 reg query ... /v Install 的 stdout,抽出 Install REG_SZ <path>
/// 里的 path
/// stdout 形如(第二行带前导缩进):
///     HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Tencent\QQNT
///         Install    REG_SZ    C:\Program Files\Tencent\QQNT
fn parse_reg_install_value(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Install") {
            continue;
        }
        // 值类型固定 REG_SZ;按它切分,右侧即注册表里的路径字符串
        if let Some((_, rest)) = trimmed.split_once("REG_SZ") {
            let path = rest.trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// 拉 pcConfig.json 解析 Windows 段的 (version, ntDownloadX64Url)
async fn fetch_windows_qq_release() -> Result<(String, String), ActionError> {
    let body = fetch_text(QQ_PCCONFIG_URL).await?;
    parse_windows_qq_release(&body)
}

/// 从 pcConfig.json 文本解析 Windows 段的 version 与 x64 NSIS 安装包地址
fn parse_windows_qq_release(body: &str) -> Result<(String, String), ActionError> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| ActionError::install_step("parse_pcconfig", format!("invalid json: {e}")))?;
    let win = json
        .get("Windows")
        .ok_or_else(|| ActionError::install_step("parse_pcconfig", "missing Windows section"))?;
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

async fn fetch_text(url: &str) -> Result<String, ActionError> {
    let resp = ncd_network::shared_client()
        .get(url)
        .send()
        .await
        .map_err(|e| ActionError::DownloadFailed {
            url: url.to_string(),
            reason: e.to_string(),
        })?;
    if !resp.status().is_success() {
        return Err(ActionError::DownloadFailed {
            url: url.to_string(),
            reason: format!("HTTP {}", resp.status()),
        });
    }
    resp.text().await.map_err(|e| ActionError::DownloadFailed {
        url: url.to_string(),
        reason: e.to_string(),
    })
}

/// 官方 pcConfig Linux 段 → 完整 deb/rpm 直链
async fn fetch_linux_qq_from_pcconfig(
    pkg: PackageFormat,
    arch: Arch,
) -> Result<LinuxQqRelease, ActionError> {
    let body = fetch_text(QQ_PCCONFIG_URL).await?;
    parse_linux_qq_from_pcconfig(&body, pkg, arch)
}

/// 给 release 快照 / 组件页用:探测 Linux QQ 当前可装版本(官方→社区→pin)
///
/// arch 用 x86_64 deb 作为版本代表即可(腾讯各 arch 版本号一致)
pub async fn probe_linux_qq_latest() -> Result<LinuxQqRelease, ActionError> {
    match fetch_linux_qq_from_pcconfig(PackageFormat::Deb, Arch::X86_64).await {
        Ok(rel) => return Ok(rel),
        Err(_) => {}
    }
    match fetch_linux_qq_from_nclatest(PackageFormat::Deb, Arch::X86_64).await {
        Ok(rel) => return Ok(rel),
        Err(_) => {}
    }
    QQComponent::default_v3_2_25(HostPath::from_posix("/tmp/ncd-probe"))
        .pin_linux_release(PackageFormat::Deb, Arch::X86_64)
}

/// 给 release 快照用:探测 Windows QQ 当前安装包版本
pub async fn probe_windows_qq_latest() -> Result<(String, String), ActionError> {
    fetch_windows_qq_release().await
}

fn parse_linux_qq_from_pcconfig(
    body: &str,
    pkg: PackageFormat,
    arch: Arch,
) -> Result<LinuxQqRelease, ActionError> {
    let json: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        ActionError::install_step("parse_pcconfig_linux", format!("invalid json: {e}"))
    })?;
    let linux = json.get("Linux").ok_or_else(|| {
        ActionError::install_step("parse_pcconfig_linux", "missing Linux section")
    })?;
    let version = linux
        .get("version")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string();

    let arch_key = match arch {
        Arch::X86_64 => "x64DownloadUrl",
        Arch::Aarch64 => "armDownloadUrl",
        _ => {
            return Err(ActionError::UnsupportedTarget {
                component: "qq".into(),
                os: Os::Linux,
                locality: Locality::Remote,
            });
        }
    };
    let fmt_key = match pkg {
        PackageFormat::Deb => "deb",
        PackageFormat::Rpm => "rpm",
    };
    let url = linux
        .get(arch_key)
        .and_then(|v| v.get(fmt_key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ActionError::install_step(
                "parse_pcconfig_linux",
                format!("missing Linux.{arch_key}.{fmt_key}"),
            )
        })?
        .to_string();

    Ok(LinuxQqRelease {
        version,
        download_url: url,
        source: "pcConfig",
    })
}

/// 社区 nclatest:linuxVersion + linuxVerHash → 旧 dldir1 拼法
async fn fetch_linux_qq_from_nclatest(
    pkg: PackageFormat,
    arch: Arch,
) -> Result<LinuxQqRelease, ActionError> {
    let body = fetch_text(NCLATEST_QQ_VER_URL).await?;
    parse_linux_qq_from_nclatest(&body, pkg, arch)
}

fn parse_linux_qq_from_nclatest(
    body: &str,
    pkg: PackageFormat,
    arch: Arch,
) -> Result<LinuxQqRelease, ActionError> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| ActionError::install_step("parse_nclatest", format!("invalid json: {e}")))?;
    let version = json
        .get("linuxVersion")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ActionError::install_step("parse_nclatest", "missing linuxVersion"))?
        .to_string();
    let hash = json
        .get("linuxVerHash")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ActionError::install_step("parse_nclatest", "missing linuxVerHash"))?
        .to_string();

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
    let download_url =
        format!("https://dldir1.qq.com/qqfile/qq/QQNT/{hash}/linuxqq_{version}_{arch_str}.{ext}");
    Ok(LinuxQqRelease {
        version,
        download_url,
        source: "nclatest",
    })
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
        let err = c
            .package_filename(PackageFormat::Deb, Arch::X86)
            .unwrap_err();
        assert!(matches!(err, ActionError::UnsupportedTarget { .. }));
    }

    #[test]
    fn download_url_format_matches_official() {
        let c = comp();
        let url = c
            .build_download_url(PackageFormat::Deb, Arch::X86_64)
            .unwrap();
        // 与官方 install.sh L640 完全一致
        assert_eq!(
            url,
            "https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_amd64.deb"
        );
    }

    #[test]
    fn download_url_arm64_deb() {
        let c = comp();
        let url = c
            .build_download_url(PackageFormat::Deb, Arch::Aarch64)
            .unwrap();
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
        assert!(
            c.supported_targets()
                .contains(&(Os::Linux, Locality::Local))
        );
        assert!(
            c.supported_targets()
                .contains(&(Os::Linux, Locality::Remote))
        );
        assert!(
            c.supported_targets()
                .contains(&(Os::Windows, Locality::Local))
        );
    }

    #[test]
    fn info_lists_windows_local_in_supported_targets() {
        let info = QQComponent::info();
        assert!(
            info.supported_targets
                .iter()
                .any(|t| { t.os == Os::Windows && t.locality == Locality::Local })
        );
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
        // 截取自真实 pcConfig.json 结构(Windows 段)
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
        // 真实 versions/config.json 结构
        let body =
            br#"{"baseVersion":"9.9.26-44343","curVersion":"9.9.26-44343","buildId":"44343"}"#;
        assert_eq!(
            parse_json_string_field(body, "curVersion"),
            Some("9.9.26-44343".to_string())
        );
    }

    #[test]
    fn parse_qqnt_cur_version_none_when_empty_or_missing() {
        assert_eq!(
            parse_json_string_field(br#"{"curVersion":""}"#, "curVersion"),
            None
        );
        assert_eq!(
            parse_json_string_field(br#"{"baseVersion":"x"}"#, "curVersion"),
            None
        );
        assert_eq!(parse_json_string_field(b"not json", "curVersion"), None);
    }

    #[test]
    fn parse_qq_package_version_reads_old_layout() {
        let body = br#"{"name":"qq","version":"9.9.15-32869"}"#;
        assert_eq!(
            parse_json_string_field(body, "version"),
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

    #[test]
    fn parse_linux_qq_from_pcconfig_x64_deb() {
        let body = r#"{"Linux":{"version":"3.2.31","x64DownloadUrl":{"deb":"https://qqdl.gtimg.cn/qqfile/QQNTV2/9.9.32/release/c390e792/QQ_3.2.31_260710_amd64_01.deb","rpm":"https://qqdl.gtimg.cn/x.rpm"},"armDownloadUrl":{"deb":"https://qqdl.gtimg.cn/arm.deb","rpm":"https://qqdl.gtimg.cn/arm.rpm"}}}"#;
        let rel = parse_linux_qq_from_pcconfig(body, PackageFormat::Deb, Arch::X86_64).unwrap();
        assert_eq!(rel.version, "3.2.31");
        assert_eq!(rel.source, "pcConfig");
        assert!(rel.download_url.ends_with("_amd64_01.deb"));
    }

    #[test]
    fn parse_linux_qq_from_pcconfig_arm_rpm() {
        let body = r#"{"Linux":{"version":"3.2.31","x64DownloadUrl":{"deb":"https://qqdl.gtimg.cn/x64.deb","rpm":"https://qqdl.gtimg.cn/x64.rpm"},"armDownloadUrl":{"deb":"https://qqdl.gtimg.cn/arm.deb","rpm":"https://qqdl.gtimg.cn/arm.rpm"}}}"#;
        let rel = parse_linux_qq_from_pcconfig(body, PackageFormat::Rpm, Arch::Aarch64).unwrap();
        assert_eq!(rel.version, "3.2.31");
        assert_eq!(rel.download_url, "https://qqdl.gtimg.cn/arm.rpm");
    }

    #[test]
    fn parse_linux_qq_from_pcconfig_missing_url_errors() {
        let body = r#"{"Linux":{"version":"3.2.31"}}"#;
        assert!(parse_linux_qq_from_pcconfig(body, PackageFormat::Deb, Arch::X86_64).is_err());
    }

    #[test]
    fn parse_linux_qq_from_nclatest_builds_old_cdn_url() {
        let body = r#"{"linuxVersion":"3.2.25-45758","linuxVerHash":"7516007c"}"#;
        let rel = parse_linux_qq_from_nclatest(body, PackageFormat::Deb, Arch::X86_64).unwrap();
        assert_eq!(rel.version, "3.2.25-45758");
        assert_eq!(rel.source, "nclatest");
        assert_eq!(
            rel.download_url,
            "https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_amd64.deb"
        );
    }

    #[test]
    fn pin_linux_release_matches_build_download_url() {
        let c = comp();
        let pin = c
            .pin_linux_release(PackageFormat::Deb, Arch::X86_64)
            .unwrap();
        assert_eq!(pin.source, "pin");
        assert_eq!(pin.version, "3.2.25-45758");
        assert_eq!(
            pin.download_url,
            c.build_download_url(PackageFormat::Deb, Arch::X86_64)
                .unwrap()
        );
    }
}

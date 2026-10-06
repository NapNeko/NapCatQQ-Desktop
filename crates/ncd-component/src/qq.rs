//! QQComponent:QQ runtime 组件
//!
//! 跨平台:
//! - Linux 本地/远端:rootless 安装,对齐 NapCat-Installer 官方一键脚本
//!   下载 deb/rpm 解压到 <install_base_dir>/opt/QQ/
//! - Windows 本地:侧装,不碰用户自己装的系统 QQ。安装包解包到
//!   <install_base_dir>/versions/<curVersion>/,current.txt 记当前版本;
//!   detect 先认侧装,没有才回落注册表里的系统 QQ(只读,不卸不改)
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
//! Windows 安装流程(侧装,不跑安装器、不写注册表、不要 UAC):
//! 1. pcConfig.json 取 Windows.ntDownloadX64Url;gtimg/QQNTV2 裸链先 UrlSign
//! 2. 下载到 data_root/runtime/cache/qq/QQNT-{version}.exe,已存在则复用
//! 3. 安装包是 PE,payload 是内嵌的 7z(LZMA+BCJ2)。7zr 直接 x 只会命中前面
//!    一个 28KB 的许可协议小包,所以先 `l -t#` 列出内嵌档案,取最大的 N.7z
//!    `e -t#` 抠出来再 `x`;payload 里 Files/ 就是完整 QQNT 树
//! 4. 读 Files/versions/config.json 的 curVersion,挪到 versions/<ver>/,写 current.txt
//! 5. 成功才删安装包;7zr 不在就从官方 Release 下到 components/7za/

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, Host, HostCommand, HostError, HostPath, HostShell, LinuxPackageManager, Locality, Os,
    PathStyle,
};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::requirement::{HostPackageGroup, Requirement};
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 腾讯 QQ 实时版本 / 下载地址清单(Win + Linux 同源,legacy Urls.QQ_Version)
pub const QQ_PCCONFIG_URL: &str =
    "https://cdn-go.cn/qq-web/im.qq.com_new/latest/rainbow/pcConfig.json";

/// 官网防盗链签名(Win/Linux gtimg/QQNTV2 裸链 403,需先换 sign+t)
/// 与 im.qq.com SPA 中 trpc.qqntv2.urlsign.UrlSign/GetSign 一致
pub const QQ_URL_SIGN_URL: &str =
    "https://im.qq.com/http2rpc/gotrpc/noauth/trpc.qqntv2.urlsign.UrlSign/GetSign";

/// NapCat 社区推荐 QQ 版本(辅路;挂掉则跳过)
pub const NCLATEST_QQ_VER_URL: &str = "https://nclatest.znin.net/get_qq_ver";

/// 离线 pin:旧版 dldir1 拼法仍可用时的最后兜底
const PIN_LINUX_QQ_VERSION: &str = "3.2.25-45758";
const PIN_LINUX_QQ_HASH: &str = "7516007c";

/// Windows QQNT 安装信息所在注册表子键(legacy PathFunc.get_qq_path)。
/// 启动链路找系统 QQ 用的是同一个键,别在别处再写一遍字面量
pub const QQ_REGISTRY_SUBKEY: &str = r"SOFTWARE\WOW6432Node\Tencent\QQNT";

/// 包格式(rootless 模式只需要 dpkg / rpm 两种)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageFormat {
    Deb,
    Rpm,
}

/// 官方 7zr 单文件:只认 7z 格式,但 `-t#` 能扫出 PE 里内嵌的 7z,够解 QQ 安装包
const SEVENZR_URL: &str = "https://github.com/ip7z/7zip/releases/download/26.02/7zr.exe";
const SEVENZR_SHA256: &str = "56b8cc9f4971cef253644fafe54063ed7fdca551d4dee0f8c6baa81b855acd72";

/// 侧装布局:<qq_root>/current.txt 一行版本号,<qq_root>/versions/<ver>/QQ.exe
const MANAGED_CURRENT_FILE: &str = "current.txt";
const MANAGED_VERSIONS_DIR: &str = "versions";
/// 侧装根在 components 下的目录名;解包安装、启动找 QQ、启动前提醒都按这个拼
pub const MANAGED_QQ_DIR_NAME: &str = "QQ";
/// 解包暂存目录前缀;上次中断留下的同前缀目录开装前一并清掉
const EXTRACT_STAGING_PREFIX: &str = "_extract_staging_";

/// 解一次 1.2GB 的树,机械盘上也够
const SEVENZIP_TIMEOUT: Duration = Duration::from_secs(1800);

/// components 目录下的侧装 QQ 根。调用方只有 components 根时用这个,
/// 别自己 join("QQ") —— 目录名改起来只有一处
pub fn managed_qq_root(components_dir: &Path) -> PathBuf {
    components_dir.join(MANAGED_QQ_DIR_NAME)
}

/// 侧装 QQ 的当前版本目录(含 QQ.exe)。current.txt 缺失或指向的目录没有 QQ.exe 都算没装
pub fn managed_qq_current_dir(qq_root: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(qq_root.join(MANAGED_CURRENT_FILE)).ok()?;
    let ver = raw.trim();
    if ver.is_empty() {
        return None;
    }
    let dir = qq_root
        .join(MANAGED_VERSIONS_DIR)
        .join(sanitize_qq_version_for_filename(ver));
    dir.join("QQ.exe").is_file().then_some(dir)
}

/// 从 `7z l -t#` 的输出里挑最大的内嵌 7z(条目名形如 `4.7z`),那个才是 QQ 本体
fn pick_embedded_7z_payload(listing: &str) -> Option<String> {
    listing
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace().rev();
            let name = tokens.next()?;
            let stem = name.strip_suffix(".7z")?;
            if stem.is_empty() || !stem.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let _packed = tokens.next()?;
            let size: u64 = tokens.next()?.parse().ok()?;
            Some((size, name.to_string()))
        })
        .max_by_key(|(size, _)| *size)
        .map(|(_, name)| name)
}

/// 跑一次 7z,非 0 退出带上输出尾巴报错
async fn run_sevenzip(
    host: &dyn Host,
    exe: &Path,
    args: &[String],
    step: &'static str,
) -> Result<String, ActionError> {
    let mut cmd = HostCommand::new(exe.to_string_lossy().to_string());
    for arg in args {
        cmd = cmd.arg(arg.clone());
    }
    let out = host.run_to_string(cmd.timeout(SEVENZIP_TIMEOUT)).await?;
    if !out.success() {
        let detail = if out.stderr.trim().is_empty() {
            out.stdout.trim()
        } else {
            out.stderr.trim()
        };
        let tail: String = detail
            .chars()
            .rev()
            .take(600)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(ActionError::install_step(
            step,
            format!("7z exit={:?}: {tail}", out.exit_code),
        ));
    }
    Ok(out.stdout)
}

/// 删目录;被占用(QQ 还开着)时给能看懂的话
async fn remove_managed_dir(dir: &Path) -> Result<(), ActionError> {
    match tokio::fs::remove_dir_all(dir).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ActionError::other(format!(
            "删不掉 {}：{e}。这个版本的 QQ 可能还开着，先停掉用它的 Bot 再试",
            dir.display()
        ))),
    }
}

/// 清掉上次中断留下的解包暂存目录,best-effort
async fn clear_stale_staging(qq_root: &Path) {
    let Ok(mut rd) = tokio::fs::read_dir(qq_root).await else {
        return;
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(EXTRACT_STAGING_PREFIX)
        {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
}

/// 版本号清洗成文件名安全片段:只留 [A-Za-z0-9._-],其它变 _,空串变 unknown
fn sanitize_qq_version_for_filename(version: &str) -> String {
    let cleaned: String = version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.chars().all(|c| c == '_') || cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

/// 缓存目录下的安装包路径(稳定文件名,跨进程/重启可复用)
fn windows_installer_cache_path(tmp_dir: &HostPath, version: &str) -> HostPath {
    tmp_dir.join(format!(
        "QQNT-{}.exe",
        sanitize_qq_version_for_filename(version)
    ))
}

/// 缓存安装包的本地文件系统路径(Windows host 上 tokio::fs 直接可用)
fn windows_installer_local_path(tmp_dir: &HostPath, version: &str) -> PathBuf {
    PathBuf::from(windows_installer_cache_path(tmp_dir, version).render(PathStyle::Windows))
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
    /// 安装根目录:Linux 对齐官方 $HOME/Napcat;Windows 是侧装根 components/QQ
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

    /// 官方 → 社区 → pin。
    /// gtimg/QQNTV2 裸链会 403,对每条候选先走官网 UrlSign 换可下载地址,不先探裸链。
    async fn resolve_linux_release(
        &self,
        pkg: PackageFormat,
        arch: Arch,
        ctx: &mut ActionCtx,
    ) -> Result<LinuxQqRelease, ActionError> {
        let mut candidates: Vec<LinuxQqRelease> = Vec::with_capacity(3);

        match fetch_linux_qq_from_pcconfig(pkg, arch).await {
            Ok(rel) => {
                ctx.info(format!(
                    "候选 Linux QQ: source=pcConfig version={} url={}",
                    rel.version, rel.download_url
                ))
                .await;
                candidates.push(rel);
            }
            Err(e) => {
                ctx.info(format!("pcConfig Linux 段不可用: {e}")).await;
            }
        }

        match fetch_linux_qq_from_nclatest(pkg, arch).await {
            Ok(rel) => {
                ctx.info(format!(
                    "候选 Linux QQ: source=nclatest version={} url={}",
                    rel.version, rel.download_url
                ))
                .await;
                candidates.push(rel);
            }
            Err(e) => {
                ctx.info(format!("nclatest 不可用: {e}")).await;
            }
        }

        let pin = self.pin_linux_release(pkg, arch)?;
        ctx.info(format!(
            "候选 Linux QQ: source=pin version={} url={}",
            pin.version, pin.download_url
        ))
        .await;
        candidates.push(pin);

        let mut last_err: Option<String> = None;
        for rel in candidates {
            // gtimg/QQNTV2 直接 UrlSign,不先探裸链;dldir1 原样可用
            match prepare_qq_download_url(&rel.download_url).await {
                Ok(prepared) => {
                    let signed = prepared != rel.download_url;
                    let source = if signed {
                        match rel.source {
                            "pcConfig" => "pcConfig+UrlSign",
                            "nclatest" => "nclatest+UrlSign",
                            "pin" => "pin+UrlSign",
                            other => other,
                        }
                    } else {
                        rel.source
                    };
                    ctx.info(format!(
                        "选用 Linux QQ source={} version={}{}",
                        source,
                        rel.version,
                        if signed { " (已 UrlSign)" } else { "" }
                    ))
                    .await;
                    return Ok(LinuxQqRelease {
                        version: rel.version,
                        download_url: prepared,
                        source,
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    ctx.info(format!(
                        "跳过 source={} url={}：{msg}",
                        rel.source, rel.download_url
                    ))
                    .await;
                    last_err = Some(msg);
                }
            }
        }

        Err(ActionError::install_step(
            "resolve_linux_qq",
            format!(
                "没有可下载的 Linux QQ 安装包（pcConfig/nclatest/pin 均不可达）{}",
                last_err
                    .map(|e| format!("; 最后错误: {e}"))
                    .unwrap_or_default()
            ),
        ))
    }

    /// 探测远端有 dpkg-deb 还是 rpm2cpio,dpkg 优先(deb 更普遍)
    async fn detect_package_format(&self, host: &dyn Host) -> Result<PackageFormat, ActionError> {
        for (binary, fmt) in &[
            ("dpkg-deb", PackageFormat::Deb),
            ("rpm2cpio", PackageFormat::Rpm),
        ] {
            if !host.command_exists(binary).await {
                continue;
            }
            if *fmt == PackageFormat::Rpm && !host.command_exists("cpio").await {
                // 只在报错时探包管理器，提示里给这台机能直接跑的那条
                let pm = match LinuxPackageManager::detect(host).await {
                    Some(pm @ LinuxPackageManager::Yum) => pm,
                    _ => LinuxPackageManager::Dnf,
                };
                return Err(ActionError::install_step(
                    "detect_pkg_format",
                    format!(
                        "已找到 rpm2cpio，但缺少 cpio，无法解包 LinuxQQ rpm。请在远端执行 sudo {} 后重试",
                        pm.install_hint(&["cpio"])
                    ),
                ));
            }
            return Ok(*fmt);
        }
        let how = match LinuxPackageManager::detect(host).await {
            Some(pm @ LinuxPackageManager::Apt) => {
                format!("执行 sudo {}", pm.install_hint(&["dpkg"]))
            }
            Some(pm @ (LinuxPackageManager::Dnf | LinuxPackageManager::Yum)) => {
                format!("执行 sudo {}", pm.install_hint(&["rpm2cpio", "cpio"]))
            }
            _ => format!(
                "安装 dpkg 或执行 sudo {}",
                LinuxPackageManager::Dnf.install_hint(&["rpm2cpio", "cpio"])
            ),
        };
        Err(ActionError::install_step(
            "detect_pkg_format",
            format!(
                "远端缺少 LinuxQQ 解包工具：既没有 dpkg-deb，也没有 rpm2cpio + cpio。请先{how} 后重试"
            ),
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
            description: "框架运行所需的 QQ 客户端".to_string(),
            repo_url: Some("https://im.qq.com/".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Windows, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
            ],
            category: crate::types::ComponentCategory::RuntimeDep,
            uninstall: crate::types::UninstallSupport::Supported,
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

    fn requirements(&self, os: Os, _locality: Locality) -> Vec<Requirement> {
        if os == Os::Linux {
            vec![Requirement::host_packages(HostPackageGroup::QqDependencies)]
        } else {
            Vec::new()
        }
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

        // 远端优先：host 上 wget/curl 直下并桥接进度；失败再本机下载 + upload
        let mut placed = false;
        if host.locality() == Locality::Remote {
            ctx.info(format!(
                "准备在远端下载 QQ 安装包（source={}）",
                release.source
            ))
            .await;
            match crate::host_download::download_url_to_host_with_progress(
                host,
                &url,
                &remote_pkg,
                ctx,
                3,
                self.expected_sha256.as_deref(),
            )
            .await
            {
                Ok(()) => {
                    placed = true;
                    ctx.info(format!("远端下载完成 {}", remote_pkg.as_posix()))
                        .await;
                }
                Err(ActionError::Cancelled) => return Err(ActionError::Cancelled),
                Err(e) => {
                    ctx.warn(format!("远端下载失败，回退本机下载后上传: {e}"))
                        .await;
                    let _ = host.remove_file(&remote_pkg).await;
                }
            }
        }

        if !placed {
            ctx.info(format!(
                "准备本机下载 QQ 安装包（source={}）",
                release.source
            ))
            .await;

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
                .download_to_file(&url, &local_tmp, self.expected_sha256.as_deref(), ctx, 3)
                .await?;

            if host.locality() == Locality::Remote {
                ctx.info("本机下载完成，上传到远端").await;
            }
            host.upload(&local_tmp, &remote_pkg).await?;
            let _ = tokio::fs::remove_file(&local_tmp).await;
        }

        ctx.info(format!("QQ 安装包已就位 {}", remote_pkg.as_posix()))
            .await;

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
                BashShell.escape(pkg_path),
                BashShell.escape(install_base)
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

    fn windows_qq_root(&self) -> PathBuf {
        PathBuf::from(self.install_base_dir.render(PathStyle::Windows))
    }

    /// 侧装的 7zr 放在 QQ 根的兄弟目录 components/7za/
    fn managed_sevenzr_path(&self) -> PathBuf {
        let root = self.windows_qq_root();
        root.parent().unwrap_or(&root).join("7za").join("7zr.exe")
    }

    /// Windows detect:先认侧装 current;没有再回落注册表里的系统 QQ。
    /// 系统 QQ 只拿来报「已安装」和兜底启动,安装 / 卸载都不碰它
    async fn detect_windows(
        &self,
        host: &dyn Host,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        if let Some(dir) = managed_qq_current_dir(&self.windows_qq_root()) {
            // QQ 目录树自带 versions/config.json,版本号在那里。这个 versions
            // 是 QQ 的布局,不是侧装外层那个 MANAGED_VERSIONS_DIR,别混用常量
            let cfg = dir.join("versions").join("config.json");
            let version = tokio::fs::read(&cfg)
                .await
                .ok()
                .and_then(|b| parse_json_string_field(&b, "curVersion"))
                .unwrap_or_else(|| "unknown".to_string());
            return Ok(Some(DetectedVersion {
                version,
                source: dir.display().to_string(),
            }));
        }
        self.detect_windows_system(host).await
    }

    /// 系统 QQ:注册表 Install 拿安装根,新布局 versions/config.json 的 curVersion,
    /// 旧布局扁平 resources/app/package.json,两种都试
    async fn detect_windows_system(
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

    /// Windows install:下安装包 → 7zr 解出 QQNT 树 → 落到侧装 versions/<ver>/。
    /// 不跑安装器,用户自己装的 QQ 不受影响;只有成功才删缓存包
    async fn install_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;
        ctx.info("准备获取 QQ Windows 安装器").await;

        // Step 1:拉 pcConfig.json 解析下载地址;gtimg 裸链先 UrlSign
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "fetch QQ pcConfig.json".into(),
        })
        .await;
        let (qq_version, download_url) = fetch_windows_qq_release().await?;
        ctx.info(format!("QQ Windows {qq_version} -> {download_url}"))
            .await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // Step 2:下载安装包到稳定缓存;已有完整 exe 直接复用
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "download QQ installer".into(),
        })
        .await;
        let local_exe = windows_installer_local_path(&self.tmp_dir, &qq_version);
        if let Some(parent) = local_exe.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ActionError::other(format!("创建 QQ 安装包缓存目录失败: {e}")))?;
        }
        if local_exe.is_file() {
            ctx.info(format!("复用已下载的 QQ 安装包 {}", local_exe.display()))
                .await;
        } else {
            // Windows 安装包是腾讯 CDN,单 URL 切片下载(≥16MB 切 4 片;
            // CDN 不支持 Range 时 download_smart 自动退单流)
            DownloadHelper::new()?
                .download_with_mirrors(
                    std::slice::from_ref(&download_url),
                    &local_exe,
                    None,
                    ctx,
                    2,
                )
                .await?;
        }
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }

        // Step 3:解包工具
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "prepare 7zr".into(),
        })
        .await;
        let sevenzip = self.ensure_sevenzip(ctx, 3).await?;
        ctx.info(format!("解包工具: {}", sevenzip.display())).await;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }

        // Step 4:解包落盘
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "extract QQ".into(),
        })
        .await;
        let qq_root = self.windows_qq_root();
        tokio::fs::create_dir_all(&qq_root)
            .await
            .map_err(|e| ActionError::other(format!("创建 {} 失败: {e}", qq_root.display())))?;
        clear_stale_staging(&qq_root).await;
        let staging = qq_root.join(format!("{EXTRACT_STAGING_PREFIX}{}", std::process::id()));
        let placed = self
            .extract_and_place(host, ctx, &sevenzip, &local_exe, &qq_root, &staging)
            .await;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        let version = placed?;

        // 只有装成功才清缓存;失败保留,重试不用重新下载
        let _ = tokio::fs::remove_file(&local_exe).await;
        ctx.info(format!(
            "QQ {version} 已解包到 {}",
            qq_root.join(MANAGED_VERSIONS_DIR).join(&version).display()
        ))
        .await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 7zr:侧装的 → 本机 7-Zip(完整版同样认 `-t#`)→ 下官方 7zr 到 components/7za/
    async fn ensure_sevenzip(&self, ctx: &ActionCtx, step: u32) -> Result<PathBuf, ActionError> {
        let managed = self.managed_sevenzr_path();
        if managed.is_file() {
            return Ok(managed);
        }
        for var in ["ProgramFiles", "ProgramW6432"] {
            if let Some(dir) = std::env::var_os(var) {
                let exe = PathBuf::from(dir).join("7-Zip").join("7z.exe");
                if exe.is_file() {
                    return Ok(exe);
                }
            }
        }
        if let Some(parent) = managed.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ActionError::other(format!("创建 {} 失败: {e}", parent.display())))?;
        }
        ctx.info("下载 7zr.exe").await;
        let mirrors = ncd_network::build_mirror_urls(SEVENZR_URL, None);
        DownloadHelper::new()?
            .download_with_mirrors_no_chunk(&mirrors, &managed, Some(SEVENZR_SHA256), ctx, step)
            .await?;
        Ok(managed)
    }

    /// 抠出内嵌 7z → 解出 Files/ → 挪到 versions/<curVersion>/ → 写 current.txt。返回版本号
    async fn extract_and_place(
        &self,
        host: &dyn Host,
        ctx: &ActionCtx,
        sevenzip: &Path,
        installer: &Path,
        qq_root: &Path,
        staging: &Path,
    ) -> Result<String, ActionError> {
        let installer_arg = installer.to_string_lossy().to_string();
        let carve_dir = staging.join("carve");
        let out_dir = staging.join("out");

        let listing = run_sevenzip(
            host,
            sevenzip,
            &[
                "l".into(),
                "-t#".into(),
                "-sccUTF-8".into(),
                installer_arg.clone(),
            ],
            "qq_list_payload",
        )
        .await?;
        let payload = pick_embedded_7z_payload(&listing).ok_or_else(|| {
            ActionError::install_step("qq_list_payload", "安装包里没找到内嵌的 7z，格式可能变了")
        })?;

        ctx.info(format!("抠出安装包内的 {payload}")).await;
        run_sevenzip(
            host,
            sevenzip,
            &[
                "e".into(),
                "-t#".into(),
                "-y".into(),
                "-bso0".into(),
                "-bsp0".into(),
                format!("-o{}", carve_dir.display()),
                installer_arg,
                payload.clone(),
            ],
            "qq_carve_payload",
        )
        .await?;

        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        ctx.info("解包 QQ（约 1.2GB，需要十几秒到几分钟）").await;
        run_sevenzip(
            host,
            sevenzip,
            &[
                "x".into(),
                "-y".into(),
                "-bso0".into(),
                "-bsp0".into(),
                format!("-o{}", out_dir.display()),
                carve_dir.join(&payload).to_string_lossy().to_string(),
            ],
            "qq_extract",
        )
        .await?;
        let _ = tokio::fs::remove_dir_all(&carve_dir).await;

        let files = out_dir.join("Files");
        if !files.join("QQ.exe").is_file() {
            return Err(ActionError::install_step(
                "qq_extract",
                "解包结果里没有 Files/QQ.exe，安装包格式可能变了",
            ));
        }
        let cfg = tokio::fs::read(files.join("versions").join("config.json"))
            .await
            .map_err(|e| {
                ActionError::install_step(
                    "qq_extract",
                    format!("读 versions/config.json 失败: {e}"),
                )
            })?;
        let version = parse_json_string_field(&cfg, "curVersion")
            .map(|v| sanitize_qq_version_for_filename(&v))
            .ok_or_else(|| {
                ActionError::install_step("qq_extract", "versions/config.json 缺 curVersion")
            })?;

        let versions_dir = qq_root.join(MANAGED_VERSIONS_DIR);
        tokio::fs::create_dir_all(&versions_dir)
            .await
            .map_err(|e| {
                ActionError::other(format!("创建 {} 失败: {e}", versions_dir.display()))
            })?;
        let dest = versions_dir.join(&version);
        remove_managed_dir(&dest).await?;
        tokio::fs::rename(&files, &dest)
            .await
            .map_err(|e| ActionError::other(format!("放置 QQ 到 {} 失败: {e}", dest.display())))?;
        tokio::fs::write(qq_root.join(MANAGED_CURRENT_FILE), format!("{version}\n"))
            .await
            .map_err(|e| ActionError::other(format!("写 current.txt 失败: {e}")))?;

        // 旧版本尽量清掉;还开着的删不动就留着,下次再清
        if let Ok(mut rd) = tokio::fs::read_dir(&versions_dir).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                if entry.file_name().to_string_lossy() != version {
                    let _ = tokio::fs::remove_dir_all(entry.path()).await;
                }
            }
        }
        Ok(version)
    }

    /// Windows uninstall:只删侧装树。只有系统 QQ 时报错说明,不去跑它的卸载器
    async fn uninstall_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        let qq_root = self.windows_qq_root();
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("remove {}", qq_root.display()),
        })
        .await;
        let versions_dir = qq_root.join(MANAGED_VERSIONS_DIR);
        let has_managed = versions_dir.is_dir() || qq_root.join(MANAGED_CURRENT_FILE).is_file();
        if !has_managed {
            if self.query_windows_install_root(host).await?.is_some() {
                return Err(ActionError::other(
                    "这台电脑上的 QQ 是你自己装的，这里不会卸载它；需要的话请在 Windows 设置里卸载",
                ));
            }
            ctx.info("没有装 QQ，无需卸载").await;
        } else {
            let _ = tokio::fs::remove_file(qq_root.join(MANAGED_CURRENT_FILE)).await;
            remove_managed_dir(&versions_dir).await?;
            clear_stale_staging(&qq_root).await;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// Windows verify:侧装 current 有 QQ.exe,或回落的系统 QQ 注册表在;再看能否读出版本号
    async fn verify_windows(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let mut report = match managed_qq_current_dir(&self.windows_qq_root()) {
            Some(dir) => VerifyReport::ok().with_check(
                "managed QQ.exe present",
                true,
                Some(dir.join("QQ.exe").display().to_string()),
            ),
            None => {
                let install_root = self.query_windows_install_root(host).await?;
                VerifyReport::ok().with_check(
                    "system QQ registry Install value present",
                    install_root.is_some(),
                    install_root.as_ref().map(|p| format!("{p}")),
                )
            }
        };
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

/// 拉 pcConfig.json 解析 Windows 段,并对 gtimg/QQNTV2 裸链做 UrlSign。
/// 返回的 download_url 已可直接下载(与 Linux 路径一致)。
async fn fetch_windows_qq_release() -> Result<(String, String), ActionError> {
    let body = fetch_text(QQ_PCCONFIG_URL).await?;
    let (version, raw_url) = parse_windows_qq_release(&body)?;
    let download_url = prepare_qq_download_url(&raw_url).await?;
    Ok((version, download_url))
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

/// gtimg / QQNTV2 等受防盗链保护的地址需要签名;dldir1 旧链直接可用。
/// Win/Linux 共用(pcConfig 两边都已切到 qqdl.gtimg.cn)。
fn qq_url_needs_sign(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("qqdl.gtimg.cn") || lower.contains("qqntv2") || lower.contains("gtimg.cn/qqfile")
}

/// 官网 UrlSign:把裸链换成 ?sign=&t= 可下载地址。
/// 签名失败不吞错,交给上层换下一条候选或直接报错。
async fn sign_qq_download_url(raw_url: &str) -> Result<String, ActionError> {
    let oidb = r#"{"uint32_command":"0x9b8e","uint32_service_type":1}"#;
    let body = serde_json::json!({ "url": raw_url });
    let resp = ncd_network::shared_client()
        .post(QQ_URL_SIGN_URL)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("x-oidb", oidb)
        .header(reqwest::header::ORIGIN, "https://im.qq.com")
        .header(reqwest::header::REFERER, "https://im.qq.com/index/")
        .json(&body)
        .send()
        .await
        .map_err(|e| ActionError::DownloadFailed {
            url: raw_url.to_string(),
            reason: format!("UrlSign request failed: {e}"),
        })?;
    if !resp.status().is_success() {
        return Err(ActionError::DownloadFailed {
            url: raw_url.to_string(),
            reason: format!("UrlSign HTTP {}", resp.status()),
        });
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| ActionError::DownloadFailed {
        url: raw_url.to_string(),
        reason: format!("UrlSign invalid json: {e}"),
    })?;
    let retcode = json.get("retcode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if retcode != 0 {
        let msg = json
            .pointer("/error/message")
            .or_else(|| json.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(ActionError::DownloadFailed {
            url: raw_url.to_string(),
            reason: format!("UrlSign retcode={retcode}: {msg}"),
        });
    }
    let signed = json
        .pointer("/data/url")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ActionError::DownloadFailed {
            url: raw_url.to_string(),
            reason: "UrlSign missing data.url".into(),
        })?;
    Ok(signed.to_string())
}

/// 安装 / probe 共用:需要签名则签名,否则原样返回。
/// Win/Linux 同一套防盗链规则。
async fn prepare_qq_download_url(url: &str) -> Result<String, ActionError> {
    if qq_url_needs_sign(url) {
        sign_qq_download_url(url).await
    } else {
        Ok(url.to_string())
    }
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
/// 对 gtimg 链直接签名,不先探裸链;返回的 download_url 已可下。
pub async fn probe_linux_qq_latest() -> Result<LinuxQqRelease, ActionError> {
    let mut last_err: Option<String> = None;
    for rel in [
        fetch_linux_qq_from_pcconfig(PackageFormat::Deb, Arch::X86_64).await,
        fetch_linux_qq_from_nclatest(PackageFormat::Deb, Arch::X86_64).await,
        QQComponent::default_v3_2_25(HostPath::from_posix("/tmp/ncd-probe"))
            .pin_linux_release(PackageFormat::Deb, Arch::X86_64),
    ] {
        match rel {
            Ok(candidate) => match prepare_qq_download_url(&candidate.download_url).await {
                Ok(prepared) => {
                    let signed = prepared != candidate.download_url;
                    let source = if signed {
                        match candidate.source {
                            "pcConfig" => "pcConfig+UrlSign",
                            "nclatest" => "nclatest+UrlSign",
                            "pin" => "pin+UrlSign",
                            other => other,
                        }
                    } else {
                        candidate.source
                    };
                    return Ok(LinuxQqRelease {
                        version: candidate.version,
                        download_url: prepared,
                        source,
                    });
                }
                Err(e) => last_err = Some(e.to_string()),
            },
            Err(e) => last_err = Some(e.to_string()),
        }
    }
    Err(ActionError::install_step(
        "probe_linux_qq_latest",
        format!(
            "没有可下载的 Linux QQ 安装包{}",
            last_err
                .map(|e| format!("; 最后错误: {e}"))
                .unwrap_or_default()
        ),
    ))
}

/// 给 release 快照用:探测 Windows QQ 当前安装包版本
///
/// 返回的 URL 已对 gtimg/QQNTV2 做过 UrlSign,可直接下载。
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
    fn sanitize_qq_version_strips_path_chars() {
        assert_eq!(sanitize_qq_version_for_filename("9.9.31"), "9.9.31");
        assert_eq!(sanitize_qq_version_for_filename(r"9.9/31\x"), "9.9_31_x");
        // 全部字符都不安全时按 unknown 处理,避免出现纯下划线的歧义文件名
        assert_eq!(sanitize_qq_version_for_filename("   "), "unknown");
    }

    #[test]
    fn windows_installer_cache_path_is_stable_across_pids() {
        let dir = HostPath::from_windows(r"C:\ProgramData\NapCatQQ Desktop\runtime\cache\qq");
        let a = windows_installer_cache_path(&dir, "9.9.31");
        let b = windows_installer_cache_path(&dir, "9.9.31");
        assert_eq!(a, b);
        assert!(a.as_posix().ends_with("QQNT-9.9.31.exe"));
        assert!(!a.as_posix().contains(&std::process::id().to_string()));
    }

    #[test]
    fn pick_embedded_7z_payload_takes_largest_7z_entry() {
        // 7zr 26.02 对 QQ_9.9.36_260924_x64_01.exe 跑 `l -t#` 的真实输出
        let listing = "\
   Date      Time    Attr         Size   Compressed  Name
------------------- ----- ------------ ------------  ------------------------
                    .....       132120       132120  1
                    .....        28447        28447  2.7z
                    .....            1            1  3
                    .....    327637756    327637756  4.7z
                    .....       855789       855789  5.7z
                    .....      1792399      1792399  6
------------------- ----- ------------ ------------  ------------------------
                             330446512    330446512  6 files
";
        assert_eq!(pick_embedded_7z_payload(listing).as_deref(), Some("4.7z"));
        assert_eq!(pick_embedded_7z_payload("no archives here"), None);
    }

    #[test]
    fn managed_qq_current_dir_requires_qq_exe() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert!(managed_qq_current_dir(root).is_none());

        std::fs::write(root.join(MANAGED_CURRENT_FILE), "9.9.36-53644\n").unwrap();
        let dir = root.join(MANAGED_VERSIONS_DIR).join("9.9.36-53644");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(managed_qq_current_dir(root).is_none());

        std::fs::write(dir.join("QQ.exe"), b"").unwrap();
        assert_eq!(managed_qq_current_dir(root), Some(dir));
    }

    #[test]
    fn managed_qq_current_dir_sanitizes_pointer() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(MANAGED_CURRENT_FILE), r"..\..\evil").unwrap();
        assert!(managed_qq_current_dir(tmp.path()).is_none());
    }

    #[test]
    fn qq_url_needs_sign_for_gtimg_and_qqntv2() {
        assert!(qq_url_needs_sign(
            "https://qqdl.gtimg.cn/qqfile/QQNTV2/9.9.32/release/9d4083e2/QQ_9.9.32_260716_x64_01.exe"
        ));
        assert!(qq_url_needs_sign(
            "https://qqdl.gtimg.cn/qqfile/QQNT/9.9.31/release/092069d7/QQ_9.9.31_260528_x64_01.exe"
        ));
        assert!(!qq_url_needs_sign(
            "https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_amd64.deb"
        ));
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

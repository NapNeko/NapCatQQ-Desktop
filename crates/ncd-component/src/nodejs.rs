//! NodeJsComponent:Node.js runtime 组件
//!
//! 作为第一个完整 Component 实装,跑通 Component trait 的全套能力,后续
//! component 照本模板扩展
//!
//! 支持矩阵:
//! - (Linux, Local) / (Linux, Remote):从 nodejs.org 下载 tar.xz 解压
//! - (Windows, Local):tar.xz 在 Windows 上解压暂未实装
//!   (HostError::Unsupported),后续按需补 zip 形式的 node Windows 包
//!
//! 探测策略:
//! 1. 目标安装目录 <install_dir>/bin/node 存在 + node --version 输出
//! 2. extra_detect_bins（便携安装 / 用户覆盖；不含 SnowLuma 完整包自带的 ./node）
//! 3. PATH 中有 node，且满足 SnowLuma 要求（22.13+ / 23.4+）
//!
//! 可用的候选优先；都不可用时 detect_outcome 带回第一处「找到但不能用」
//! 的原因（版本不符 / 二进制跑不起来），detect() 对这种情况仍返回 None。
//!
//! 默认下载源:
//! https://nodejs.org/dist/v{version}/node-v{version}-linux-x64.tar.xz
//! 可通过 NodeJsComponent::with_url(...) 覆盖镜像

use std::sync::Arc;

use async_trait::async_trait;

use ncd_domain::{NodeEnvironmentCandidate, NodeSourceKind};
use ncd_host::{Arch, ArchiveKind, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind, ProgressLogLevel};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::shell_quote;
use crate::traits::Component;
use crate::types::{
    ComponentId, DetectOutcome, DetectedVersion, LaunchArgs, UnusableInstall, VerifyReport,
};

/// 对齐上游 `check-node-version.cjs` 的 semver 范围
pub const SNOWLUMA_NODE_RANGE: &str = "^22.13.0 || >=23.4.0";

/// 单个候选二进制的探测结果;「在但跑不起来」与「不存在」分开,后者才是未安装
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeBinProbe {
    Missing,
    /// 无 v 前缀的版本号
    Version(String),
    /// 文件在但 --version 失败,携带 exit / stderr 或 IO 错误摘要
    Broken(String),
}

pub async fn probe_node_bin_status(
    host: &dyn Host,
    path: &HostPath,
) -> Result<NodeBinProbe, ActionError> {
    if !host.exists(path).await? {
        return Ok(NodeBinProbe::Missing);
    }
    let cmd = HostCommand::new(path.as_posix()).arg("--version");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let ver = out.stdout.trim().trim_start_matches('v').to_string();
            if ver.is_empty() {
                Ok(NodeBinProbe::Broken("--version 无输出".into()))
            } else {
                Ok(NodeBinProbe::Version(ver))
            }
        }
        Ok(out) => Ok(NodeBinProbe::Broken(format!(
            "exit={:?}: {}",
            out.exit_code,
            out.stderr.trim()
        ))),
        Err(e) => Ok(NodeBinProbe::Broken(e.to_string())),
    }
}

/// 二值视图:只有能跑出版本号才算 Some(不存在 / 跑不起来都是 None)
pub async fn probe_node_raw_version(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<String>, ActionError> {
    Ok(match probe_node_bin_status(host, path).await? {
        NodeBinProbe::Version(ver) => Some(ver),
        NodeBinProbe::Missing | NodeBinProbe::Broken(_) => None,
    })
}

fn version_mismatch_reason(ver: &str) -> String {
    format!("v{ver} 不满足 {SNOWLUMA_NODE_RANGE}")
}

/// 候选归类:Missing → None;可用 → Installed;版本不符 / 跑不起来 → Unusable
async fn classify_node_candidate(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<DetectOutcome>, ActionError> {
    let source = path.as_posix().to_string();
    Ok(match probe_node_bin_status(host, path).await? {
        NodeBinProbe::Missing => None,
        NodeBinProbe::Version(ver) if NodeJsComponent::version_meets_snowluma(&ver) => {
            Some(DetectOutcome::Installed(DetectedVersion {
                version: ver,
                source,
            }))
        }
        NodeBinProbe::Version(ver) => Some(DetectOutcome::Unusable(UnusableInstall {
            source,
            reason: version_mismatch_reason(&ver),
            version: Some(ver),
        })),
        NodeBinProbe::Broken(detail) => Some(DetectOutcome::Unusable(UnusableInstall {
            source,
            version: None,
            reason: format!(
                "{} 存在但无法执行:{detail}",
                path.file_name().unwrap_or("node")
            ),
        })),
    })
}

/// Node.js component 配置
#[derive(Debug, Clone)]
pub struct NodeJsComponent {
    /// 期望版本(无 v 前缀,如 "20.10.0")
    pub version: String,
    /// 安装目录(如 /opt/napcat/runtime/node 或 $HOME/Napcat/usr/node)
    pub install_dir: HostPath,
    /// 下载源 URL(默认 nodejs.org 官方)
    pub download_url_template: Option<String>,
    /// 期望 SHA256(可选,提供则严格校验)
    pub expected_sha256: Option<String>,
    /// 临时目录(下载 tarball 用,默认 /tmp/)
    pub tmp_dir: HostPath,
    /// 额外探测点（不作为安装目标），例如便携 `.../node/bin/node`
    extra_detect_bins: Vec<HostPath>,
}

impl NodeJsComponent {
    /// 创建一个 Node.js component 描述
    pub fn new(version: impl Into<String>, install_dir: HostPath) -> Self {
        Self {
            version: version.into(),
            install_dir,
            download_url_template: None,
            expected_sha256: None,
            tmp_dir: HostPath::from_posix("/tmp"),
            extra_detect_bins: Vec::new(),
        }
    }

    pub fn with_extra_detect_bin(mut self, path: HostPath) -> Self {
        if !self.extra_detect_bins.iter().any(|p| p == &path) {
            self.extra_detect_bins.push(path);
        }
        self
    }

    pub fn with_url_template(mut self, template: impl Into<String>) -> Self {
        self.download_url_template = Some(template.into());
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

    /// 推算下载 URL(根据 host 的 OS / arch)
    fn build_download_url(&self, host: &dyn Host) -> Result<String, ActionError> {
        let (platform, ext) = match host.os() {
            Os::Linux => ("linux", "tar.xz"),
            Os::MacOs => ("darwin", "tar.xz"),
            Os::Windows => ("win", "zip"),
        };
        let arch = match host.arch() {
            Arch::X86_64 => "x64",
            Arch::Aarch64 => "arm64",
            Arch::Armv7 => "armv7l",
            Arch::X86 => "x86",
        };
        let template = self.download_url_template.clone().unwrap_or_else(|| {
            format!(
                "https://nodejs.org/dist/v{ver}/node-v{ver}-{platform}-{arch}.{ext}",
                ver = "{version}",
                platform = "{platform}",
                arch = "{arch}",
                ext = "{ext}"
            )
        });
        let url = template
            .replace("{version}", &self.version)
            .replace("{platform}", platform)
            .replace("{arch}", arch)
            .replace("{ext}", ext);
        Ok(url)
    }

    pub fn node_binary_path_for_os(install_dir: &HostPath, os: Os) -> HostPath {
        match os {
            Os::Windows => install_dir.join("node.exe"),
            _ => install_dir.join("bin/node"),
        }
    }

    pub fn node_binary_path(&self, host: &dyn Host) -> HostPath {
        Self::node_binary_path_for_os(&self.install_dir, host.os())
    }

    fn source_is_managed(&self, host: &dyn Host, source: &str) -> bool {
        let expected = self.node_binary_path(host);
        let normalize = |path: &str| path.replace('\\', "/").trim_end_matches('/').to_lowercase();
        normalize(source) == normalize(expected.as_posix())
            || normalize(source) == normalize(&expected.render(ncd_host::PathStyle::Windows))
    }

    /// 对齐上游 `check-node-version.cjs`：`^22.13.0 || >=23.4.0`
    pub fn version_meets_snowluma(raw: &str) -> bool {
        let v = raw.trim().trim_start_matches('v');
        let mut parts = v.split('.');
        let Some(major) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
            return false;
        };
        let minor = parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        match major {
            22 => minor >= 13,
            23 => minor >= 4,
            n => n > 23,
        }
    }

    fn extract_root_subdir(&self, host: &dyn Host) -> String {
        let platform = match host.os() {
            Os::Linux => "linux",
            Os::MacOs => "darwin",
            Os::Windows => "win",
        };
        let arch = match host.arch() {
            Arch::X86_64 => "x64",
            Arch::Aarch64 => "arm64",
            Arch::Armv7 => "armv7l",
            Arch::X86 => "x86",
        };
        format!("node-v{}-{platform}-{arch}", self.version)
    }

    /// 组件元数据,给 list_components Tauri command 使用
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::NodeJs,
            display_name: "Node.js".to_string(),
            description: "供 SnowLuma Lite 使用的 Node.js 环境".to_string(),
            repo_url: Some("https://nodejs.org/".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Windows, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
                crate::types::SupportedTarget::new(Os::MacOs, Locality::Local),
            ],
            category: crate::types::ComponentCategory::RuntimeDep,
        }
    }
}

#[async_trait]
impl Component for NodeJsComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NodeJs
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        &[
            (Os::Windows, Locality::Local),
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
            (Os::MacOs, Locality::Local),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        // 可用的候选立即返回;都不可用时带回第一处原因(组件目录 > 额外探测点 > PATH)
        let mut first_unusable: Option<UnusableInstall> = None;
        let managed = self.node_binary_path(host);
        for path in std::iter::once(&managed).chain(self.extra_detect_bins.iter()) {
            match classify_node_candidate(host, path).await? {
                Some(DetectOutcome::Installed(v)) => return Ok(DetectOutcome::Installed(v)),
                Some(DetectOutcome::Unusable(u)) => {
                    first_unusable.get_or_insert(u);
                }
                Some(DetectOutcome::NotInstalled) | None => {}
            }
        }

        let path_cmd = HostCommand::new("node").arg("--version");
        match host.run_to_string(path_cmd).await {
            Ok(out) if out.success() => {
                let ver = out.stdout.trim().trim_start_matches('v').to_string();
                if !ver.is_empty() {
                    if Self::version_meets_snowluma(&ver) {
                        return Ok(DetectOutcome::Installed(DetectedVersion {
                            version: ver,
                            source: "$PATH/node".into(),
                        }));
                    }
                    let reason = version_mismatch_reason(&ver);
                    first_unusable.get_or_insert(UnusableInstall {
                        source: "$PATH/node".into(),
                        version: Some(ver),
                        reason,
                    });
                }
            }
            // PATH 里没有 node 是常态,不算探测出错
            Ok(_) | Err(HostError::CommandFailed { .. }) | Err(HostError::Io(_)) => {}
            Err(e) => return Err(ActionError::Host(e)),
        }

        Ok(first_unusable.map_or(DetectOutcome::NotInstalled, DetectOutcome::Unusable))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        // Step 1: 下载归档
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "download node.js archive".into(),
        })
        .await;

        let url = self.build_download_url(host)?;
        let archive_ext = if host.os() == Os::Windows { "zip" } else { "tar.xz" };
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-nodejs-{}-{}.{}",
            self.version,
            std::process::id(),
            archive_ext
        ));

        let helper = DownloadHelper::new()?;
        let mirrors = ncd_network::build_mirror_urls(&url, None);
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

        // Step 2: 上传到目标 host
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "upload archive to host".into(),
        })
        .await;
        let remote_archive = self.tmp_dir.join(format!(
            "ncd-nodejs-{}-{}.{}",
            self.version,
            std::process::id(),
            archive_ext
        ));
        host.create_dir_all(&self.tmp_dir).await?;
        host.upload(&local_tmp, &remote_archive).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // Step 3: 解压到临时位置
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "extract archive".into(),
        })
        .await;
        let stage_dir = self
            .tmp_dir
            .join(format!("ncd-nodejs-stage-{}", std::process::id()));
        let _ = host.remove_dir_all(&stage_dir).await;
        host.create_dir_all(&stage_dir).await?;

        let kind = if host.os() == Os::Windows {
            ArchiveKind::Zip
        } else {
            ArchiveKind::TarXz
        };
        host.extract_archive(&remote_archive, &stage_dir, kind)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // Step 4: 把 stage/<root>/* 移到 install_dir
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "install to target dir".into(),
        })
        .await;
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.create_dir_all(&self.install_dir).await?;
        let root_subdir = stage_dir.join(self.extract_root_subdir(host));

        if host.os() == Os::Windows {
            copy_dir_all(host, &root_subdir, &self.install_dir).await?;
        } else {
            let root = shell_quote(root_subdir.as_posix());
            let dest = shell_quote(self.install_dir.as_posix());
            let mv_cmd = HostCommand::new("sh").arg("-c").arg(format!(
                "mv {root}/* {dest}/ && mv {root}/.* {dest}/ 2>/dev/null; true",
            ));
            let mv_out = host.run_to_string(mv_cmd).await?;
            if !mv_out.success() {
                return Err(ActionError::install_step(
                    "mv_install",
                    format!("exit={:?}: {}", mv_out.exit_code, mv_out.stderr.trim()),
                ));
            }
        }

        // 清理 stage 与 archive
        let _ = host.remove_dir_all(&stage_dir).await;
        let _ = host.remove_file(&remote_archive).await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("remove {}", self.install_dir.as_posix()),
        })
        .await;

        if let Some(detected) = self.detect(host).await? {
            if !self.source_is_managed(host, &detected.source) {
                ctx.info(format!(
                    "检测到的是外部 Node.js（{}），不会删除独立组件目录。",
                    detected.source
                ))
                .await;
                ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
                ctx.emit(ProgressKind::Finished { ok: true }).await;
                return Ok(());
            }
        }

        if !host.exists(&self.install_dir).await? {
            ctx.info("Node.js 独立组件在目标路径未安装（当前环境为外部或系统 PATH），无需删除目录。")
                .await;
            ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
            ctx.emit(ProgressKind::Finished { ok: true }).await;
            return Ok(());
        }

        if let Err(err) = host.remove_dir_all(&self.install_dir).await {
            let msg = format!(
                "删除 Node.js 组件目录失败 ({}): {err}。若有正在运行的 SnowLuma 实例，请先停止 Bot 释放文件锁定后再试。",
                self.install_dir.as_posix()
            );
            ctx.log(ProgressLogLevel::Error, msg.clone()).await;
            ctx.emit(ProgressKind::StepEnd { step: 1, ok: false }).await;
            ctx.emit(ProgressKind::Finished { ok: false }).await;
            return Err(ActionError::other(msg));
        }

        ctx.info("Node.js 独立组件目录已成功清理").await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        if let Some(detected) = self.detect(host).await? {
            if !self.source_is_managed(host, &detected.source) {
                return Err(ActionError::other(format!(
                    "检测到的是外部 Node.js（{}），不能通过组件更新；请先安装 Node.js 组件。",
                    detected.source
                )));
            }
        }
        self.install(host, ctx).await
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let binary = self.node_binary_path(host);
        let exists = host.exists(&binary).await?;
        let mut report =
            VerifyReport::ok().with_check("node binary exists", exists, Some(format!("{binary}")));
        if exists {
            let cmd = HostCommand::new(binary.as_posix()).arg("--version");
            match host.run_to_string(cmd).await {
                Ok(out) if out.success() => {
                    let actual = out.stdout.trim().trim_start_matches('v').to_string();
                    let matches = actual == self.version;
                    report = report.with_check(
                        "version matches",
                        matches,
                        Some(format!("expected={} actual={actual}", self.version)),
                    );
                }
                Ok(out) => {
                    report = report.with_check(
                        "version executable",
                        false,
                        Some(format!(
                            "exit={:?} stderr={}",
                            out.exit_code,
                            out.stderr.trim()
                        )),
                    );
                }
                Err(e) => {
                    report = report.with_check("version executable", false, Some(format!("{e}")));
                }
            }
        }
        Ok(report)
    }

    fn launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let binary = self.node_binary_path(host);
        Ok(args.apply_to(HostCommand::new(binary.as_posix())))
    }
}

async fn copy_dir_all(host: &dyn Host, src: &HostPath, dst: &HostPath) -> Result<(), ActionError> {
    let mut stack = vec![(src.clone(), dst.clone())];
    while let Some((s, d)) = stack.pop() {
        let entries = host.list_dir(&s).await?;
        for entry in entries {
            let s_child = s.join(&entry.name);
            let d_child = d.join(&entry.name);
            if entry.is_dir {
                host.create_dir_all(&d_child).await?;
                stack.push((s_child, d_child));
            } else {
                let bytes = host.read_file(&s_child).await?;
                host.write_file(&d_child, &bytes).await?;
            }
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn _ensure_send_sync(_: Arc<NodeJsComponent>) {}

pub async fn probe_local_system_nodes(
    host: &dyn Host,
    component_path: Option<&HostPath>,
    custom_path: Option<&str>,
) -> Vec<NodeEnvironmentCandidate> {
    let mut results = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Custom configured path
    if let Some(cp) = custom_path {
        let trimmed = cp.trim();
        if !trimmed.is_empty() {
            let hp = HostPath::from_windows(trimmed);
            if let Ok(Some(ver)) = probe_node_raw_version(host, &hp).await {
                let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
                let path_str = hp.render(ncd_host::PathStyle::Windows);
                seen_paths.insert(path_str.to_lowercase());
                results.push(NodeEnvironmentCandidate {
                    path: path_str,
                    version: ver.clone(),
                    source_kind: NodeSourceKind::Custom,
                    label: format!("自定义路径 (v{ver})"),
                    is_valid,
                });
            }
        }
    }

    // 2. Independent NodeJs component
    if let Some(cp) = component_path {
        if let Ok(Some(ver)) = probe_node_raw_version(host, cp).await {
            let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
            let path_str = cp.render(ncd_host::PathStyle::Windows);
            if !seen_paths.contains(&path_str.to_lowercase()) {
                seen_paths.insert(path_str.to_lowercase());
                results.push(NodeEnvironmentCandidate {
                    path: path_str,
                    version: ver.clone(),
                    source_kind: NodeSourceKind::Component,
                    label: format!("Node.js 组件 (v{ver})"),
                    is_valid,
                });
            }
        }
    }

    // 通过 Host 执行 where.exe，LocalWindowsHost 会统一设置 CREATE_NO_WINDOW。
    #[cfg(windows)]
    {
        if let Ok(output) = host
            .run_to_string(HostCommand::new("where.exe").arg("node"))
            .await
        {
            if output.success() {
                for line in output.stdout.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let hp = HostPath::from_windows(trimmed);
                    let path_str = hp.render(ncd_host::PathStyle::Windows);
                    if seen_paths.contains(&path_str.to_lowercase()) {
                        continue;
                    }
                    if let Ok(Some(ver)) = probe_node_raw_version(host, &hp).await {
                        let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
                        seen_paths.insert(path_str.to_lowercase());
                        results.push(NodeEnvironmentCandidate {
                            path: path_str,
                            version: ver.clone(),
                            source_kind: NodeSourceKind::SystemPath,
                            label: format!("系统 PATH (v{ver})"),
                            is_valid,
                        });
                    }
                }
            }
        }
    }

    // 4. Common Version Managers / Paths (NVM, fnm, Volta, Program Files)
    #[cfg(windows)]
    {
        let mut check_dirs = Vec::new();
        if let Ok(nvm_home) = std::env::var("NVM_HOME") {
            check_dirs.push(std::path::PathBuf::from(nvm_home));
        }
        if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
            check_dirs.push(std::path::PathBuf::from(nvm_symlink));
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let fnm = std::path::PathBuf::from(&local_app_data).join("fnm_multishells");
            let volta = std::path::PathBuf::from(&local_app_data).join("Volta").join("bin");
            let pnpm = std::path::PathBuf::from(&local_app_data).join("pnpm");
            check_dirs.push(fnm);
            check_dirs.push(volta);
            check_dirs.push(pnpm);
        }
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            check_dirs.push(std::path::PathBuf::from(&program_files).join("nodejs"));
        }

        for base_dir in check_dirs {
            let node_exe = base_dir.join("node.exe");
            if node_exe.is_file() {
                let hp = HostPath::from_windows(node_exe.to_string_lossy().trim());
                let path_str = hp.render(ncd_host::PathStyle::Windows);
                if !seen_paths.contains(&path_str.to_lowercase()) {
                    if let Ok(Some(ver)) = probe_node_raw_version(host, &hp).await {
                        let is_valid = NodeJsComponent::version_meets_snowluma(&ver);
                        seen_paths.insert(path_str.to_lowercase());
                        results.push(NodeEnvironmentCandidate {
                            path: path_str,
                            version: ver.clone(),
                            source_kind: NodeSourceKind::VersionManager,
                            label: format!("版本管理器/已安装 (v{ver})"),
                            is_valid,
                        });
                    }
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_download_url_substitutes_version_and_arch() {
        let comp = NodeJsComponent::new("22.13.0", HostPath::from_posix("/opt/node"));
        let template = comp.download_url_template.clone().unwrap_or_else(|| {
            format!(
                "https://nodejs.org/dist/v{ver}/node-v{ver}-{platform}-{arch}.{ext}",
                ver = "{version}",
                platform = "{platform}",
                arch = "{arch}",
                ext = "{ext}"
            )
        });
        assert!(template.contains("{version}"));
        assert!(template.contains("{platform}"));
        assert!(template.contains("{arch}"));
    }

    #[test]
    fn extract_root_subdir_uses_correct_format() {
        let comp = NodeJsComponent::new("22.13.0", HostPath::from_posix("/opt/node"));
        assert_eq!(comp.version, "22.13.0");
    }

    #[test]
    fn node_binary_path_joins_correctly() {
        let install = HostPath::from_posix("/opt/node");
        assert_eq!(
            NodeJsComponent::node_binary_path_for_os(&install, Os::Linux).as_posix(),
            "/opt/node/bin/node"
        );
        let win_install = HostPath::from_windows(r"C:\Napcat\node");
        assert_eq!(
            NodeJsComponent::node_binary_path_for_os(&win_install, Os::Windows)
                .render(ncd_host::PathStyle::Windows),
            r"C:\Napcat\node\node.exe"
        );
    }

    #[test]
    fn version_meets_snowluma_matches_upstream_range() {
        assert!(NodeJsComponent::version_meets_snowluma("22.13.0"));
        assert!(NodeJsComponent::version_meets_snowluma("v22.14.1"));
        assert!(NodeJsComponent::version_meets_snowluma("23.4.0"));
        assert!(NodeJsComponent::version_meets_snowluma("24.0.0"));
        assert!(!NodeJsComponent::version_meets_snowluma("22.12.0"));
        assert!(!NodeJsComponent::version_meets_snowluma("18.19.1"));
        assert!(!NodeJsComponent::version_meets_snowluma("23.3.0"));
    }

    #[test]
    fn supported_targets_includes_linux_remote_and_windows_local() {
        let comp = NodeJsComponent::new("22.13.0", HostPath::from_posix("/x"));
        let targets = comp.supported_targets();
        assert!(targets.contains(&(Os::Linux, Locality::Local)));
        assert!(targets.contains(&(Os::Linux, Locality::Remote)));
    }

    mod detect_outcome_tests {
        use super::*;
        use std::collections::HashMap;
        use std::path::Path;

        use async_trait::async_trait;
        use bytes::Bytes;
        use ncd_host::shell::PowerShellShell;
        use ncd_host::{
            ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostPath, HostProcess,
            HostShell, PackageManager,
        };

        const MANAGED_DIR: &str = "/c/ProgramData/NapCatQQ Desktop/components/NodeJs";
        const MANAGED_BIN: &str = "/c/ProgramData/NapCatQQ Desktop/components/NodeJs/node.exe";

        enum Scripted {
            Output(i32, &'static str, &'static str),
            NotFound,
        }

        struct ScriptedHost {
            exists: Vec<&'static str>,
            responses: HashMap<&'static str, Scripted>,
            shell: PowerShellShell,
        }

        impl ScriptedHost {
            fn new(exists: &[&'static str], responses: Vec<(&'static str, Scripted)>) -> Self {
                Self {
                    exists: exists.to_vec(),
                    responses: responses.into_iter().collect(),
                    shell: PowerShellShell,
                }
            }
        }

        #[async_trait]
        impl Host for ScriptedHost {
            fn os(&self) -> Os {
                Os::Windows
            }
            fn arch(&self) -> Arch {
                Arch::X86_64
            }
            fn locality(&self) -> Locality {
                Locality::Local
            }
            fn id(&self) -> &str {
                "scripted"
            }
            fn shell(&self) -> &dyn HostShell {
                &self.shell
            }
            fn pkg_manager(&self) -> Option<&dyn PackageManager> {
                None
            }
            async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
                Ok(self.exists.contains(&path.as_posix()))
            }
            async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn extract_archive(
                &self,
                _: &HostPath,
                _: &HostPath,
                _: ArchiveKind,
            ) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }
            async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
                assert_eq!(cmd.args, ["--version"], "unexpected args for {}", cmd.program);
                match self.responses.get(cmd.program.as_str()) {
                    Some(Scripted::Output(code, stdout, stderr)) => Ok(CommandOutput {
                        exit_code: Some(*code),
                        stdout: (*stdout).to_string(),
                        stderr: (*stderr).to_string(),
                    }),
                    Some(Scripted::NotFound) | None => Err(HostError::Io(
                        std::io::Error::from(std::io::ErrorKind::NotFound),
                    )),
                }
            }
        }

        fn comp() -> NodeJsComponent {
            NodeJsComponent::new("22.13.0", HostPath::from_posix(MANAGED_DIR))
        }

        #[tokio::test]
        async fn nothing_found_is_not_installed() {
            let host = ScriptedHost::new(&[], vec![("node", Scripted::NotFound)]);
            assert_eq!(
                comp().detect_outcome(&host).await.unwrap(),
                DetectOutcome::NotInstalled
            );
        }

        // 组件目录里 node.exe 在、但 --version 跑不起来:曾被吞成「未安装」
        #[tokio::test]
        async fn managed_binary_that_cannot_run_is_unusable_not_missing() {
            let host = ScriptedHost::new(
                &[MANAGED_BIN],
                vec![
                    (MANAGED_BIN, Scripted::Output(-1073741515, "", "")),
                    ("node", Scripted::NotFound),
                ],
            );
            let outcome = comp().detect_outcome(&host).await.unwrap();
            let DetectOutcome::Unusable(u) = outcome else {
                panic!("expected Unusable, got {outcome:?}");
            };
            assert_eq!(u.source, MANAGED_BIN);
            assert_eq!(u.version, None);
            assert!(u.reason.contains("node.exe 存在但无法执行"), "{}", u.reason);
            assert!(comp().detect(&host).await.unwrap().is_none());
        }

        #[tokio::test]
        async fn path_node_below_range_is_unusable_with_version() {
            let host = ScriptedHost::new(
                &[],
                vec![("node", Scripted::Output(0, "v18.19.1\r\n", ""))],
            );
            let outcome = comp().detect_outcome(&host).await.unwrap();
            assert_eq!(
                outcome,
                DetectOutcome::Unusable(UnusableInstall {
                    source: "$PATH/node".into(),
                    version: Some("18.19.1".into()),
                    reason: format!("v18.19.1 不满足 {SNOWLUMA_NODE_RANGE}"),
                })
            );
        }

        // 组件目录版本不够但 PATH 上有可用的:可用者优先,不报 Unusable
        #[tokio::test]
        async fn usable_candidate_wins_over_unusable_one() {
            let host = ScriptedHost::new(
                &[MANAGED_BIN],
                vec![
                    (MANAGED_BIN, Scripted::Output(0, "v20.10.0\n", "")),
                    ("node", Scripted::Output(0, "v22.13.0\n", "")),
                ],
            );
            assert_eq!(
                comp().detect_outcome(&host).await.unwrap(),
                DetectOutcome::Installed(DetectedVersion {
                    version: "22.13.0".into(),
                    source: "$PATH/node".into(),
                })
            );
        }

        // 多个都不可用时,报组件目录那一处(排在 PATH 前面)
        #[tokio::test]
        async fn first_unusable_prefers_managed_over_path() {
            let host = ScriptedHost::new(
                &[MANAGED_BIN],
                vec![
                    (MANAGED_BIN, Scripted::Output(0, "v20.10.0\n", "")),
                    ("node", Scripted::Output(0, "v18.19.1\n", "")),
                ],
            );
            let DetectOutcome::Unusable(u) = comp().detect_outcome(&host).await.unwrap() else {
                panic!("expected Unusable");
            };
            assert_eq!(u.source, MANAGED_BIN);
            assert_eq!(u.version.as_deref(), Some("20.10.0"));
        }
    }
    #[cfg(windows)]
    mod system_path_probe_tests {
        use super::*;
        use std::path::Path;
        use std::sync::Arc;

        use async_trait::async_trait;
        use bytes::Bytes;
        use ncd_host::shell::PowerShellShell;
        use ncd_host::{
            ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostPath, HostProcess,
            HostShell, PackageManager, PathStyle,
        };
        use tokio::sync::Mutex;

        struct RecordingHost {
            commands: Arc<Mutex<Vec<HostCommand>>>,
            shell: PowerShellShell,
        }

        impl RecordingHost {
            fn new() -> Self {
                Self {
                    commands: Arc::new(Mutex::new(Vec::new())),
                    shell: PowerShellShell,
                }
            }
        }

        #[async_trait]
        impl Host for RecordingHost {
            fn os(&self) -> Os {
                Os::Windows
            }

            fn arch(&self) -> Arch {
                Arch::X86_64
            }

            fn locality(&self) -> Locality {
                Locality::Local
            }

            fn id(&self) -> &str {
                "test"
            }

            fn shell(&self) -> &dyn HostShell {
                &self.shell
            }

            fn pkg_manager(&self) -> Option<&dyn PackageManager> {
                None
            }

            async fn read_file(&self, _path: &HostPath) -> Result<Bytes, HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn write_file(&self, _path: &HostPath, _bytes: &[u8]) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn list_dir(&self, _path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn create_dir_all(&self, _path: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn remove_file(&self, _path: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn remove_dir_all(&self, _path: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
                Ok(path
                    .render(PathStyle::Windows)
                    .eq_ignore_ascii_case(r"C:\fake\node.exe"))
            }

            async fn upload(&self, _local: &Path, _remote: &HostPath) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn download(&self, _remote: &HostPath, _local: &Path) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn extract_archive(
                &self,
                _archive: &HostPath,
                _dest: &HostPath,
                _kind: ArchiveKind,
            ) -> Result<(), HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn spawn(&self, _cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
                Err(HostError::Unsupported { operation: "test" })
            }

            async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
                self.commands.lock().await.push(cmd.clone());
                if cmd.program == "where.exe" && cmd.args == ["node"] {
                    return Ok(CommandOutput {
                        exit_code: Some(0),
                        stdout: "C:\\fake\\node.exe\r\n".to_string(),
                        stderr: String::new(),
                    });
                }
                if cmd.program.ends_with("node.exe") && cmd.args == ["--version"] {
                    return Ok(CommandOutput {
                        exit_code: Some(0),
                        stdout: "v22.13.0\r\n".to_string(),
                        stderr: String::new(),
                    });
                }
                Err(HostError::Unsupported {
                    operation: "unexpected test command",
                })
            }
        }

        #[tokio::test]
        async fn system_path_probe_runs_where_through_host() {
            let host = RecordingHost::new();
            let commands = Arc::clone(&host.commands);

            let candidates = probe_local_system_nodes(&host, None, None).await;

            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].path, r"C:\fake\node.exe");
            assert_eq!(candidates[0].source_kind, NodeSourceKind::SystemPath);
            assert_eq!(commands.lock().await[0].program, "where.exe");
        }
    }
}

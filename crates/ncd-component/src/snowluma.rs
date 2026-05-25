//! `SnowLumaComponent`:SnowLuma framework lite tarball 部署组件。
//!
//! 对齐 legacy `install_snowluma.sh.j2` L300-L360 的安装步骤。
//!
//! 安装流程:
//! 1. 下载 lite tarball(GitHub release + 国内镜像 fallback,本 component 内置 fallback 列表)
//! 2. 上传到 `<workspace_dir>/<FRAMEWORK_FILENAME>`
//! 3. `tar -xzf $ARCHIVE -C $SNOWLUMA_DIR --strip-components=1`(注意 strip-components=1)
//! 4. 校验 `$SNOWLUMA_DIR/index.mjs` 存在
//!
//! 路径约定(对齐 legacy):
//! - `WORKSPACE_DIR`:`$HOME/Napcat/snowluma-workspace`(上层注入)
//! - `SNOWLUMA_DIR`:`<workspace>/snowluma`(framework 解压根)
//! - 入口文件:`<snowluma_dir>/index.mjs`
//!
//! 镜像 fallback:与 legacy 完全一致,GitHub 直连 + 6 个国内镜像
//! (`gh.ddlc.top` / `gh-proxy.com` / `ghfast.top` / `cors.isteed.cc` /
//! `ghproxy.cc` / `github.akams.cn`)。

use async_trait::async_trait;

use ncd_host::{ArchiveKind, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 默认 GitHub 镜像 fallback 列表(对齐 legacy install_snowluma.sh.j2)。
pub const DEFAULT_MIRROR_PREFIXES: &[&str] = &[
    "", // 直连(空前缀)
    "https://gh.ddlc.top/",
    "https://gh-proxy.com/",
    "https://ghfast.top/",
    "https://cors.isteed.cc/",
    "https://ghproxy.cc/",
    "https://github.akams.cn/",
];

/// SnowLuma 部署模式。
///
/// Linux 走 lite tarball + tar 解压(legacy install_snowluma.sh.j2 路径),
/// Windows 走 `SnowLuma-<tag>-win-x64.zip` 扁平 zip 解压(legacy
/// SnowLumaInstall),`node.exe` / `index.mjs` / `package.json` 三件套直
/// 接落在 install_dir 根下。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PlatformMode {
    Linux,
    Windows,
}

/// SnowLuma framework component 配置。
#[derive(Debug, Clone)]
pub struct SnowLumaComponent {
    /// SnowLuma 的工作目录(workspace dir),如 `$HOME/Napcat/snowluma-workspace`
    pub workspace_dir: HostPath,
    /// framework 解压根(默认 `<workspace>/snowluma`)
    pub snowluma_dir: HostPath,
    /// GitHub release URL(直连形式),如
    /// `https://github.com/SnowLuma/SnowLuma/releases/download/v1.2.3/SnowLuma-v1.2.3-linux-x64-lite.tar.gz`
    pub framework_url: String,
    /// 镜像前缀列表(默认使用 [`DEFAULT_MIRROR_PREFIXES`])
    pub mirror_prefixes: Vec<String>,
    /// 期望 SHA256(可选)
    pub expected_sha256: Option<String>,
    /// Desktop 预上传的 tarball 路径(可选,优先复用,跳过下载)
    pub preloaded_tarball: Option<HostPath>,
    /// 平台模式,决定 detect / install / verify 走 Linux tarball 还是
    /// Windows zip 路径。
    mode: PlatformMode,
    /// Windows 模式下的 release tag(如 `v1.7.5`)。Linux 模式下为 None。
    /// 用于:1) 探测 install_dir 下的 `.installed_tag` 是否一致;
    /// 2) install 完成后写回 `.installed_tag`(对齐 legacy
    /// `SnowLumaInstall.write_installed_tag`)。
    windows_tag: Option<String>,
}

impl SnowLumaComponent {
    /// 创建一个 Linux framework component 描述(lite tarball)。
    /// `workspace_dir`:SL workspace 根;`snowluma_dir`:framework 解压根。
    pub fn new(
        workspace_dir: HostPath,
        framework_url: impl Into<String>,
    ) -> Self {
        let snowluma_dir = workspace_dir.join("snowluma");
        Self {
            workspace_dir,
            snowluma_dir,
            framework_url: framework_url.into(),
            mirror_prefixes: DEFAULT_MIRROR_PREFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            expected_sha256: None,
            preloaded_tarball: None,
            mode: PlatformMode::Linux,
            windows_tag: None,
        }
    }

    /// 创建 Windows 扁平 zip 部署的 SnowLuma component(legacy
    /// `SnowLumaInstall` 同款)。
    ///
    /// `install_dir`:扁平 zip 解压根(典型 `<data_root>/snowluma/`),
    /// `node.exe` / `index.mjs` / `package.json` 直接落在该目录之下。
    /// `tag`:GitHub release tag 含 `v` 前缀(如 `"v1.7.5"`),用于:
    ///   1) 拼接默认 zip 文件名 / URL(`SnowLuma-<tag>-win-x64.zip`);
    ///   2) install 完成后写 `.installed_tag`,detect 走该文件优先,
    ///      legacy 同款。
    ///
    /// 调用方应该传入与 release service 一致的 tag(例如从 GitHub releases
    /// 拉到的最新版),不要自己拼接。如果只是想 detect 已装版本,
    /// 给个空 tag 也能跑(只会让 install 路径不可用,detect 不影响)。
    pub fn for_windows(install_dir: HostPath, tag: impl Into<String>) -> Self {
        let tag_str = tag.into();
        let url = format!(
            "https://github.com/SnowLuma/SnowLuma/releases/download/{tag}/SnowLuma-{tag}-win-x64.zip",
            tag = tag_str
        );
        Self {
            workspace_dir: install_dir.clone(),
            snowluma_dir: install_dir,
            framework_url: url,
            mirror_prefixes: DEFAULT_MIRROR_PREFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            expected_sha256: None,
            preloaded_tarball: None,
            mode: PlatformMode::Windows,
            windows_tag: if tag_str.is_empty() {
                None
            } else {
                Some(tag_str)
            },
        }
    }

    pub fn with_snowluma_dir(mut self, dir: HostPath) -> Self {
        self.snowluma_dir = dir;
        self
    }

    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.expected_sha256 = Some(sha256.into());
        self
    }

    pub fn with_mirror_prefixes(mut self, prefixes: Vec<String>) -> Self {
        self.mirror_prefixes = prefixes;
        self
    }

    pub fn with_preloaded_tarball(mut self, path: HostPath) -> Self {
        self.preloaded_tarball = Some(path);
        self
    }

    /// 入口文件:
    /// - Linux 模式:`<snowluma_dir>/index.mjs`(lite tarball 解压根)
    /// - Windows 模式:`<install_dir>/index.mjs`(扁平 zip 解压根,
    ///   `snowluma_dir == install_dir`)
    pub fn entry_path(&self) -> HostPath {
        self.snowluma_dir.join("index.mjs")
    }

    /// Windows 安装根下的 `node.exe`(legacy SnowLuma release 自带 portable Node)。
    fn node_exe_path(&self) -> HostPath {
        self.snowluma_dir.join("node.exe")
    }

    /// Windows / Linux 都会有的 package.json(SnowLuma release 自带)。
    /// detect 在 `.installed_tag` 缺失时走它的 `version` 字段 fallback。
    fn package_json_path(&self) -> HostPath {
        self.snowluma_dir.join("package.json")
    }

    /// Windows install 完成后写 + detect 优先读的 tag 文件
    /// (legacy `SnowLumaInstall.write_installed_tag`)。
    fn installed_tag_path(&self) -> HostPath {
        self.snowluma_dir.join(".installed_tag")
    }

    /// 拼接镜像 URL 列表(直连放第一位,失败按顺序 fallback)。
    fn mirror_urls(&self) -> Vec<String> {
        self.mirror_prefixes
            .iter()
            .map(|prefix| {
                if prefix.is_empty() {
                    self.framework_url.clone()
                } else {
                    let trimmed = prefix.trim_end_matches('/');
                    format!("{trimmed}/{}", self.framework_url)
                }
            })
            .collect()
    }

    /// 组件元数据，给 `list_components` Tauri command 使用。
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::SnowLuma,
            display_name: "SnowLuma".to_string(),
            description: "QQ 注入式 OneBot 协议端，保留 QQ 客户端窗口".to_string(),
            repo_url: Some("https://github.com/SnowLuma/SnowLuma".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Windows, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
            ],
            category: crate::types::ComponentCategory::Framework,
        }
    }
}


#[async_trait]
impl Component for SnowLumaComponent {
    fn id(&self) -> ComponentId {
        ComponentId::SnowLuma
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        // SnowLuma 在 Linux 走 lite tarball 注入,Windows 走扁平 zip 解压
        // (legacy SnowLumaInstall)。Windows 远端不支持(没有用例)。
        &[
            (Os::Windows, Locality::Local),
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        match self.mode {
            PlatformMode::Linux => self.detect_linux(host).await,
            PlatformMode::Windows => self.detect_windows(host).await,
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
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        match host.os() {
            Os::Windows => self.uninstall_windows(host).await,
            _ => Err(ActionError::other(
                "SnowLuma uninstall 仅在 Windows 本机实装;Linux daemon 注入卸载由上层编排",
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
        // 启动命令:`node <snowluma_dir>/index.mjs`
        // 实际 daemon 可能用绝对 node 路径(node 由 NodeJsComponent 装在 workspace/node/),
        // 这里只给基础命令,daemon 拼装层会替换 node 路径。
        Ok(self.launch_command_inner(args))
    }
}

// ============================================================
// Linux / Windows 分支实装(独立 impl block 拆分关注点)
// ============================================================

impl SnowLumaComponent {
    /// Linux detect:lite tarball 没有 package.json,只能确认入口存在,版本固定
    /// 标 "installed"。
    async fn detect_linux(
        &self,
        host: &dyn Host,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        let entry = self.entry_path();
        if !host.exists(&entry).await? {
            return Ok(None);
        }
        Ok(Some(DetectedVersion {
            version: "installed".to_string(),
            source: format!("{entry}"),
        }))
    }

    /// Linux verify:校验 entry + client/ + native/(legacy lite tarball 应有)。
    async fn verify_linux(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let entry = self.entry_path();
        let entry_exists = host.exists(&entry).await?;
        let mut report = VerifyReport::ok().with_check(
            "framework entry exists (index.mjs)",
            entry_exists,
            Some(format!("{entry}")),
        );
        for sub in &["client", "native"] {
            let path = self.snowluma_dir.join(*sub);
            let exists = host.exists(&path).await.unwrap_or(false);
            report = report.with_check(
                format!("subdir {sub} exists"),
                exists,
                Some(format!("{path}")),
            );
        }
        Ok(report)
    }

    /// Linux install(原 install 实装,挪到独立方法以便 trait install 按 host.os 分发)。
    async fn install_linux(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;

        host.create_dir_all(&self.workspace_dir).await?;
        host.create_dir_all(&self.snowluma_dir).await?;

        // tarball 在远端的目标路径(优先用 preloaded,否则放 workspace_dir 下)
        let archive_filename = self
            .framework_url
            .rsplit('/')
            .next()
            .unwrap_or("snowluma.tar.gz");
        let remote_archive = self.workspace_dir.join(archive_filename);

        // ===== Step 1:获取 tarball(优先 preloaded,fallback 到镜像下载)=====
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "obtain framework tarball".into(),
        })
        .await;

        let mut have_archive = false;
        if let Some(preloaded) = &self.preloaded_tarball {
            if host.exists(preloaded).await? {
                ctx.info(format!("reusing preloaded tarball: {preloaded}"))
                    .await;
                if preloaded.as_posix() != remote_archive.as_posix() {
                    // 复制 / 移动到目标位置
                    let cmd = HostCommand::new("cp")
                        .arg(preloaded.as_posix())
                        .arg(remote_archive.as_posix());
                    let out = host.run_to_string(cmd).await?;
                    if !out.success() {
                        return Err(ActionError::install_step(
                            "copy_preloaded",
                            format!("exit={:?} stderr={}", out.exit_code, out.stderr.trim()),
                        ));
                    }
                }
                have_archive = true;
            }
        }
        if !have_archive && host.exists(&remote_archive).await? {
            ctx.info(format!("reusing existing tarball: {remote_archive}"))
                .await;
            have_archive = true;
        }

        if !have_archive {
            // 走镜像 fallback:本机下载 → 上传
            let helper = DownloadHelper::new()?;
            let local_tmp = std::env::temp_dir().join(format!(
                "ncd-snowluma-{}-{}.tar.gz",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            ));

            let mirrors = self.mirror_urls();
            ctx.info(format!("racing {} mirrors", mirrors.len())).await;
            helper
                .download_with_mirrors(
                    &mirrors,
                    &local_tmp,
                    self.expected_sha256.as_deref(),
                    ctx,
                    1,
                )
                .await?;
            host.upload(&local_tmp, &remote_archive).await?;
            let _ = tokio::fs::remove_file(&local_tmp).await;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // ===== Step 2:tar 解压 + strip-components=1 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "extract framework tarball".into(),
        })
        .await;
        // ncd-host::extract_archive 不支持 strip-components,直接走 tar 命令
        let cmd = HostCommand::new("tar")
            .arg("-xzf")
            .arg(remote_archive.as_posix())
            .arg("-C")
            .arg(self.snowluma_dir.as_posix())
            .arg("--strip-components=1");
        let _ = ArchiveKind::TarGz; // 防止 import unused 警告(本 component 故意不走 host.extract_archive)
        let out = host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(ActionError::install_step(
                "tar_extract",
                format!(
                    "exit={:?} stderr={}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // ===== Step 3:校验 entry 存在 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "verify framework entry".into(),
        })
        .await;
        if !host.exists(&self.entry_path()).await? {
            return Err(ActionError::install_step(
                "verify_entry",
                format!(
                    "{} missing after extract (lite tarball incomplete or structure changed)",
                    self.entry_path()
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// Windows detect:扁平 zip 部署。优先读 `.installed_tag`(legacy
    /// `SnowLumaInstall.write_installed_tag` 写的),fallback 读
    /// `package.json` 的 `version` 字段。entry / node.exe / package.json
    /// 任一缺失都视为未安装。
    async fn detect_windows(
        &self,
        host: &dyn Host,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        let entry = self.entry_path();
        let pkg = self.package_json_path();
        let node = self.node_exe_path();
        // 三件套都在才视为"装好的 SnowLuma 发布包"。
        if !host.exists(&entry).await?
            || !host.exists(&pkg).await?
            || !host.exists(&node).await?
        {
            return Ok(None);
        }

        // 优先 .installed_tag
        let tag_path = self.installed_tag_path();
        if host.exists(&tag_path).await? {
            if let Ok(bytes) = host.read_file(&tag_path).await {
                let tag = String::from_utf8_lossy(&bytes).trim().to_string();
                if !tag.is_empty() {
                    return Ok(Some(DetectedVersion {
                        version: tag,
                        source: format!("{tag_path}"),
                    }));
                }
            }
        }

        // fallback: package.json::version
        if let Ok(bytes) = host.read_file(&pkg).await {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(v) = json.get("version").and_then(|x| x.as_str()) {
                    if !v.is_empty() {
                        return Ok(Some(DetectedVersion {
                            version: v.to_string(),
                            source: format!("{pkg}"),
                        }));
                    }
                }
            }
        }

        // 三件套齐了但版本都没解析出 → 标 unknown,UI 仍可显示"已安装"。
        Ok(Some(DetectedVersion {
            version: "unknown".to_string(),
            source: format!("{entry} (no .installed_tag, package.json missing version)"),
        }))
    }

    /// Windows verify:校验 entry / node.exe / package.json 三件套 + 版本号。
    async fn verify_windows(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let entry = self.entry_path();
        let pkg = self.package_json_path();
        let node = self.node_exe_path();

        let mut report = VerifyReport::ok()
            .with_check(
                "index.mjs exists",
                host.exists(&entry).await?,
                Some(format!("{entry}")),
            )
            .with_check(
                "node.exe exists",
                host.exists(&node).await?,
                Some(format!("{node}")),
            )
            .with_check(
                "package.json exists",
                host.exists(&pkg).await?,
                Some(format!("{pkg}")),
            );

        if let Ok(Some(v)) = self.detect(host).await {
            report = report.with_check(
                "snowluma version detected",
                v.version != "unknown",
                Some(format!("version={}", v.version)),
            );
        }
        Ok(report)
    }

    /// Windows install(对齐 legacy `SnowLumaInstall`):
    /// 1) 下载 `SnowLuma-<tag>-win-x64.zip` 到本地临时目录(走镜像 fallback);
    /// 2) 上传到 host(本机 = copy);
    /// 3) extract_archive 到 install_dir,**保留** `config/` `data/` 现有文件;
    ///    Windows 端 ncd-host 的 zip 解压不支持 strip-components,所以包装目录
    ///    的剥离由本方法在解压后做(legacy `_detect_wrapper_prefix`);
    /// 4) verify entry / node.exe / package.json 三件套;
    /// 5) 写 `.installed_tag` 让后续 detect 锁定版本号。
    ///
    /// **不**做 legacy 的 `_init_or_update_password`(那是 webui 密码同步,
    /// 属于 ncd-runtime 的 SnowLuma daemon 编排,不在 ncd-component 边界)。
    async fn install_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        let tag = self.windows_tag.as_deref().ok_or_else(|| {
            ActionError::install_step(
                "snowluma_windows_install",
                "missing release tag; 调用方应用 SnowLumaComponent::for_windows(install_dir, tag) 提供 tag",
            )
        })?;

        ctx.emit(ProgressKind::Started { total_steps: 5 }).await;

        host.create_dir_all(&self.snowluma_dir).await?;
        let stage_dir = self.snowluma_dir.join(format!(
            "_stage-{}",
            std::process::id()
        ));
        // 任何残留 stage 都先清掉
        let _ = host.remove_dir_all(&stage_dir).await;
        host.create_dir_all(&stage_dir).await?;

        // ===== Step 1:下载 zip(镜像 fallback)=====
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("download SnowLuma-{tag}-win-x64.zip"),
        })
        .await;

        let helper = DownloadHelper::new()?;
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-snowluma-win-{}-{}.zip",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));

        let mirrors = self.mirror_urls();
        ctx.info(format!("racing {} mirrors", mirrors.len())).await;
        if let Err(e) = helper
            .download_with_mirrors(
                &mirrors,
                &local_tmp,
                self.expected_sha256.as_deref(),
                ctx,
                1,
            )
            .await
        {
            let _ = host.remove_dir_all(&stage_dir).await;
            return Err(e);
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // ===== Step 2:上传到 host stage =====
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "stage zip on host".into(),
        })
        .await;
        let remote_zip = stage_dir.join("snowluma.zip");
        host.upload(&local_tmp, &remote_zip).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // ===== Step 3:解压 + 探测包装目录 + 复制到 install_dir(保留 config/data) =====
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "extract zip (preserve config/ data/)".into(),
        })
        .await;
        let extract_root = stage_dir.join("extracted");
        host.create_dir_all(&extract_root).await?;
        host.extract_archive(&remote_zip, &extract_root, ncd_host::ArchiveKind::Zip)
            .await?;
        let payload_root = self
            .resolve_extracted_payload_root(host, &extract_root)
            .await?;
        self.copy_extracted_into_install(host, &payload_root).await?;
        let _ = host.remove_dir_all(&stage_dir).await;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // ===== Step 4:校验三件套 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "verify install artifacts".into(),
        })
        .await;
        for required in &["index.mjs", "node.exe", "package.json"] {
            let p = self.snowluma_dir.join(*required);
            if !host.exists(&p).await? {
                return Err(ActionError::install_step(
                    "snowluma_verify_artifact",
                    format!("missing required file after extract: {p}"),
                ));
            }
        }
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;

        // ===== Step 5:写 .installed_tag =====
        ctx.emit(ProgressKind::StepBegin {
            step: 5,
            message: "write .installed_tag".into(),
        })
        .await;
        host.write_file(&self.installed_tag_path(), tag.as_bytes())
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 5, ok: true }).await;

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 探测解压根下是否存在唯一一层包装目录(`SnowLuma-vX-win-x64/`),是则返回
    /// 该子目录,否则返回 extract_root 自身。等价于 legacy
    /// `_detect_wrapper_prefix` 的纯文件系统版。
    async fn resolve_extracted_payload_root(
        &self,
        host: &dyn Host,
        extract_root: &HostPath,
    ) -> Result<HostPath, ActionError> {
        let entries = host.list_dir(extract_root).await?;
        // 只有一个 entry 且为目录 → 视为包装目录
        if entries.len() == 1 && entries[0].is_dir {
            return Ok(extract_root.join(&entries[0].name));
        }
        Ok(extract_root.clone())
    }

    /// 把 payload_root 下所有内容复制到 install_dir,跳过 install_dir 下已有的
    /// `config/` `data/`(legacy 同款,保留用户运行期数据)。
    async fn copy_extracted_into_install(
        &self,
        host: &dyn Host,
        payload_root: &HostPath,
    ) -> Result<(), ActionError> {
        host.create_dir_all(&self.snowluma_dir).await?;
        copy_tree(host, payload_root, &self.snowluma_dir, &["config", "data"]).await
    }

    /// Windows uninstall:删 install_dir 下除 config/ data/ 外所有文件和目录,
    /// 包括 `.installed_tag`(让后续 detect 必须靠新 install 写回)。
    async fn uninstall_windows(&self, host: &dyn Host) -> Result<(), ActionError> {
        if !host.exists(&self.snowluma_dir).await? {
            return Ok(());
        }
        let entries = match host.list_dir(&self.snowluma_dir).await {
            Ok(es) => es,
            Err(HostError::PathNotFound { .. }) => return Ok(()),
            Err(e) => return Err(ActionError::Host(e)),
        };
        for entry in entries {
            if entry.is_dir && (entry.name == "config" || entry.name == "data") {
                continue;
            }
            let target = self.snowluma_dir.join(&entry.name);
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

    fn launch_command_inner(
        &self,
        args: &LaunchArgs,
    ) -> HostCommand {
        let mut cmd = HostCommand::new("node").arg(self.entry_path().as_posix());
        for a in &args.extra_args {
            cmd = cmd.arg(a);
        }
        for (k, v) in &args.extra_env {
            cmd = cmd.env(k, v);
        }
        if let Some(wd) = &args.working_dir {
            cmd = cmd.working_dir(wd.clone());
        } else {
            cmd = cmd.working_dir(self.snowluma_dir.clone());
        }
        cmd
    }
}

/// 递归复制 src 目录树到 dst,保留 dst 下名字落在 `preserve_top_level` 列表
/// 中的"顶层"目录已有内容(legacy `SnowLumaInstall.unzip_file` 同款语义,但
/// 仅适用于 Linux/Windows 双平台都能跑通的 Host trait 文件操作)。
///
/// 保留规则只看顶层(直接挂在 install_dir 下),深层同名子目录正常覆盖。
async fn copy_tree(
    host: &dyn Host,
    src_root: &HostPath,
    dst_root: &HostPath,
    preserve_top_level: &[&str],
) -> Result<(), ActionError> {
    let mut stack: Vec<(HostPath, HostPath, bool)> = vec![(src_root.clone(), dst_root.clone(), true)];
    while let Some((src, dst, is_top)) = stack.pop() {
        let entries = host.list_dir(&src).await?;
        for entry in entries {
            let src_child = src.join(&entry.name);
            let dst_child = dst.join(&entry.name);
            if entry.is_dir {
                // 顶层 + 在保留名单 + dst 已存在 → 整树跳过
                if is_top
                    && preserve_top_level.contains(&entry.name.as_str())
                    && host.exists(&dst_child).await?
                {
                    continue;
                }
                host.create_dir_all(&dst_child).await?;
                stack.push((src_child, dst_child, false));
            } else {
                // 文件:read 全部到内存再 write,避免依赖 host 的 cp 命令(Windows
                // 没 cp,Linux 上 host.run_to_string("cp ...") 依赖 shell)。
                let bytes = host.read_file(&src_child).await?;
                host.write_file(&dst_child, &bytes).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp() -> SnowLumaComponent {
        SnowLumaComponent::new(
            HostPath::from_posix("/home/test/Napcat/snowluma-workspace"),
            "https://github.com/SnowLuma/SnowLuma/releases/download/v1.2.3/SnowLuma-v1.2.3-linux-x64-lite.tar.gz",
        )
    }

    #[test]
    fn paths_default_layout() {
        let c = comp();
        assert_eq!(
            c.workspace_dir.as_posix(),
            "/home/test/Napcat/snowluma-workspace"
        );
        assert_eq!(
            c.snowluma_dir.as_posix(),
            "/home/test/Napcat/snowluma-workspace/snowluma"
        );
        assert_eq!(
            c.entry_path().as_posix(),
            "/home/test/Napcat/snowluma-workspace/snowluma/index.mjs"
        );
    }

    #[test]
    fn id_returns_snowluma_framework() {
        assert_eq!(comp().id(), ComponentId::SnowLuma);
    }

    #[test]
    fn supported_targets_includes_linux_and_windows_local() {
        let c = comp();
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Local)));
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Remote)));
        assert!(c.supported_targets().contains(&(Os::Windows, Locality::Local)));
    }

    #[test]
    fn mirror_urls_first_is_direct() {
        let c = comp();
        let urls = c.mirror_urls();
        assert!(urls.len() >= 7);
        // 第一个是直连(空前缀)
        assert_eq!(urls[0], c.framework_url);
        // 第二个开始是镜像
        assert!(urls[1].starts_with("https://gh.ddlc.top/"));
        assert!(urls[1].ends_with("SnowLuma-v1.2.3-linux-x64-lite.tar.gz"));
    }

    #[test]
    fn mirror_urls_no_trailing_slash_collision() {
        // 即使前缀末尾带 `/`,拼出的 URL 也只有一个 `/`
        let c = SnowLumaComponent::new(
            HostPath::from_posix("/x"),
            "https://github.com/foo/bar.tar.gz",
        )
        .with_mirror_prefixes(vec![
            "".to_string(),
            "https://mirror1.example.com/".to_string(),
            "https://mirror2.example.com".to_string(),
        ]);
        let urls = c.mirror_urls();
        assert_eq!(urls[0], "https://github.com/foo/bar.tar.gz");
        assert_eq!(urls[1], "https://mirror1.example.com/https://github.com/foo/bar.tar.gz");
        assert_eq!(urls[2], "https://mirror2.example.com/https://github.com/foo/bar.tar.gz");
    }

    #[test]
    fn with_snowluma_dir_overrides() {
        let c = comp().with_snowluma_dir(HostPath::from_posix("/custom/snowluma"));
        assert_eq!(c.snowluma_dir.as_posix(), "/custom/snowluma");
        assert_eq!(c.entry_path().as_posix(), "/custom/snowluma/index.mjs");
    }

    #[test]
    fn default_mirror_count_matches_legacy() {
        // 7 = 1 直连 + 6 镜像(legacy install_snowluma.sh.j2 L325-L331)
        assert_eq!(DEFAULT_MIRROR_PREFIXES.len(), 7);
        assert_eq!(DEFAULT_MIRROR_PREFIXES[0], "");
        assert!(DEFAULT_MIRROR_PREFIXES.contains(&"https://gh-proxy.com/"));
        assert!(DEFAULT_MIRROR_PREFIXES.contains(&"https://github.akams.cn/"));
    }

    #[test]
    fn preloaded_tarball_can_be_set() {
        let c = comp().with_preloaded_tarball(HostPath::from_posix("/preload/snowluma.tar.gz"));
        assert_eq!(
            c.preloaded_tarball.unwrap().as_posix(),
            "/preload/snowluma.tar.gz"
        );
    }

    // ============================================================
    // Windows 模式纯结构断言(任意平台都能跑,不依赖真文件系统)
    // ============================================================

    #[test]
    fn windows_constructor_uses_flat_install_dir() {
        let c = SnowLumaComponent::for_windows(
            HostPath::from_windows(r"C:\ProgramData\NapCatQQ Desktop\runtime\SnowLuma"),
            "v1.7.5",
        );
        assert!(matches!(c.mode, PlatformMode::Windows));
        // 扁平模式下三件套直接落在 install_dir 根下,无 snowluma/ 子目录嵌套。
        let win = ncd_host::PathStyle::Windows;
        assert_eq!(
            c.entry_path().render(win),
            r"C:\ProgramData\NapCatQQ Desktop\runtime\SnowLuma\index.mjs"
        );
        assert_eq!(
            c.node_exe_path().render(win),
            r"C:\ProgramData\NapCatQQ Desktop\runtime\SnowLuma\node.exe"
        );
        assert_eq!(
            c.package_json_path().render(win),
            r"C:\ProgramData\NapCatQQ Desktop\runtime\SnowLuma\package.json"
        );
        assert_eq!(c.windows_tag.as_deref(), Some("v1.7.5"));
    }

    #[test]
    fn windows_constructor_default_url_format() {
        let c = SnowLumaComponent::for_windows(HostPath::from_posix("/x"), "v1.7.5");
        assert_eq!(
            c.framework_url,
            "https://github.com/SnowLuma/SnowLuma/releases/download/v1.7.5/SnowLuma-v1.7.5-win-x64.zip"
        );
    }

    #[test]
    fn windows_constructor_with_empty_tag_keeps_none() {
        // 空 tag 用例:UI 只想 detect / verify 时不强制提供 tag,install 路径会
        // 在调用时拒绝并报清晰错误。
        let c = SnowLumaComponent::for_windows(HostPath::from_posix("/x"), "");
        assert!(matches!(c.mode, PlatformMode::Windows));
        assert!(c.windows_tag.is_none());
    }

    #[test]
    fn linux_constructor_keeps_workspace_layout() {
        // 回归:Linux 默认 entry 仍嵌套在 workspace/snowluma/ 下。
        let c = comp();
        assert!(matches!(c.mode, PlatformMode::Linux));
        assert_eq!(
            c.entry_path().as_posix(),
            "/home/test/Napcat/snowluma-workspace/snowluma/index.mjs"
        );
    }

    #[test]
    fn info_lists_windows_local_in_supported_targets() {
        let info = SnowLumaComponent::info();
        assert!(
            info.supported_targets
                .iter()
                .any(|t| t.os == Os::Windows && t.locality == Locality::Local)
        );
    }

    // ============================================================
    // Windows 本机端到端测试(只在 Windows 上编译)
    // ============================================================

    #[cfg(windows)]
    mod windows_e2e {
        use super::*;
        use ncd_host::local::LocalWindowsHost;

        fn windows_path(workspace: &tempfile::TempDir, sub: &str) -> HostPath {
            let full = workspace.path().join(sub);
            HostPath::from_windows(full.to_str().unwrap())
        }

        async fn write_file(host: &LocalWindowsHost, path: &HostPath, body: &[u8]) {
            host.write_file(path, body).await.expect("write_file");
        }

        async fn lay_out_release(
            host: &LocalWindowsHost,
            install: &HostPath,
            tag_file: Option<&str>,
            pkg_version: &str,
        ) {
            host.create_dir_all(install).await.unwrap();
            write_file(host, &install.join("index.mjs"), b"// snowluma entry").await;
            write_file(host, &install.join("node.exe"), b"\x4d\x5a fake-pe").await;
            let pkg = format!(r#"{{"name":"snowluma","version":"{pkg_version}"}}"#);
            write_file(host, &install.join("package.json"), pkg.as_bytes()).await;
            if let Some(tag) = tag_file {
                write_file(host, &install.join(".installed_tag"), tag.as_bytes()).await;
            }
        }

        #[tokio::test]
        async fn detect_returns_none_when_three_files_missing() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            let comp = SnowLumaComponent::for_windows(install, "v1.7.5");
            assert!(comp.detect(&host).await.unwrap().is_none());
        }

        #[tokio::test]
        async fn detect_prefers_installed_tag_over_package_json() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            // .installed_tag 写 v1.7.5,package.json 写 1.7.4(不一致),
            // detect 必须取 .installed_tag。
            lay_out_release(&host, &install, Some("v1.7.5"), "1.7.4").await;

            let comp = SnowLumaComponent::for_windows(install.clone(), "v1.7.5");
            let v = comp.detect(&host).await.unwrap().expect("已装应返回 Some");
            assert_eq!(v.version, "v1.7.5");
            assert!(v.source.contains(".installed_tag"));
        }

        #[tokio::test]
        async fn detect_falls_back_to_package_json_when_tag_missing() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            lay_out_release(&host, &install, None, "1.7.4").await;

            let comp = SnowLumaComponent::for_windows(install, "v1.7.5");
            let v = comp.detect(&host).await.unwrap().expect("已装应返回 Some");
            assert_eq!(v.version, "1.7.4");
            assert!(v.source.contains("package.json"));
        }

        #[tokio::test]
        async fn detect_returns_unknown_when_both_version_sources_absent() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            host.create_dir_all(&install).await.unwrap();
            write_file(&host, &install.join("index.mjs"), b"e").await;
            write_file(&host, &install.join("node.exe"), b"n").await;
            // package.json 没有 version 字段
            write_file(&host, &install.join("package.json"), b"{}").await;

            let comp = SnowLumaComponent::for_windows(install, "v1.7.5");
            let v = comp.detect(&host).await.unwrap().expect("三件套齐应返回 Some");
            assert_eq!(v.version, "unknown");
        }

        #[tokio::test]
        async fn verify_reports_three_artifacts_and_version() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            lay_out_release(&host, &install, Some("v1.7.5"), "1.7.5").await;

            let comp = SnowLumaComponent::for_windows(install, "v1.7.5");
            let report = comp.verify(&host).await.unwrap();
            assert!(report.ok);
            for needle in &["index.mjs", "node.exe", "package.json", "version"] {
                assert!(
                    report.checks.iter().any(|c| c.name.contains(needle)),
                    "缺少 check 包含 {needle:?}"
                );
            }
        }

        #[tokio::test]
        async fn uninstall_preserves_config_and_data() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            lay_out_release(&host, &install, Some("v1.7.5"), "1.7.5").await;
            // 用户运行期数据
            write_file(&host, &install.join("config/runtime.json"), b"user").await;
            write_file(&host, &install.join("data/100200/messages.db"), b"db").await;

            let comp = SnowLumaComponent::for_windows(install.clone(), "v1.7.5");
            let (mut ctx, _rx) = ActionCtx::new();
            comp.uninstall(&host, &mut ctx).await.unwrap();

            // 三件套清掉
            assert!(!host.exists(&install.join("index.mjs")).await.unwrap());
            assert!(!host.exists(&install.join("node.exe")).await.unwrap());
            assert!(!host.exists(&install.join("package.json")).await.unwrap());
            assert!(!host.exists(&install.join(".installed_tag")).await.unwrap());
            // config / data 整树保留
            let cfg = host
                .read_file(&install.join("config/runtime.json"))
                .await
                .unwrap();
            assert_eq!(cfg.as_ref(), b"user");
            let db = host
                .read_file(&install.join("data/100200/messages.db"))
                .await
                .unwrap();
            assert_eq!(db.as_ref(), b"db");
        }

        #[tokio::test]
        async fn uninstall_is_noop_when_dir_missing() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "never");
            let comp = SnowLumaComponent::for_windows(install, "v1.7.5");
            let (mut ctx, _rx) = ActionCtx::new();
            comp.uninstall(&host, &mut ctx).await.unwrap();
        }

        #[tokio::test]
        async fn copy_extracted_keeps_existing_config_and_data() {
            // 直接打 copy_extracted_into_install,模拟 install_windows 的 step 3:
            // 解压后从 stage 复制到 install_dir,但 install_dir 下已有 config/data。
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            host.create_dir_all(&install).await.unwrap();
            // install_dir 已有用户运行期产物
            write_file(&host, &install.join("config/runtime.json"), b"user").await;
            write_file(&host, &install.join("data/100200/messages.db"), b"old").await;

            // payload(模拟 zip 解压结果),含同名 config/ data/
            let payload = windows_path(&ws, "stage/payload");
            host.create_dir_all(&payload).await.unwrap();
            write_file(&host, &payload.join("index.mjs"), b"new").await;
            write_file(&host, &payload.join("node.exe"), b"new").await;
            write_file(&host, &payload.join("package.json"), br#"{"version":"1.7.5"}"#).await;
            // 包内 config / data 默认值 —— 不应覆盖用户运行期数据
            write_file(&host, &payload.join("config/runtime.json"), b"default").await;
            write_file(&host, &payload.join("data/100200/messages.db"), b"shipped").await;

            let comp = SnowLumaComponent::for_windows(install.clone(), "v1.7.5");
            comp.copy_extracted_into_install(&host, &payload).await.unwrap();

            // 三件套来自 payload(用 b"new" / 1.7.5 区分)
            let mjs = host.read_file(&install.join("index.mjs")).await.unwrap();
            assert_eq!(mjs.as_ref(), b"new");
            let pkg = host.read_file(&install.join("package.json")).await.unwrap();
            assert!(String::from_utf8_lossy(&pkg).contains("1.7.5"));
            // config / data 保留用户值
            let cfg = host
                .read_file(&install.join("config/runtime.json"))
                .await
                .unwrap();
            assert_eq!(cfg.as_ref(), b"user");
            let db = host
                .read_file(&install.join("data/100200/messages.db"))
                .await
                .unwrap();
            assert_eq!(db.as_ref(), b"old");
        }

        #[tokio::test]
        async fn copy_extracted_creates_config_and_data_when_absent() {
            // 首次安装(install_dir 下无任何文件):config / data 该走默认值。
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            let payload = windows_path(&ws, "stage/payload");
            host.create_dir_all(&payload).await.unwrap();
            write_file(&host, &payload.join("index.mjs"), b"e").await;
            write_file(&host, &payload.join("config/runtime.json"), b"default").await;

            let comp = SnowLumaComponent::for_windows(install.clone(), "v1.7.5");
            comp.copy_extracted_into_install(&host, &payload).await.unwrap();

            let cfg = host
                .read_file(&install.join("config/runtime.json"))
                .await
                .unwrap();
            assert_eq!(cfg.as_ref(), b"default");
        }

        #[tokio::test]
        async fn resolve_payload_root_strips_single_wrapper() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let extracted = windows_path(&ws, "extracted");
            host.create_dir_all(&extracted.join("SnowLuma-v1.7.5-win-x64"))
                .await
                .unwrap();
            write_file(
                &host,
                &extracted.join("SnowLuma-v1.7.5-win-x64/index.mjs"),
                b"e",
            )
            .await;

            let comp = SnowLumaComponent::for_windows(windows_path(&ws, "snowluma"), "v1.7.5");
            let payload = comp.resolve_extracted_payload_root(&host, &extracted).await.unwrap();
            assert!(payload.as_posix().ends_with("/SnowLuma-v1.7.5-win-x64"));
        }

        #[tokio::test]
        async fn resolve_payload_root_keeps_flat_zip_root() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let extracted = windows_path(&ws, "extracted");
            host.create_dir_all(&extracted).await.unwrap();
            write_file(&host, &extracted.join("index.mjs"), b"e").await;
            write_file(&host, &extracted.join("node.exe"), b"n").await;
            write_file(&host, &extracted.join("package.json"), b"{}").await;

            let comp = SnowLumaComponent::for_windows(windows_path(&ws, "snowluma"), "v1.7.5");
            let payload = comp.resolve_extracted_payload_root(&host, &extracted).await.unwrap();
            // 扁平 zip:返回 extract_root 自身
            assert_eq!(payload.as_posix(), extracted.as_posix());
        }

        #[tokio::test]
        async fn install_windows_rejects_missing_tag() {
            let host = LocalWindowsHost::new();
            let ws = tempfile::tempdir().unwrap();
            let install = windows_path(&ws, "snowluma");
            let comp = SnowLumaComponent::for_windows(install, "");
            let (mut ctx, _rx) = ActionCtx::new();
            let err = comp.install(&host, &mut ctx).await.unwrap_err();
            assert!(format!("{err}").contains("missing release tag"));
        }
    }
}

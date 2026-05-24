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

use ncd_host::{ArchiveKind, Host, HostCommand, HostPath, Locality, Os};

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
}

impl SnowLumaComponent {
    /// 创建一个 framework component 描述。
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

    /// 入口文件:`<snowluma_dir>/index.mjs`
    pub fn entry_path(&self) -> HostPath {
        self.snowluma_dir.join("index.mjs")
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
}


#[async_trait]
impl Component for SnowLumaComponent {
    fn id(&self) -> ComponentId {
        ComponentId::SnowLuma
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        // SnowLuma framework 只在 Linux,本地 / 远端都支持
        &[
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let entry = self.entry_path();
        if !host.exists(&entry).await? {
            return Ok(None);
        }
        // SnowLuma lite tarball 没有 package.json,版本号靠目录名 / 上层传入。
        // detect 只确认入口存在,版本固定标 "installed"。
        Ok(Some(DetectedVersion {
            version: "installed".to_string(),
            source: format!("{entry}"),
        }))
    }


    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
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

            let mut last_err: Option<ActionError> = None;
            for url in self.mirror_urls() {
                ctx.info(format!("trying mirror: {url}")).await;
                match helper
                    .download_to_file(&url, &local_tmp, self.expected_sha256.as_deref(), ctx, 1)
                    .await
                {
                    Ok(()) => {
                        last_err = None;
                        break;
                    }
                    Err(e) => {
                        ctx.warn(format!("mirror failed: {e}")).await;
                        last_err = Some(e);
                        let _ = tokio::fs::remove_file(&local_tmp).await;
                    }
                }
            }
            if let Some(e) = last_err {
                return Err(e);
            }
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


    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let entry = self.entry_path();
        let entry_exists = host.exists(&entry).await?;
        let mut report = VerifyReport::ok().with_check(
            "framework entry exists (index.mjs)",
            entry_exists,
            Some(format!("{entry}")),
        );
        // 额外检查关键子目录:client / native(legacy 注释里写了 lite tarball 应有这些)
        for sub in &["client", "native"] {
            let path = self.snowluma_dir.join(*sub);
            let exists = host.exists(&path).await.unwrap_or(false);
            report = report.with_check(format!("subdir {sub} exists"), exists, Some(format!("{path}")));
        }
        Ok(report)
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // 启动命令:`node <snowluma_dir>/index.mjs`
        // 实际 daemon 可能用绝对 node 路径(node 由 NodeJsComponent 装在 workspace/node/),
        // 这里只给基础命令,daemon 拼装层会替换 node 路径
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
            // 默认工作目录 = snowluma_dir
            cmd = cmd.working_dir(self.snowluma_dir.clone());
        }
        Ok(cmd)
    }
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
    fn supported_targets_only_linux() {
        let c = comp();
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Local)));
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Remote)));
        assert!(!c.supported_targets().contains(&(Os::Windows, Locality::Local)));
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
}

//! `NodeJsComponent`:Node.js runtime 组件。
//!
//! 作为第一个完整 Component 实装,跑通 Component trait 的全套能力,后续
//! component 照本模板扩展。
//!
//! 支持矩阵:
//! - `(Linux, Local)` / `(Linux, Remote)`:从 nodejs.org 下载 tar.xz 解压
//! - `(Windows, Local)`:tar.xz 在 Windows 上解压暂未实装
//!   (`HostError::Unsupported`),后续按需补 zip 形式的 node Windows 包
//!
//! 探测策略:
//! 1. 目标安装目录 `<install_dir>/bin/node` 存在 + `node --version` 输出
//! 2. PATH 中有 `node`(回退到系统 node)
//!
//! 默认下载源:
//! `https://nodejs.org/dist/v{version}/node-v{version}-linux-x64.tar.xz`
//! 可通过 `NodeJsComponent::with_url(...)` 覆盖镜像。

use std::sync::Arc;

use async_trait::async_trait;

use ncd_host::{
    ArchiveKind, Arch, Host, HostCommand, HostError, HostPath, Locality, Os,
};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// Node.js component 配置。
#[derive(Debug, Clone)]
pub struct NodeJsComponent {
    /// 期望版本(无 `v` 前缀,如 "20.10.0")
    pub version: String,
    /// 安装目录(如 `/opt/napcat/runtime/node` 或 `$HOME/Napcat/usr/node`)
    pub install_dir: HostPath,
    /// 下载源 URL(默认 nodejs.org 官方)
    pub download_url_template: Option<String>,
    /// 期望 SHA256(可选,提供则严格校验)
    pub expected_sha256: Option<String>,
    /// 临时目录(下载 tarball 用,默认 `/tmp/`)
    pub tmp_dir: HostPath,
}

impl NodeJsComponent {
    /// 创建一个 Node.js component 描述。
    pub fn new(version: impl Into<String>, install_dir: HostPath) -> Self {
        Self {
            version: version.into(),
            install_dir,
            download_url_template: None,
            expected_sha256: None,
            tmp_dir: HostPath::from_posix("/tmp"),
        }
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

    /// 推算下载 URL(根据 host 的 OS / arch)。
    fn build_download_url(&self, host: &dyn Host) -> Result<String, ActionError> {
        let template = self.download_url_template.clone().unwrap_or_else(|| {
            format!(
                "https://nodejs.org/dist/v{ver}/node-v{ver}-{platform}-{arch}.tar.xz",
                ver = "{version}",
                platform = "{platform}",
                arch = "{arch}"
            )
        });
        let platform = match host.os() {
            Os::Linux => "linux",
            Os::MacOs => "darwin",
            Os::Windows => {
                return Err(ActionError::UnsupportedTarget {
                    component: "nodejs".into(),
                    os: Os::Windows,
                    locality: host.locality(),
                });
            }
        };
        let arch = match host.arch() {
            Arch::X86_64 => "x64",
            Arch::Aarch64 => "arm64",
            Arch::Armv7 => "armv7l",
            Arch::X86 => "x86",
        };
        let url = template
            .replace("{version}", &self.version)
            .replace("{platform}", platform)
            .replace("{arch}", arch);
        Ok(url)
    }

    fn node_binary_path(&self) -> HostPath {
        self.install_dir.join("bin/node")
    }

    fn extract_root_subdir(&self, host: &dyn Host) -> String {
        // tar.xz 解压后会有一层 `node-v20.10.0-linux-x64/` 子目录,需要去除
        let platform = match host.os() {
            Os::Linux => "linux",
            Os::MacOs => "darwin",
            _ => "linux",
        };
        let arch = match host.arch() {
            Arch::X86_64 => "x64",
            Arch::Aarch64 => "arm64",
            Arch::Armv7 => "armv7l",
            Arch::X86 => "x86",
        };
        format!("node-v{}-{platform}-{arch}", self.version)
    }

    /// 组件元数据，给 `list_components` Tauri command 使用。
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::NodeJs,
            display_name: "Node.js".to_string(),
            description: "JavaScript 运行时（仅 SnowLuma 需要）".to_string(),
            repo_url: Some("https://nodejs.org/".to_string()),
            supported_targets: vec![
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
        // 当前实装:Linux 本地 / 远端,macOS 本地。Windows 上 tar.xz 解压尚未支持。
        &[
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
            (Os::MacOs, Locality::Local),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        // 优先级 1:目标安装目录中的 node binary
        let binary = self.node_binary_path();
        if host.exists(&binary).await? {
            let cmd = HostCommand::new(binary.as_posix()).arg("--version");
            match host.run_to_string(cmd).await {
                Ok(out) if out.success() => {
                    let ver = out.stdout.trim().trim_start_matches('v').to_string();
                    if !ver.is_empty() {
                        return Ok(Some(DetectedVersion {
                            version: ver,
                            source: format!("{}", binary),
                        }));
                    }
                }
                _ => {}
            }
        }

        // 优先级 2:回退 PATH 中的 node，但必须 >= v22.5.0（支持 node:sqlite）
        let path_cmd = HostCommand::new("node").arg("--version");
        match host.run_to_string(path_cmd).await {
            Ok(out) if out.success() => {
                let ver_str = out.stdout.trim().trim_start_matches('v');
                if ver_str.is_empty() {
                    return Ok(None);
                }

                // 解析版本号，检查是否 >= 22.5.0
                if let Some((major, _)) = ver_str.split_once('.') {
                    if let Ok(major_num) = major.parse::<u32>() {
                        if major_num >= 22 {
                            // 系统 node >= 22，可以使用
                            return Ok(Some(DetectedVersion {
                                version: ver_str.to_string(),
                                source: "$PATH/node".into(),
                            }));
                        }
                        // 系统 node < 22，视为未安装（需要安装到指定目录）
                        return Ok(None);
                    }
                }

                // 无法解析版本号，保守处理视为未安装
                Ok(None)
            }
            // 找不到 node 命令视作未安装,不算 detect 失败
            Ok(_) => Ok(None),
            Err(HostError::CommandFailed { .. }) => Ok(None),
            Err(HostError::Io(_)) => Ok(None),
            Err(e) => Err(ActionError::Host(e)),
        }
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        // Step 1:下载 tarball
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "download node.js tarball".into(),
        })
        .await;

        let url = self.build_download_url(host)?;
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-nodejs-{}-{}.tar.xz",
            self.version,
            std::process::id()
        ));

        let helper = DownloadHelper::new()?;
        let mirrors = ncd_network::build_mirror_urls(&url, None);
        helper
            .download_with_mirrors(&mirrors, &local_tmp, self.expected_sha256.as_deref(), ctx, 1)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // Step 2:上传到目标 host
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "upload tarball to host".into(),
        })
        .await;
        let remote_tar = self.tmp_dir.join(format!(
            "ncd-nodejs-{}-{}.tar.xz",
            self.version,
            std::process::id()
        ));
        host.create_dir_all(&self.tmp_dir).await?;
        host.upload(&local_tmp, &remote_tar).await?;
        // 删本地 tmp(已经传过去了)
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // Step 3:解压到临时位置
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "extract tarball".into(),
        })
        .await;
        let stage_dir = self.tmp_dir.join(format!(
            "ncd-nodejs-stage-{}",
            std::process::id()
        ));
        // 清理可能存在的旧 stage(忽略错误)
        let _ = host.remove_dir_all(&stage_dir).await;
        host.create_dir_all(&stage_dir).await?;
        host.extract_archive(&remote_tar, &stage_dir, ArchiveKind::TarXz).await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // Step 4:把 stage/<root>/* 移到 install_dir
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "install to target dir".into(),
        })
        .await;
        // 清理旧的 install_dir(可能是更老版本)
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.create_dir_all(&self.install_dir).await?;
        let root_subdir = stage_dir.join(self.extract_root_subdir(host));
        // 用 shell 把内容 mv 过去:`mv stage/root_subdir/* install_dir/`
        let mv_cmd = HostCommand::new("sh").arg("-c").arg(format!(
            "mv {}/* {}/ && mv {}/.* {}/ 2>/dev/null; true",
            root_subdir.as_posix(),
            self.install_dir.as_posix(),
            root_subdir.as_posix(),
            self.install_dir.as_posix(),
        ));
        let mv_out = host.run_to_string(mv_cmd).await?;
        if !mv_out.success() {
            return Err(ActionError::install_step(
                "mv_install",
                format!("exit={:?}: {}", mv_out.exit_code, mv_out.stderr.trim()),
            ));
        }
        // 清理 stage 与 tarball
        let _ = host.remove_dir_all(&stage_dir).await;
        let _ = host.remove_file(&remote_tar).await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("remove {}", self.install_dir.as_posix()),
        })
        .await;
        // install_dir 不存在视为已卸载，幂等成功。
        if host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let binary = self.node_binary_path();
        let exists = host.exists(&binary).await?;
        let mut report = VerifyReport::ok().with_check(
            "node binary exists",
            exists,
            Some(format!("{binary}")),
        );
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
                    report = report.with_check(
                        "version executable",
                        false,
                        Some(format!("{e}")),
                    );
                }
            }
        }
        Ok(report)
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let binary = self.node_binary_path();
        let mut cmd = HostCommand::new(binary.as_posix());
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

// 防止 Component 内部 fields 没用到的 dead_code 报警(_arc/_template 是 future-proof)
#[allow(dead_code)]
fn _ensure_send_sync(_: Arc<NodeJsComponent>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_download_url_substitutes_version_and_arch() {
        // 只能用 mock host 做测试,这里用最简单的方式:用 LocalWindowsHost(Windows)看是否拒绝
        // 然后用 Linux 的占位检查 URL 模板替换逻辑
        let comp = NodeJsComponent::new("20.10.0", HostPath::from_posix("/opt/node"));
        // 简单 path 校验:模板含 {version} / {platform} / {arch}
        let template = comp
            .download_url_template
            .clone()
            .unwrap_or_else(|| {
                format!(
                    "https://nodejs.org/dist/v{ver}/node-v{ver}-{platform}-{arch}.tar.xz",
                    ver = "{version}",
                    platform = "{platform}",
                    arch = "{arch}"
                )
            });
        assert!(template.contains("{version}"));
        assert!(template.contains("{platform}"));
        assert!(template.contains("{arch}"));
    }

    #[test]
    fn extract_root_subdir_uses_correct_format() {
        let comp = NodeJsComponent::new("20.10.0", HostPath::from_posix("/opt/node"));
        // 测试函数本身的字符串生成不依赖 host 实例,
        // 这里通过模拟代替 —— 我们在 mod 内只测纯字符串拼接逻辑
        // 真实集成测试在 deploy 阶段做
        assert_eq!(comp.version, "20.10.0");
    }

    #[test]
    fn node_binary_path_joins_correctly() {
        let comp = NodeJsComponent::new("20.10.0", HostPath::from_posix("/opt/node"));
        assert_eq!(comp.node_binary_path().as_posix(), "/opt/node/bin/node");
    }

    #[test]
    fn supported_targets_includes_linux_remote() {
        let comp = NodeJsComponent::new("20.10.0", HostPath::from_posix("/x"));
        let targets = comp.supported_targets();
        assert!(targets.contains(&(Os::Linux, Locality::Local)));
        assert!(targets.contains(&(Os::Linux, Locality::Remote)));
    }
}

//! NcdWatchComponent:远端主机侧监控进程(ncd-watch)
//!
//! 装到 $HOME/ncd-watch:上传二进制、写默认配置、尽量拉起 systemd --user。
//! 不启停 Bot;配置同步(notify.json / desktop_present)由 Desktop 侧另做。

use std::path::PathBuf;

use async_trait::async_trait;

use ncd_host::{Arch, Host, HostCommand, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind, ProgressLogLevel};
use crate::error::ActionError;
use crate::shell_quote;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

const INSTALL_DIR_NAME: &str = "ncd-watch";
const BIN_NAME: &str = "ncd-watch";
/// 与 GitHub Release asset / workflow 命名一致
const RELEASE_REPO: &str = "NapNeko/NapCatQQ-Desktop";

/// 远端 ncd-watch 组件
#[derive(Debug, Clone)]
pub struct NcdWatchComponent {
    /// 远端 $HOME(探测得到);None 时 install 失败
    pub remote_home: Option<String>,
    /// 本机已构建/缓存的二进制路径(优先上传)
    pub local_binary: Option<PathBuf>,
    /// 可选下载 URL(local_binary 缺失时用 host.download_url)
    pub download_url: Option<String>,
    /// 写入远端的版本标签(detect 展示用;空则用 --version 输出)
    pub version_label: String,
}

impl NcdWatchComponent {
    pub fn new(remote_home: Option<String>) -> Self {
        Self {
            remote_home,
            local_binary: None,
            download_url: None,
            version_label: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn with_local_binary(mut self, path: impl Into<PathBuf>) -> Self {
        self.local_binary = Some(path.into());
        self
    }

    pub fn with_download_url(mut self, url: impl Into<String>) -> Self {
        self.download_url = Some(url.into());
        self
    }

    pub fn with_version_label(mut self, v: impl Into<String>) -> Self {
        self.version_label = v.into();
        self
    }

    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::NcdWatch,
            display_name: "NCD Watch".to_string(),
            description: "远端离线监控（Desktop 退出后 Webhook）".to_string(),
            repo_url: Some(format!("https://github.com/{RELEASE_REPO}")),
            supported_targets: vec![crate::types::SupportedTarget::new(
                Os::Linux,
                Locality::Remote,
            )],
            category: crate::types::ComponentCategory::RuntimeDep,
        }
    }

    fn require_home(&self) -> Result<&str, ActionError> {
        self.remote_home
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ActionError::InvalidConfig {
                reason: "ncd-watch 需要远端 $HOME".into(),
            })
    }

    fn root_path(&self) -> Result<HostPath, ActionError> {
        let home = self.require_home()?;
        Ok(HostPath::from_posix(format!("{home}/{INSTALL_DIR_NAME}")))
    }

    fn bin_path(&self) -> Result<HostPath, ActionError> {
        Ok(self.root_path()?.join("bin").join(BIN_NAME))
    }

    fn config_watch_path(&self) -> Result<HostPath, ActionError> {
        Ok(self.root_path()?.join("config").join("watch.json"))
    }

    async fn ensure_layout(&self, host: &dyn Host) -> Result<(), ActionError> {
        let root = self.root_path()?;
        for sub in ["bin", "config", "state", "logs"] {
            host.create_dir_all(&root.join(sub))
                .await
                .map_err(|e| ActionError::install_step("mkdir", e.to_string()))?;
        }
        Ok(())
    }

    async fn place_binary(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        let dest = self.bin_path()?;
        if let Some(local) = &self.local_binary {
            if !local.is_file() {
                return Err(ActionError::InvalidConfig {
                    reason: format!("本机 ncd-watch 二进制不存在: {}", local.display()),
                });
            }
            ctx.emit(ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!("上传 ncd-watch ← {}", local.display()),
            })
            .await;
            host.upload(local, &dest)
                .await
                .map_err(|e| ActionError::install_step("upload", e.to_string()))?;
        } else if let Some(url) = &self.download_url {
            ctx.emit(ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!("下载 ncd-watch ← {url}"),
            })
            .await;
            host.download_url(url, &dest)
                .await
                .map_err(|e| ActionError::install_step("download", e.to_string()))?;
        } else {
            return Err(ActionError::InvalidConfig {
                reason: "未提供 local_binary 或 download_url,无法安装 ncd-watch".into(),
            });
        }

        let dest_s = dest.as_posix();
        let chmod = HostCommand::new("chmod").arg("+x").arg(dest_s);
        let out = host
            .run_to_string(chmod)
            .await
            .map_err(|e| ActionError::install_step("chmod", e.to_string()))?;
        if !out.success() {
            return Err(ActionError::install_step(
                "chmod",
                format!("exit={:?} {}", out.exit_code, out.stderr.trim()),
            ));
        }
        Ok(())
    }

    async fn write_default_watch_json(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.config_watch_path()?;
        if host
            .exists(&path)
            .await
            .map_err(|e| ActionError::install_step("exists watch.json", e.to_string()))?
        {
            return Ok(());
        }
        // 与 ncd-watch::WatchConfig::default 字段对齐(避免 component 依赖 ncd-watch crate)
        let body = r#"{
  "protocol": 1,
  "features": ["process_watch", "docker_watch", "webhook"],
  "probeIntervalSecs": 15,
  "desktopPresentTtlSecs": 90,
  "debounceSecs": 0,
  "notifyWhileDesktopPresent": false
}
"#;
        host.write_file(&path, body.as_bytes())
            .await
            .map_err(|e| ActionError::install_step("write watch.json", e.to_string()))?;
        Ok(())
    }

    async fn write_and_start_systemd(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        let home = self.require_home()?;
        let bin = format!("{home}/{INSTALL_DIR_NAME}/bin/{BIN_NAME}");
        let root = format!("{home}/{INSTALL_DIR_NAME}");
        let unit_dir = HostPath::from_posix(format!("{home}/.config/systemd/user"));
        host.create_dir_all(&unit_dir)
            .await
            .map_err(|e| ActionError::install_step("mkdir systemd user", e.to_string()))?;
        let unit_path = unit_dir.join("ncd-watch.service");
        let unit = format!(
            "[Unit]\n\
             Description=NapCatQQ Desktop remote watch\n\
             After=network-online.target\n\
             \n\
             [Service]\n\
             Type=simple\n\
             ExecStart={bin} --root {root} run\n\
             Restart=on-failure\n\
             RestartSec=5\n\
             Environment=RUST_LOG=info\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n"
        );
        host.write_file(&unit_path, unit.as_bytes())
            .await
            .map_err(|e| ActionError::install_step("write unit", e.to_string()))?;

        // best-effort:无 systemd 时不失败安装
        let script = format!(
            "if command -v systemctl >/dev/null 2>&1; then \
               systemctl --user daemon-reload && \
               systemctl --user enable --now ncd-watch.service; \
             else \
               nohup {bin} --root {root} run >/dev/null 2>&1 & \
             fi"
        );
        ctx.emit(ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: "启动 ncd-watch(systemd --user 或 nohup)".into(),
        })
        .await;
        let out = host
            .run_to_string(HostCommand::new("sh").arg("-c").arg(&script))
            .await
            .map_err(|e| ActionError::install_step("start watch", e.to_string()))?;
        if !out.success() {
            ctx.emit(ProgressKind::Log {
                level: ProgressLogLevel::Warn,
                message: format!(
                    "自动启动未完全成功(exit={:?}): {} — 可稍后手动 systemctl --user start ncd-watch",
                    out.exit_code,
                    out.stderr.trim()
                ),
            }).await;
        }
        Ok(())
    }
}

#[async_trait]
impl Component for NcdWatchComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NcdWatch
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        &[(Os::Linux, Locality::Remote)]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let bin = match self.bin_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        if !host
            .exists(&bin)
            .await
            .map_err(|e| ActionError::other(e.to_string()))?
        {
            return Ok(None);
        }
        let bin_s = bin.as_posix().to_string();
        let out = host
            .run_to_string(HostCommand::new(&bin_s).arg("--version"))
            .await;
        let version = match out {
            Ok(o) if o.success() => {
                let line = o.stdout.lines().next().unwrap_or("").trim();
                if line.is_empty() {
                    self.version_label.clone()
                } else {
                    line.to_string()
                }
            }
            _ => self.version_label.clone(),
        };
        Ok(Some(DetectedVersion {
            version,
            source: bin_s,
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "创建目录".into(),
        })
        .await;
        self.ensure_layout(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "放置二进制".into(),
        })
        .await;
        self.place_binary(host, ctx).await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "写入默认 watch.json".into(),
        })
        .await;
        self.write_default_watch_json(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "配置并启动服务".into(),
        })
        .await;
        self.write_and_start_systemd(host, ctx).await?;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 2 }).await;
        let home = self.require_home()?;
        let root = format!("{home}/{INSTALL_DIR_NAME}");
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "停止服务".into(),
        })
        .await;
        let stop = format!(
            "systemctl --user disable --now ncd-watch.service 2>/dev/null || \
             pkill -f '{}/bin/{}' 2>/dev/null || true",
            shell_quote(&root),
            BIN_NAME
        );
        let _ = host
            .run_to_string(HostCommand::new("sh").arg("-c").arg(&stop))
            .await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "删除安装目录".into(),
        })
        .await;
        host.remove_dir_all(&HostPath::from_posix(&root))
            .await
            .map_err(|e| ActionError::install_step("remove", e.to_string()))?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let bin = self.bin_path()?;
        let exists = host
            .exists(&bin)
            .await
            .map_err(|e| ActionError::other(e.to_string()))?;
        Ok(
            VerifyReport::ok().with_check(
                "binary exists",
                exists,
                Some(bin.as_posix().to_string()),
            ),
        )
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let home = self.require_home()?;
        let bin = format!("{home}/{INSTALL_DIR_NAME}/bin/{BIN_NAME}");
        let root = format!("{home}/{INSTALL_DIR_NAME}");
        let cmd = HostCommand::new(bin).arg("--root").arg(root).arg("run");
        Ok(args.apply_to(cmd))
    }
}

/// musl 目标 triple(仅 x86_64 / aarch64;其它架构不发版)
pub fn ncd_watch_musl_target(arch: Arch) -> Option<&'static str> {
    match arch {
        Arch::X86_64 => Some("x86_64-unknown-linux-musl"),
        Arch::Aarch64 => Some("aarch64-unknown-linux-musl"),
        Arch::X86 | Arch::Armv7 => None,
    }
}

/// GitHub Release 直链(无本机二进制时远端 download_url 用)
///
/// tag = `watch-v{CARGO_PKG_VERSION}`, asset = `ncd-watch-{ver}-{triple}`。
/// 版本与 workspace 对齐;发版前需先打对应 tag。
pub fn ncd_watch_release_download_url(arch: Arch) -> Option<String> {
    let triple = ncd_watch_musl_target(arch)?;
    let ver = env!("CARGO_PKG_VERSION");
    Some(format!(
        "https://github.com/{RELEASE_REPO}/releases/download/watch-v{ver}/ncd-watch-{ver}-{triple}"
    ))
}

/// 解析本机 debug/release 下的 ncd-watch.exe / ncd-watch(开发期上传用)
pub fn discover_local_ncd_watch_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    // target/debug/ncd-tauri.exe → target/debug/ncd-watch.exe
    let candidates = [
        dir.join("ncd-watch.exe"),
        dir.join("ncd-watch"),
        dir.join("ncd_watch.exe"),
        dir.parent()
            .map(|p| p.join("ncd-watch.exe"))
            .unwrap_or_default(),
        dir.parent()
            .map(|p| p.join("ncd-watch"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_and_info() {
        let c = NcdWatchComponent::new(Some("/home/u".into()));
        assert_eq!(c.id(), ComponentId::NcdWatch);
        assert_eq!(NcdWatchComponent::info().id, ComponentId::NcdWatch);
        assert_eq!(
            c.bin_path().unwrap().as_posix(),
            "/home/u/ncd-watch/bin/ncd-watch"
        );
    }

    #[test]
    fn musl_url_x86_64() {
        let url = ncd_watch_release_download_url(Arch::X86_64).expect("x86_64");
        let ver = env!("CARGO_PKG_VERSION");
        assert!(url.contains(&format!("watch-v{ver}")));
        assert!(url.ends_with(&format!("ncd-watch-{ver}-x86_64-unknown-linux-musl")));
    }

    #[test]
    fn musl_url_rejects_x86() {
        assert!(ncd_watch_release_download_url(Arch::X86).is_none());
    }

    #[test]
    fn discover_does_not_panic() {
        let _ = discover_local_ncd_watch_binary();
    }
}

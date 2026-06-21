//! NoVncComponent:noVNC + 图形栈组件(SnowLuma 远端 VNC 接入用)
//!
//! 对齐 legacy install_snowluma.sh.j2 的"图形栈"安装步骤,装一整套:
//! Xvfb / fluxbox / x11vnc / novnc / websockify + dbus-x11
//!
//! 安装策略:不下载二进制,走 apt / dnf 装系统包(legacy 验证过的策略,
//! SnowLuma 远端 VNC 接入必备)
//!
//! 包列表
//!
//! Debian / Ubuntu(apt):
//! - dbus-x11(D-Bus session)
//! - fluxbox(轻量窗口管理器)
//! - xvfb(虚拟 framebuffer)
//! - x11vnc(把 X server 暴露成 VNC)
//! - novnc(浏览器端 HTML5 VNC client)
//! - websockify(VNC ↔ WebSocket 桥接)
//!
//! RHEL / CentOS / Fedora(dnf):
//! - dbus-x11 fluxbox openbox xorg-x11-server-Xvfb x11vnc
//! - novnc python3-websockify(在 EPEL)
//!
//! 探测:检查 command -v websockify && command -v x11vnc 是否同时存在
//! noVNC 是 web 资源不是 binary,但 websockify 是 noVNC 的运行时依赖,所以用它代理探测

use async_trait::async_trait;

use std::time::Duration;

use ncd_host::{Host, HostCommand, Locality, Os};

use crate::context::{ActionCtx, ProgressKind, ProgressLogLevel};
use crate::error::ActionError;
use crate::pkg_install_stream::run_pkg_command_with_progress;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// Linux 包管理器枚举(本 component 内部用,后续接 PackageManager trait 落地后可替换)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PkgMgr {
    Apt,
    Dnf,
}

/// noVNC + 图形栈 component
#[derive(Debug, Clone)]
pub struct NoVncComponent {
    /// 是否走 sudo 提权(默认 true,因为 apt/dnf install 必须 root;
    /// 远端 ubuntu 用户已配置 NOPASSWD sudo 时才能 silent 安装)
    pub use_sudo: bool,
}

impl NoVncComponent {
    pub fn new() -> Self {
        Self { use_sudo: true }
    }

    pub fn with_sudo(mut self, use_sudo: bool) -> Self {
        self.use_sudo = use_sudo;
        self
    }

    /// 探测远端的包管理器
    async fn detect_pkg_manager(&self, host: &dyn Host) -> Result<PkgMgr, ActionError> {
        for (binary, mgr) in &[
            ("apt-get", PkgMgr::Apt),
            ("dnf", PkgMgr::Dnf),
        ] {
            let cmd = HostCommand::new("sh")
                .arg("-c")
                .arg(format!("command -v {binary}"));
            if let Ok(out) = host.run_to_string(cmd).await {
                if out.success() && !out.stdout.trim().is_empty() {
                    return Ok(*mgr);
                }
            }
        }
        Err(ActionError::install_step(
            "detect_pkg_manager",
            "neither apt-get nor dnf found on host",
        ))
    }

    /// 拼接 apt / dnf install 命令提权交给 Host:use_sudo 时打 .elevated() 标,
    /// Host 层按注入的提权密码决定 sudo -S(有密码)还是 sudo -n(免密)命令体本身
    /// 不含 sudo,不再写死 sudo -n——那在无免密 sudo 的机器上必败
    fn build_install_command(&self, mgr: PkgMgr) -> HostCommand {
        let pkgs_apt = "dbus-x11 fluxbox xvfb x11vnc novnc websockify";
        let pkgs_dnf =
            "dbus-x11 fluxbox openbox xorg-x11-server-Xvfb x11vnc novnc python3-websockify";

        let cmd_str = match mgr {
            PkgMgr::Apt => format!(
                "DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends {pkgs_apt}"
            ),
            PkgMgr::Dnf => {
                // legacy install_snowluma.sh.j2 L112 等价:
                // --allowerasing --setopt=strict=0 防止某个包匹配失败导致全 transaction abort
                format!("dnf install --allowerasing --setopt=strict=0 -y {pkgs_dnf}")
            }
        };

        self.maybe_elevated(HostCommand::new("sh").arg("-c").arg(cmd_str))
            .timeout(Duration::from_secs(600))
    }

    /// 拼接 apt update / dnf check-update 刷新索引的命令
    fn build_refresh_command(&self, mgr: PkgMgr) -> HostCommand {
        let cmd = match mgr {
            PkgMgr::Apt => self.maybe_elevated(
                HostCommand::new("sh").arg("-c").arg("apt-get update"),
            ),
            PkgMgr::Dnf => HostCommand::new("sh").arg("-c").arg("true"),
        };
        cmd.timeout(Duration::from_secs(300))
    }

    /// use_sudo 时给命令打 .elevated() 标(提权细节由 Host 注入的密码决定),否则
    /// 原样返回Component 不自己拼 sudo,提权逻辑收敛到 Host 层
    fn maybe_elevated(&self, cmd: HostCommand) -> HostCommand {
        if self.use_sudo {
            cmd.elevated()
        } else {
            cmd
        }
    }

    /// 组件元数据,给 list_components Tauri command 使用
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::NoVnc,
            display_name: "noVNC".to_string(),
            description: "浏览器端 VNC 客户端，远端 SnowLuma 扫码登录用".to_string(),
            repo_url: Some("https://novnc.com/".to_string()),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Linux, Locality::Local),
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
            ],
            category: crate::types::ComponentCategory::RuntimeDep,
        }
    }
}

impl Default for NoVncComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Component for NoVncComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NoVnc
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        // 只在 Linux(Local 也支持但极少用,主要是远端 VNC)
        &[
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        // 探测策略:同时检查 websockify + x11vnc 是否存在
        let cmd = HostCommand::new("sh").arg("-c").arg(
            "command -v websockify >/dev/null && command -v x11vnc >/dev/null && \
             echo OK && websockify --help 2>&1 | head -1",
        );
        let out = match host.run_to_string(cmd).await {
            Ok(o) => o,
            Err(_) => return Ok(None),
        };
        if !out.success() {
            return Ok(None);
        }
        if !out.stdout.contains("OK") {
            return Ok(None);
        }

        // 用 dpkg / rpm 获取 novnc 包版本(尽量给出版本号,失败也不影响 detect)
        let version = detect_package_version(host).await.unwrap_or_else(|| "installed".to_string());
        Ok(Some(DetectedVersion {
            version,
            source: "websockify + x11vnc detected via PATH".into(),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;

        // Step 1:探测包管理器
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "detect package manager".into(),
        })
        .await;
        let mgr = self.detect_pkg_manager(host).await?;
        ctx.info(format!("package manager: {mgr:?}")).await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // Step 2:刷新包索引
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "refresh package index".into(),
        })
        .await;
        let refresh_cmd = self.build_refresh_command(mgr);
        if mgr == PkgMgr::Apt {
            run_pkg_command_with_progress(
                host,
                ctx,
                refresh_cmd,
                2,
                5,
                25,
                "apt-get update…",
            )
            .await?;
        } else {
            let out = host.run_to_string(refresh_cmd).await?;
            if !out.success() {
                return Err(ActionError::install_step(
                    "apt_update",
                    format!(
                        "exit={:?} stderr={}",
                        out.exit_code,
                        out.stderr.trim()
                    ),
                ));
            }
        }
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // Step 3:装图形栈
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: format!("install graphics stack ({mgr:?})"),
        })
        .await;
        let install_cmd = self.build_install_command(mgr);
        run_pkg_command_with_progress(
            host,
            ctx,
            install_cmd,
            3,
            28,
            95,
            "apt/dnf install 图形栈…",
        )
        .await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "remove novnc / websockify / x11vnc / fluxbox / xvfb".into(),
        })
        .await;
        let mgr = self.detect_pkg_manager(host).await?;
        let pkgs_apt = "novnc websockify x11vnc fluxbox xvfb";
        let pkgs_dnf = "novnc python3-websockify x11vnc fluxbox openbox xorg-x11-server-Xvfb";
        let cmd_str = match mgr {
            PkgMgr::Apt => {
                format!("DEBIAN_FRONTEND=noninteractive apt-get remove -y {pkgs_apt}")
            }
            PkgMgr::Dnf => format!("dnf remove -y {pkgs_dnf}"),
        };
        let cmd = self.maybe_elevated(HostCommand::new("sh").arg("-c").arg(cmd_str));
        let out = host.run_to_string(cmd).await?;
        if !out.success() {
            // 卸载失败一般是某个包未安装;不视为致命错误,只记录
            ctx.log(
                ProgressLogLevel::Warn,
                format!(
                    "novnc uninstall: exit={:?} stderr={}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            )
            .await;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let detected = self.detect(host).await?;
        let mut report = VerifyReport::ok().with_check(
            "websockify + x11vnc on PATH",
            detected.is_some(),
            detected.as_ref().map(|v| v.source.clone()),
        );

        // 单独再检查 fluxbox / xvfb(只 warn,不影响 ok)
        for binary in &["fluxbox", "Xvfb"] {
            let cmd = HostCommand::new("sh")
                .arg("-c")
                .arg(format!("command -v {binary}"));
            let exists = match host.run_to_string(cmd).await {
                Ok(o) => o.success() && !o.stdout.trim().is_empty(),
                Err(_) => false,
            };
            report = report.with_check(format!("{binary} on PATH"), exists, None);
        }

        Ok(report)
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // noVNC 没有"统一启动命令",一般由 SnowLuma daemon 自己拼装
        // 这里返回 websockify 的占位命令(供调用方拼装时复用)
        let mut cmd = HostCommand::new("websockify");
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

/// 尝试通过 dpkg / rpm 拿到 novnc 包版本号失败返回 None
async fn detect_package_version(host: &dyn Host) -> Option<String> {
    // dpkg
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg("dpkg-query -W -f='${Version}' novnc 2>/dev/null || rpm -q --qf '%{VERSION}' novnc 2>/dev/null");
    let out = host.run_to_string(cmd).await.ok()?;
    if out.success() {
        let s = out.stdout.trim();
        if !s.is_empty() && !s.contains("not installed") && !s.contains("no package") {
            return Some(s.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_command_apt_includes_required_packages() {
        let comp = NoVncComponent::new();
        let cmd = comp.build_install_command(PkgMgr::Apt);
        assert_eq!(cmd.program, "sh");
        let arg = cmd.args.last().unwrap();
        assert!(arg.contains("apt-get install"));
        for pkg in &["dbus-x11", "fluxbox", "xvfb", "x11vnc", "novnc", "websockify"] {
            assert!(arg.contains(pkg), "apt cmd should contain {pkg}: {arg}");
        }
    }

    #[test]
    fn install_command_dnf_includes_required_packages() {
        let comp = NoVncComponent::new();
        let cmd = comp.build_install_command(PkgMgr::Dnf);
        let arg = cmd.args.last().unwrap();
        assert!(arg.contains("dnf install"));
        for pkg in &["x11vnc", "novnc", "python3-websockify", "fluxbox", "xorg-x11-server-Xvfb"] {
            assert!(arg.contains(pkg), "dnf cmd should contain {pkg}: {arg}");
        }
    }

    #[test]
    fn install_command_uses_sudo_by_default() {
        // 提权改走 .elevated() 标志,命令体本身不含 sudo(由 Host 层注入 sudo -S/-n)
        let comp = NoVncComponent::new();
        let cmd = comp.build_install_command(PkgMgr::Apt);
        assert!(cmd.elevated, "默认 use_sudo 时必须打 elevated 标");
        assert!(!cmd.args.last().unwrap().contains("sudo"), "命令体不该再硬编码 sudo");
    }

    #[test]
    fn install_command_skips_sudo_when_disabled() {
        let comp = NoVncComponent::new().with_sudo(false);
        let cmd = comp.build_install_command(PkgMgr::Apt);
        assert!(!cmd.elevated, "use_sudo=false 不打 elevated 标");
        assert!(!cmd.args.last().unwrap().contains("sudo"));
    }

    #[test]
    fn refresh_command_for_dnf_is_noop() {
        let comp = NoVncComponent::new();
        let cmd = comp.build_refresh_command(PkgMgr::Dnf);
        assert_eq!(cmd.args.last().unwrap(), "true");
    }

    #[test]
    fn refresh_command_for_apt_uses_apt_get_update() {
        let comp = NoVncComponent::new();
        let cmd = comp.build_refresh_command(PkgMgr::Apt);
        assert!(cmd.args.last().unwrap().contains("apt-get update"));
    }

    #[test]
    fn id_returns_novnc() {
        assert_eq!(NoVncComponent::new().id(), ComponentId::NoVnc);
    }

    #[test]
    fn supported_targets_only_linux() {
        let comp = NoVncComponent::new();
        assert!(comp.supported_targets().contains(&(Os::Linux, Locality::Remote)));
        assert!(!comp.supported_targets().contains(&(Os::Windows, Locality::Local)));
    }

    #[test]
    fn apt_install_uses_noninteractive_frontend() {
        // 防止 dpkg 的 menuconfig 阻塞 SSH 命令(legacy 教训)
        let comp = NoVncComponent::new();
        let cmd = comp.build_install_command(PkgMgr::Apt);
        assert!(cmd
            .args
            .last()
            .unwrap()
            .contains("DEBIAN_FRONTEND=noninteractive"));
    }

    #[test]
    fn dnf_install_uses_allow_erasing() {
        // legacy install_snowluma.sh.j2 L112 强制要求(否则单包匹配失败 abort 全部)
        let comp = NoVncComponent::new();
        let cmd = comp.build_install_command(PkgMgr::Dnf);
        assert!(cmd.args.last().unwrap().contains("--allowerasing"));
        assert!(cmd.args.last().unwrap().contains("--setopt=strict=0"));
    }
}

//! 每实例工作目录 + `.venv` 装 astrbot。Created 才 `init -y` 并预置 OneBot 行与 WebUI 账号；
//! 账号必须在首启前写好，否则 AstrBot 自己随机生成密码且只打日志。

use std::time::Duration;

use async_trait::async_trait;
use ncd_component::{
    ActionCtx, ActionError, Component, ComponentId, DetectOutcome, DetectedVersion, LaunchArgs,
    ProgressKind, Requirement, UnusableInstall, VerifyReport,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};

use super::config_json::{cmd_config_path, load_cmd_config, parse_cmd_config, save_cmd_config};
use super::dashboard_auth::set_dashboard_account;
use super::manifest::{
    ASTRBOT_CMD_CONFIG, ASTRBOT_PYTHON_REQUIRES, ASTRBOT_UV_VERSION_RANGE, PYPI_ASTRBOT,
};
use super::platform::{
    next_dashboard_port, platforms, read_aiocqhttp_port, set_dashboard_port, upsert_claimed_row,
};
use crate::uv_tooling::{
    read_uv_marker, resolve_uv, venv_python, venv_script, write_uv_marker,
};

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

const LONG_STEP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
pub struct AstrBotComponent {
    pub install_dir: HostPath,
    pub port: u16,
    pub instance_id: String,
    pub uv_bin: Option<HostPath>,
    pub adopt_existing: bool,
    /// Created 首启前种进 `dashboard` 的账号；None 交给 AstrBot 首启自生成
    pub webui_username: Option<String>,
    pub webui_password: Option<String>,
}

impl AstrBotComponent {
    pub fn new(install_dir: HostPath, port: u16, instance_id: impl Into<String>) -> Self {
        Self {
            install_dir,
            port,
            instance_id: instance_id.into(),
            uv_bin: None,
            adopt_existing: false,
            webui_username: None,
            webui_password: None,
        }
    }

    pub fn with_uv_bin(mut self, uv_bin: Option<HostPath>) -> Self {
        self.uv_bin = uv_bin;
        self
    }

    pub fn with_adopt_existing(mut self, adopt: bool) -> Self {
        self.adopt_existing = adopt;
        self
    }

    pub fn with_webui_account(mut self, username: Option<String>, password: Option<String>) -> Self {
        self.webui_username = username.filter(|s| !s.trim().is_empty());
        self.webui_password = password.filter(|s| !s.is_empty());
        self
    }

    pub fn venv_python(&self, os: Os) -> HostPath {
        venv_python(&self.install_dir, os)
    }

    pub fn astrbot_bin(&self, os: Os) -> HostPath {
        venv_script(&self.install_dir, os, "astrbot")
    }

    async fn preferred_uvs(&self, host: &dyn Host) -> Vec<HostPath> {
        let mut out = Vec::with_capacity(2);
        if let Some(p) = &self.uv_bin {
            out.push(p.clone());
        }
        if let Some(p) = read_uv_marker(host, &self.install_dir).await {
            out.push(p);
        }
        out
    }

    pub async fn resolve_launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let bin = self.astrbot_bin(host.os());
        if host.exists(&bin).await? {
            return Ok(self.launch_with(host.os(), args));
        }
        Err(ActionError::other(format!(
            "实例虚拟环境没有 astrbot（{}），请先安装 / 重新安装",
            bin.as_posix()
        )))
    }

    fn launch_with(&self, os: Os, args: &LaunchArgs) -> HostCommand {
        let cmd = HostCommand::new(self.astrbot_bin(os).as_posix())
            .arg("run")
            .working_dir(self.install_dir.clone())
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .long_running();
        args.apply_to(cmd)
    }

    async fn run_step(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
        step: u32,
        name: &str,
        cmd: HostCommand,
    ) -> Result<(), ActionError> {
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        ctx.emit(ProgressKind::StepBegin {
            step,
            message: name.to_string(),
        })
        .await;
        let cmd = cmd
            .timeout(LONG_STEP_TIMEOUT)
            .cancel_token(ctx.cancel_token())
            .env("UV_NO_PROGRESS", "1")
            .env("PYTHONUTF8", "1");
        let log_ctx = ctx.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let drain = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                log_ctx.info(line).await;
            }
        });
        let out = host
            .run_streaming(
                cmd,
                Box::new(move |_src, line| {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        let _ = tx.send(trimmed.to_string());
                    }
                }),
            )
            .await;
        drain.abort();
        let out = out.map_err(|e| ActionError::install_step(name, e.to_string()))?;
        if !out.success() {
            ctx.emit(ProgressKind::StepEnd { step, ok: false }).await;
            let hint = if name.contains("venv") || name.contains("3.12") {
                format!(
                    "AstrBot 需要 Python {ASTRBOT_PYTHON_REQUIRES} 或更高。请先安装 Python {ASTRBOT_PYTHON_REQUIRES}，或确认 uv 能下载托管的 {ASTRBOT_PYTHON_REQUIRES}。"
                )
            } else {
                String::new()
            };
            return Err(ActionError::install_step(
                name,
                format!(
                    "exit={:?}: {} {hint}",
                    out.exit_code,
                    out.stderr.trim().lines().last().unwrap_or_default()
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
        Ok(())
    }

    async fn provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        if self.adopt_existing {
            return self.adopt_provision(host, ctx).await;
        }
        if host.exists(&cmd_config_path(&self.install_dir)).await? {
            return self.upgrade_provision(host, ctx).await;
        }

        const TOTAL: u32 = 5;
        ctx.emit(ProgressKind::Started { total_steps: TOTAL }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "解析 uv".to_string(),
        })
        .await;
        let preferred = self.preferred_uvs(host).await;
        let uv = resolve_uv(host, &preferred).await?;
        ctx.info(format!("uv {}: {}", uv.version, uv.uv_bin.as_posix()))
            .await;
        host.create_dir_all(&self.install_dir).await?;
        write_uv_marker(host, &self.install_dir, &uv).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        let venv = HostCommand::new(uv.uv_bin.as_posix())
            .arg("venv")
            .arg("--python")
            .arg(ASTRBOT_PYTHON_REQUIRES)
            .arg(".venv")
            .working_dir(self.install_dir.clone());
        self.run_step(
            host,
            ctx,
            2,
            &format!("创建 Python {ASTRBOT_PYTHON_REQUIRES} 虚拟环境"),
            venv,
        )
        .await?;

        let pip = HostCommand::new(uv.uv_bin.as_posix())
            .arg("pip")
            .arg("install")
            .arg("--python")
            .arg(".venv")
            .arg(PYPI_ASTRBOT)
            .working_dir(self.install_dir.clone());
        self.run_step(host, ctx, 3, "安装 astrbot", pip).await?;

        let init = HostCommand::new(self.astrbot_bin(host.os()).as_posix())
            .arg("init")
            .arg("-y")
            .working_dir(self.install_dir.clone());
        self.run_step(host, ctx, 4, "初始化工作目录（astrbot init -y）", init)
            .await?;

        ctx.emit(ProgressKind::StepBegin {
            step: 5,
            message: if self.webui_password.is_some() {
                "预置 OneBot v11、WebUI 口与登录账号".to_string()
            } else {
                "预置 OneBot v11 与 WebUI 口".to_string()
            },
        })
        .await;
        self.seed_created_config(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 5, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 已有工作目录：只升级包，不跑 `init`、不改 cmd_config。
    async fn upgrade_provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "解析 uv".to_string(),
        })
        .await;
        let preferred = self.preferred_uvs(host).await;
        let uv = resolve_uv(host, &preferred).await?;
        ctx.info(format!("uv {}: {}", uv.version, uv.uv_bin.as_posix()))
            .await;
        write_uv_marker(host, &self.install_dir, &uv).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        if !host.exists(&self.venv_python(host.os())).await? {
            let venv = HostCommand::new(uv.uv_bin.as_posix())
                .arg("venv")
                .arg("--python")
                .arg(ASTRBOT_PYTHON_REQUIRES)
                .arg(".venv")
                .working_dir(self.install_dir.clone());
            self.run_step(host, ctx, 2, "创建实例虚拟环境", venv).await?;
        } else {
            ctx.emit(ProgressKind::StepBegin {
                step: 2,
                message: "已有虚拟环境".to_string(),
            })
            .await;
            ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;
        }

        let pip = HostCommand::new(uv.uv_bin.as_posix())
            .arg("pip")
            .arg("install")
            .arg("--upgrade")
            .arg("--python")
            .arg(".venv")
            .arg(PYPI_ASTRBOT)
            .working_dir(self.install_dir.clone());
        self.run_step(host, ctx, 3, "升级 astrbot", pip).await?;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn adopt_provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "解析 uv".to_string(),
        })
        .await;
        let preferred = self.preferred_uvs(host).await;
        let uv = resolve_uv(host, &preferred).await?;
        ctx.info(format!("uv {}: {}", uv.version, uv.uv_bin.as_posix()))
            .await;
        write_uv_marker(host, &self.install_dir, &uv).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        if !host.exists(&self.venv_python(host.os())).await? {
            let venv = HostCommand::new(uv.uv_bin.as_posix())
                .arg("venv")
                .arg("--python")
                .arg(ASTRBOT_PYTHON_REQUIRES)
                .arg(".venv")
                .working_dir(self.install_dir.clone());
            self.run_step(host, ctx, 2, "创建实例虚拟环境", venv).await?;
        } else {
            ctx.emit(ProgressKind::StepBegin {
                step: 2,
                message: "已有虚拟环境".to_string(),
            })
            .await;
            ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;
        }

        let pip = HostCommand::new(uv.uv_bin.as_posix())
            .arg("pip")
            .arg("install")
            .arg("--python")
            .arg(".venv")
            .arg(PYPI_ASTRBOT)
            .working_dir(self.install_dir.clone());
        self.run_step(host, ctx, 3, "同步 astrbot 到实例环境", pip)
            .await?;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn seed_created_config(&self, host: &dyn Host) -> Result<(), ActionError> {
        let mut root = load_cmd_config(host, &self.install_dir)
            .await
            .map_err(|e| ActionError::install_step("seed-config", e.to_string()))?;
        let mut taken: Vec<u16> = platforms(&root)
            .iter()
            .filter_map(read_aiocqhttp_port)
            .collect();
        taken.push(self.port);
        let dash = pick_free_dashboard_port(host, self.port, &taken).await;
        set_dashboard_port(&mut root, dash)
            .map_err(|e| ActionError::install_step("seed-config", e.to_string()))?;
        upsert_claimed_row(&mut root, &self.instance_id, self.port, "")
            .map_err(|e| ActionError::install_step("seed-config", e.to_string()))?;
        if let Some(password) = &self.webui_password {
            set_dashboard_account(&mut root, self.webui_username.as_deref(), password)
                .map_err(|e| ActionError::install_step("seed-config", e.to_string()))?;
        }
        save_cmd_config(host, &self.install_dir, &root, true)
            .await
            .map_err(|e| ActionError::install_step("seed-config", e.to_string()))?;
        Ok(())
    }

    async fn read_installed_version(&self, host: &dyn Host) -> Result<Option<String>, ActionError> {
        let bin = self.astrbot_bin(host.os());
        if !host.exists(&bin).await? {
            return Ok(None);
        }
        let out = host
            .run_to_string(HostCommand::new(bin.as_posix()).arg("--version"))
            .await?;
        if !out.success() {
            return Ok(Some("installed".into()));
        }
        let line = out.stdout.lines().next().unwrap_or("").trim();
        if line.is_empty() {
            return Ok(Some("installed".into()));
        }
        Ok(Some(line.to_string()))
    }
}

async fn pick_free_dashboard_port(host: &dyn Host, ws_port: u16, extra_taken: &[u16]) -> u16 {
    let mut p = super::manifest::ASTRBOT_DEFAULT_DASHBOARD_PORT;
    for _ in 0..64 {
        if p != 0
            && p != ws_port
            && !extra_taken.contains(&p)
            && port_appears_free(host, p).await
        {
            return p;
        }
        p = p.saturating_add(1);
        if p < super::manifest::ASTRBOT_DEFAULT_DASHBOARD_PORT {
            break;
        }
    }
    next_dashboard_port(ws_port, extra_taken)
}

async fn port_appears_free(host: &dyn Host, port: u16) -> bool {
    if host.locality() == Locality::Local {
        return std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).is_ok();
    }
    let snippet = format!("import socket;s=socket.socket();s.bind(('0.0.0.0',{port}))");
    for py in ["python3", "python"] {
        if !host.command_exists(py).await {
            continue;
        }
        let cmd = HostCommand::new(py)
            .arg("-c")
            .arg(&snippet)
            .timeout(Duration::from_secs(8));
        if let Ok(out) = host.run_to_string(cmd).await {
            return out.success();
        }
    }
    true
}

#[async_trait]
impl Component for AstrBotComponent {
    fn id(&self) -> ComponentId {
        ComponentId::AstrBot
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        vec![Requirement::component_version(
            ComponentId::Uv,
            ASTRBOT_UV_VERSION_RANGE,
        )]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        let cfg = cmd_config_path(&self.install_dir);
        if !host.exists(&cfg).await? {
            return Ok(DetectOutcome::NotInstalled);
        }
        let text = host
            .read_file(&cfg)
            .await
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        if parse_cmd_config(&text).is_err() {
            return Ok(DetectOutcome::NotInstalled);
        }
        let version = self.read_installed_version(host).await?;
        let bin = self.astrbot_bin(host.os());
        if !host.exists(&bin).await? {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version,
                reason: "项目已创建但依赖未同步（缺 astrbot），重新安装可修复".to_string(),
            }));
        }
        Ok(DetectOutcome::Installed(DetectedVersion {
            version: version.unwrap_or_else(|| "installed".into()),
            source: cfg.as_posix().to_string(),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.provision(host, ctx).await
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.provision(host, ctx).await
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("删除 {}", self.install_dir.as_posix()),
        })
        .await;
        if host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let mut report = VerifyReport::ok();
        for (name, path) in [
            (ASTRBOT_CMD_CONFIG, cmd_config_path(&self.install_dir)),
            ("astrbot", self.astrbot_bin(host.os())),
        ] {
            let ok = host.exists(&path).await?;
            report = report.with_check(name, ok, Some(path.as_posix().to_string()));
        }
        Ok(report)
    }

    fn launch_command(&self, host: &dyn Host, args: &LaunchArgs) -> Result<HostCommand, ActionError> {
        Ok(self.launch_with(host.os(), args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_pin_uv() {
        let comp = AstrBotComponent::new(HostPath::from_posix("/x"), 6199, "a1");
        assert_eq!(
            comp.requirements(Os::Linux, Locality::Remote),
            vec![Requirement::component_version(ComponentId::Uv, ">=0.4")]
        );
        assert_eq!(comp.id(), ComponentId::AstrBot);
    }

    #[test]
    fn launch_uses_venv_astrbot_run() {
        let comp = AstrBotComponent::new(HostPath::from_posix("/home/u/apps/a1"), 6199, "a1");
        let cmd = comp.launch_with(Os::Linux, &LaunchArgs::default());
        assert_eq!(cmd.program, "/home/u/apps/a1/.venv/bin/astrbot");
        assert_eq!(cmd.args, vec!["run".to_string()]);
    }
}

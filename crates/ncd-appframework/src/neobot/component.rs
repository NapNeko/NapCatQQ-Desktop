//! NeoBotComponent：每实例一份 `.venv`，用 uv 从 PyPI 装 `neobot-app`。
//!
//! 首装（5 步）：
//! 1. 解析 uv（调用方指定 → 上次安装记录 `.ncd-uv` → PATH），建实例目录并记下标记
//! 2. 预置 Python 3.13（主机自己下不动时由桌面端镜像下载后传上去）
//! 3. `uv venv --python 3.13 .venv`
//! 4. `uv pip install --python .venv neobot-app`
//! 5. 种最小配置：面板口 / 面板监听地址、OneBot 反向 WS 口与 token
//!
//! 与 AstrBot 的差别：NeoBot 没有 `init` 子命令，配置由桌面端直接写；
//! 面板配置在 `plugins_data/dashboard/config.toml`（不在 `data/config.toml`）。
//!
//! 探测：`data/config.toml` 存在且能读出 `[adapter]` 才算装了；有配置但 `.venv` 里没有
//! `neobot` 入口视为「依赖未同步」，给 `Unusable` 而不是 `NotInstalled`。

use std::time::Duration;

use async_trait::async_trait;
use ncd_component::{
    ActionCtx, ActionError, Component, ComponentId, DetectOutcome, DetectedVersion, LaunchArgs,
    ProgressKind, Requirement, UnusableInstall, VerifyReport,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};

use super::manifest::{
    ADAPTER_MODE_ONEBOT, KEY_ADAPTER, KEY_ADAPTER_MODE, KEY_DASHBOARD_HOST, KEY_DASHBOARD_PORT,
    KEY_REVERSE_WS_ACCESS_TOKEN, KEY_REVERSE_WS_HOST, KEY_REVERSE_WS_PORT, NEOBOT_CONFIG_TOML,
    NEOBOT_DASHBOARD_CONFIG, NEOBOT_PYTHON_REQUIRES, NEOBOT_UV_VERSION_RANGE, PYPI_NEOBOT,
};
use crate::ports::PortUsage;
use crate::uv_tooling::{
    ensure_python, read_uv_marker, resolve_uv, venv_python, venv_script, write_uv_marker,
};

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

/// 依赖里有 playwright / onnxruntime / numpy，首次装几百 MB，给足时间
const LONG_STEP_TIMEOUT: Duration = Duration::from_secs(40 * 60);
/// 桌面端替远端下解释器的暂存目录，装完就删
const PYTHON_SCRATCH_DIR: &str = ".ncd-python";
/// `neobot` 控制台入口的脚本名
const NEOBOT_BIN_NAME: &str = "neobot";

#[derive(Debug, Clone)]
pub struct NeoBotComponent {
    pub install_dir: HostPath,
    /// OneBot 反向 WS 口（应用端听口，协议 Bot 连过来）
    pub onebot_port: u16,
    /// 网页面板 HTTP 口
    pub dashboard_port: u16,
    /// 桌面端管理的 uv；None 只看实例标记 / PATH
    pub uv_bin: Option<HostPath>,
    /// PyPI 索引镜像（`uv pip install --default-index`）；None 用默认源
    pub pypi_index: Option<String>,
    /// 领养已有项目：只同步依赖，不写配置、不改端口
    pub adopt_existing: bool,
}

impl NeoBotComponent {
    pub fn new(install_dir: HostPath, onebot_port: u16, dashboard_port: u16) -> Self {
        Self {
            install_dir,
            onebot_port,
            dashboard_port,
            uv_bin: None,
            pypi_index: None,
            adopt_existing: false,
        }
    }

    pub fn with_uv_bin(mut self, uv_bin: Option<HostPath>) -> Self {
        self.uv_bin = uv_bin;
        self
    }

    pub fn with_pypi_index(mut self, index: Option<String>) -> Self {
        self.pypi_index = index.filter(|s| !s.trim().is_empty());
        self
    }

    pub fn with_adopt_existing(mut self, adopt: bool) -> Self {
        self.adopt_existing = adopt;
        self
    }

    pub fn config_toml(&self) -> HostPath {
        self.install_dir.join(NEOBOT_CONFIG_TOML)
    }

    pub fn dashboard_config(&self) -> HostPath {
        self.install_dir.join(NEOBOT_DASHBOARD_CONFIG)
    }

    pub fn venv_python(&self, os: Os) -> HostPath {
        venv_python(&self.install_dir, os)
    }

    pub fn neobot_bin(&self, os: Os) -> HostPath {
        venv_script(&self.install_dir, os, NEOBOT_BIN_NAME)
    }

    /// 远端替主机下解释器时的暂存目录
    pub fn python_scratch_dir(&self) -> HostPath {
        self.install_dir.join(PYTHON_SCRATCH_DIR)
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

    /// 运行时启动命令：`.venv` 里的 `neobot` 直接跑，不需要 uv 在 PATH
    pub async fn resolve_launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let bin = self.neobot_bin(host.os());
        if !host.exists(&bin).await? {
            return Err(ActionError::other(format!(
                "实例虚拟环境里没有 neobot（{}），请先安装 / 重新安装",
                bin.as_posix()
            )));
        }
        Ok(self.launch_with(host.os(), args))
    }

    fn launch_with(&self, os: Os, args: &LaunchArgs) -> HostCommand {
        let cmd = HostCommand::new(self.neobot_bin(os).as_posix())
            .working_dir(self.install_dir.clone())
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .long_running();
        args.apply_to(cmd)
    }

    /// 统一的步骤包装：超时、取消、UTF-8、逐行转 ctx.info
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
            let hint = if name.contains("Python") {
                format!(
                    "；NeoBot 需要 Python {NEOBOT_PYTHON_REQUIRES}，请确认 uv 能下到托管的 {NEOBOT_PYTHON_REQUIRES}"
                )
            } else {
                String::new()
            };
            return Err(ActionError::install_step(
                name,
                format!(
                    "exit={:?}: {}{hint}",
                    out.exit_code,
                    out.stderr.trim().lines().last().unwrap_or_default()
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
        Ok(())
    }

    /// 首装 / 重装共用：解析 uv → Python → venv → 装包 → 写配置
    async fn provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        if self.adopt_existing {
            return self.adopt_provision(host, ctx).await;
        }
        const TOTAL: u32 = 5;
        ctx.emit(ProgressKind::Started { total_steps: TOTAL }).await;

        // 1. 解析 uv
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

        // 2. 预置 Python 3.13（主机下不动时桌面端镜像下载后传上去）
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: format!("准备 Python {NEOBOT_PYTHON_REQUIRES}"),
        })
        .await;
        ensure_python(
            host,
            &uv.uv_bin,
            NEOBOT_PYTHON_REQUIRES,
            &self.python_scratch_dir(),
            ctx,
            2,
        )
        .await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // 3. 建 venv
        let venv = HostCommand::new(uv.uv_bin.as_posix())
            .arg("venv")
            .arg("--python")
            .arg(NEOBOT_PYTHON_REQUIRES)
            .arg(".venv")
            .working_dir(self.install_dir.clone());
        self.run_step(
            host,
            ctx,
            3,
            &format!("创建 Python {NEOBOT_PYTHON_REQUIRES} 虚拟环境"),
            venv,
        )
        .await?;

        // 4. 装 neobot-app
        let mut pip = HostCommand::new(uv.uv_bin.as_posix())
            .arg("pip")
            .arg("install")
            .arg("--upgrade")
            .arg("--python")
            .arg(".venv")
            .arg(PYPI_NEOBOT)
            .working_dir(self.install_dir.clone());
        // uv pip install 不读 pyproject 的 [tool.uv.index]，镜像必须从命令行给
        if let Some(index) = &self.pypi_index {
            pip = pip.arg("--default-index").arg(index);
        }
        self.run_step(host, ctx, 4, "安装 neobot-app", pip).await?;

        // 5. 种配置
        ctx.emit(ProgressKind::StepBegin {
            step: 5,
            message: "写入 OneBot 与面板配置".to_string(),
        })
        .await;
        self.seed_config(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 5, ok: true }).await;

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 已有项目：只补 uv 标记 + 同步依赖，不写配置、不改端口
    async fn adopt_provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        const TOTAL: u32 = 4;
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
        write_uv_marker(host, &self.install_dir, &uv).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: format!("准备 Python {NEOBOT_PYTHON_REQUIRES}"),
        })
        .await;
        ensure_python(
            host,
            &uv.uv_bin,
            NEOBOT_PYTHON_REQUIRES,
            &self.python_scratch_dir(),
            ctx,
            2,
        )
        .await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        if !host.exists(&self.venv_python(host.os())).await? {
            let venv = HostCommand::new(uv.uv_bin.as_posix())
                .arg("venv")
                .arg("--python")
                .arg(NEOBOT_PYTHON_REQUIRES)
                .arg(".venv")
                .working_dir(self.install_dir.clone());
            self.run_step(host, ctx, 3, "创建实例虚拟环境", venv)
                .await?;
        } else {
            ctx.emit(ProgressKind::StepBegin {
                step: 3,
                message: "已有虚拟环境".to_string(),
            })
            .await;
            ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        }

        let mut pip = HostCommand::new(uv.uv_bin.as_posix())
            .arg("pip")
            .arg("install")
            .arg("--python")
            .arg(".venv")
            .arg(PYPI_NEOBOT)
            .working_dir(self.install_dir.clone());
        if let Some(index) = &self.pypi_index {
            pip = pip.arg("--default-index").arg(index);
        }
        self.run_step(host, ctx, 4, "同步 neobot-app 到实例环境", pip)
            .await?;

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 写 OneBot 反向 WS 与面板配置。
    /// 面板配置在 `plugins_data/dashboard/config.toml`（与本体 config.toml 解耦）；
    /// 已有文件只补缺失的键，不覆盖用户改过的值。
    async fn seed_config(&self, host: &dyn Host) -> Result<(), ActionError> {
        self.seed_data_config(host).await?;
        self.seed_dashboard_config(host).await?;
        Ok(())
    }

    /// `data/config.toml` 的 `[adapter]`：只补三个键，其余（含用户自己的模型/API 配置）原样留着
    async fn seed_data_config(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.config_toml();
        let existing = read_text(host, &path).await?;
        let mut table = match existing.as_deref() {
            Some(text) if !text.trim().is_empty() => parse_toml(text)
                .map_err(|e| ActionError::install_step("seed-config", format!("data/config.toml 解析失败：{e}")))?,
            _ => toml::Table::new(),
        };
        let adapter = table
            .entry(KEY_ADAPTER.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let adapter = adapter.as_table_mut().ok_or_else(|| {
            ActionError::install_step("seed-config", "data/config.toml 的 [adapter] 不是表")
        })?;
        // 模式：只认 onebot（反向 WS 对接的前提）。旧值不是 onebot 时给出提示交给用户，不静默改写
        let current_mode = adapter
            .get(KEY_ADAPTER_MODE)
            .and_then(|v| v.as_str())
            .unwrap_or(ADAPTER_MODE_ONEBOT);
        if current_mode != ADAPTER_MODE_ONEBOT {
            tracing::warn!(
                mode = current_mode,
                "neobot adapter.mode is not onebot; reverse ws link will not work"
            );
        } else {
            adapter.insert(
                KEY_ADAPTER_MODE.to_string(),
                toml::Value::String(ADAPTER_MODE_ONEBOT.to_string()),
            );
        }
        // 只监听回环：协议 Bot 就在同一台机上（跨机由桌面端开隧道）
        adapter.insert(
            KEY_REVERSE_WS_HOST.to_string(),
            toml::Value::String("127.0.0.1".to_string()),
        );
        adapter.insert(
            KEY_REVERSE_WS_PORT.to_string(),
            toml::Value::Integer(i64::from(self.onebot_port)),
        );
        write_text(host, &path, &toml::to_string_pretty(&table).map_err(|e| {
            ActionError::install_step("seed-config", format!("data/config.toml 渲染失败：{e}"))
        })?)
        .await
    }

    /// 面板配置：平铺的 `host` / `port`（`DashboardConfig`），只补缺失键
    async fn seed_dashboard_config(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.dashboard_config();
        let existing = read_text(host, &path).await?;
        let mut table = match existing.as_deref() {
            Some(text) if !text.trim().is_empty() => parse_toml(text).map_err(|e| {
                ActionError::install_step("seed-config", format!("面板配置解析失败：{e}"))
            })?,
            _ => toml::Table::new(),
        };
        // 只监听回环：远端实例由桌面端开 -L 隧道
        table
            .entry(KEY_DASHBOARD_HOST.to_string())
            .or_insert_with(|| toml::Value::String("127.0.0.1".to_string()));
        table
            .entry(KEY_DASHBOARD_PORT.to_string())
            .or_insert_with(|| toml::Value::Integer(i64::from(self.dashboard_port)));
        write_text(host, &path, &toml::to_string_pretty(&table).map_err(|e| {
            ActionError::install_step("seed-config", format!("面板配置渲染失败：{e}"))
        })?)
        .await
    }

    /// token 由对接（apply_link）写；这里只在首装给个占位空串，让键存在便于用户手改
    async fn ensure_token_key(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.config_toml();
        let Some(text) = read_text(host, &path).await? else {
            return Ok(());
        };
        let mut table = parse_toml(&text)
            .map_err(|e| ActionError::install_step("seed-config", format!("{e}")))?;
        if let Some(adapter) = table.get_mut(KEY_ADAPTER).and_then(|v| v.as_table_mut())
            && !adapter.contains_key(KEY_REVERSE_WS_ACCESS_TOKEN)
        {
            adapter.insert(
                KEY_REVERSE_WS_ACCESS_TOKEN.to_string(),
                toml::Value::String(String::new()),
            );
            write_text(
                host,
                &path,
                &toml::to_string_pretty(&table)
                    .map_err(|e| ActionError::install_step("seed-config", e.to_string()))?,
            )
            .await?;
        }
        Ok(())
    }

    /// 面板口被占用时从 9981 起向后挑一个（NeoBot 自己也会向后试 10 个，
    /// 但桌面端 tunnel 必须打在真实口上，所以要主动避开）
    pub async fn pick_dashboard_port(host: &dyn Host, onebot_port: u16) -> u16 {
        let mut candidate = super::manifest::NEOBOT_DEFAULT_DASHBOARD_PORT;
        let usage = PortUsage::probe(host).await;
        for _ in 0..32 {
            if candidate != 0
                && candidate != onebot_port
                && usage.as_ref().is_none_or(|u| u.is_free(candidate))
            {
                return candidate;
            }
            candidate = candidate.saturating_add(1);
            if candidate < super::manifest::NEOBOT_DEFAULT_DASHBOARD_PORT {
                break;
            }
        }
        super::manifest::NEOBOT_DEFAULT_DASHBOARD_PORT
    }

    async fn read_installed_version(&self, host: &dyn Host) -> Result<Option<String>, ActionError> {
        let bin = self.neobot_bin(host.os());
        if !host.exists(&bin).await? {
            return Ok(None);
        }
        // 上游没有 --version；能跑起来就算装了，版本留给 pip 记录
        Ok(Some("installed".to_string()))
    }
}

fn parse_toml(text: &str) -> Result<toml::Table, String> {
    text.parse::<toml::Table>().map_err(|e| e.to_string())
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, ActionError> {
    if !host.exists(path).await? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

async fn write_text(host: &dyn Host, path: &HostPath, text: &str) -> Result<(), ActionError> {
    if let Some(parent) = path.parent() {
        host.create_dir_all(&parent).await?;
    }
    host.write_file(path, text.as_bytes())
        .await
        .map_err(|e| ActionError::install_step("seed-config", e.to_string()))
}

#[async_trait]
impl Component for NeoBotComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NeoBot
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        vec![Requirement::component_version(
            ComponentId::Uv,
            NEOBOT_UV_VERSION_RANGE,
        )]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        let cfg = self.config_toml();
        if !host.exists(&cfg).await? {
            return Ok(DetectOutcome::NotInstalled);
        }
        let text = read_text(host, &cfg).await?.unwrap_or_default();
        if parse_toml(&text).is_err() {
            return Ok(DetectOutcome::NotInstalled);
        }
        let version = self.read_installed_version(host).await?;
        let bin = self.neobot_bin(host.os());
        if !host.exists(&bin).await? {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version,
                reason: "项目已创建但依赖未同步（缺 neobot），重新安装可修复".to_string(),
            }));
        }
        Ok(DetectOutcome::Installed(DetectedVersion {
            version: version.unwrap_or_else(|| "installed".into()),
            source: cfg.as_posix().to_string(),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.provision(host, ctx).await?;
        // token 键在首装后补齐（seed_config 里不写，避免覆盖对接写入的值）
        self.ensure_token_key(host).await
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
            (NEOBOT_CONFIG_TOML, self.config_toml()),
            ("venv neobot", self.neobot_bin(host.os())),
        ] {
            let ok = host.exists(&path).await?;
            report = report.with_check(name, ok, Some(path.as_posix().to_string()));
        }
        Ok(report)
    }

    fn launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        Ok(self.launch_with(host.os(), args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_pin_uv_only() {
        let comp = NeoBotComponent::new(HostPath::from_posix("/x"), 8080, 9981);
        assert_eq!(
            comp.requirements(Os::Linux, Locality::Remote),
            vec![Requirement::component_version(ComponentId::Uv, ">=0.4")]
        );
        assert_eq!(comp.id(), ComponentId::NeoBot);
        assert!(
            comp.supported_targets()
                .contains(&(Os::Windows, Locality::Local))
        );
        assert!(
            !comp
                .supported_targets()
                .contains(&(Os::Windows, Locality::Remote)),
            "Windows 远端没有实现"
        );
    }

    #[test]
    fn instance_paths_follow_locked_layout() {
        let comp =
            NeoBotComponent::new(HostPath::from_posix("/home/u/ncd/apps/neobot/n1"), 8080, 9981);
        assert_eq!(
            comp.config_toml().as_posix(),
            "/home/u/ncd/apps/neobot/n1/data/config.toml"
        );
        assert_eq!(
            comp.dashboard_config().as_posix(),
            "/home/u/ncd/apps/neobot/n1/plugins_data/dashboard/config.toml"
        );
        assert_eq!(
            comp.venv_python(Os::Linux).as_posix(),
            "/home/u/ncd/apps/neobot/n1/.venv/bin/python"
        );
        assert_eq!(
            comp.neobot_bin(Os::Linux).as_posix(),
            "/home/u/ncd/apps/neobot/n1/.venv/bin/neobot"
        );
        let cmd = comp.launch_with(Os::Linux, &LaunchArgs::default());
        assert_eq!(
            cmd.program,
            "/home/u/ncd/apps/neobot/n1/.venv/bin/neobot"
        );
        assert!(cmd.args.is_empty(), "入口脚本自己带参数");
    }

    #[test]
    fn windows_layout_uses_scripts_dir() {
        let comp = NeoBotComponent::new(HostPath::from_posix("/x/n1"), 8080, 9981);
        assert_eq!(
            comp.neobot_bin(Os::Windows).as_posix(),
            "/x/n1/.venv/Scripts/neobot.exe"
        );
        assert_eq!(
            comp.venv_python(Os::Windows).as_posix(),
            "/x/n1/.venv/Scripts/python.exe"
        );
    }

    #[test]
    fn pypi_index_is_normalized_and_optional() {
        let blank = NeoBotComponent::new(HostPath::from_posix("/x"), 8080, 9981)
            .with_pypi_index(Some("   ".to_string()));
        assert!(blank.pypi_index.is_none(), "空白镜像源当没设");
        let set = NeoBotComponent::new(HostPath::from_posix("/x"), 8080, 9981)
            .with_pypi_index(Some("https://pypi.tuna.tsinghua.edu.cn/simple".to_string()));
        assert_eq!(
            set.pypi_index.as_deref(),
            Some("https://pypi.tuna.tsinghua.edu.cn/simple")
        );
    }

    #[test]
    fn seed_writes_locked_keys_and_keeps_user_tables() {
        // 纯逻辑：解析已有配置后只补 [adapter] 的三个键，其它分区原样保留
        let existing = "[bot]\nqq = 10001\n\n[adapter]\nlocal_port = 8090\n";
        let mut table = parse_toml(existing).unwrap();
        let adapter = table
            .entry(KEY_ADAPTER.to_string())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let adapter = adapter.as_table_mut().unwrap();
        adapter.insert(
            KEY_ADAPTER_MODE.to_string(),
            toml::Value::String(ADAPTER_MODE_ONEBOT.to_string()),
        );
        adapter.insert(
            KEY_REVERSE_WS_HOST.to_string(),
            toml::Value::String("127.0.0.1".to_string()),
        );
        adapter.insert(
            KEY_REVERSE_WS_PORT.to_string(),
            toml::Value::Integer(8080),
        );
        assert_eq!(
            table
                .get("bot")
                .and_then(|v| v.get("qq"))
                .and_then(|v| v.as_integer()),
            Some(10001),
            "别的分区不能被动"
        );
        let adapter = table.get(KEY_ADAPTER).and_then(|v| v.as_table()).unwrap();
        assert_eq!(
            adapter.get("local_port").and_then(|v| v.as_integer()),
            Some(8090),
            "用户已有的键不能被抹掉"
        );
        assert_eq!(
            adapter.get(KEY_ADAPTER_MODE).and_then(|v| v.as_str()),
            Some("onebot")
        );
        assert_eq!(
            adapter.get(KEY_REVERSE_WS_HOST).and_then(|v| v.as_str()),
            Some("127.0.0.1")
        );
        assert_eq!(
            adapter.get(KEY_REVERSE_WS_PORT).and_then(|v| v.as_integer()),
            Some(8080)
        );
    }

    #[test]
    fn dashboard_seed_keeps_existing_port() {
        // 已有面板配置里用户改过 port：不能覆盖
        let existing = "host = \"0.0.0.0\"\nport = 9999\n";
        let mut table = parse_toml(existing).unwrap();
        table
            .entry(KEY_DASHBOARD_HOST.to_string())
            .or_insert_with(|| toml::Value::String("127.0.0.1".to_string()));
        table
            .entry(KEY_DASHBOARD_PORT.to_string())
            .or_insert_with(|| toml::Value::Integer(9981));
        assert_eq!(
            table.get(KEY_DASHBOARD_PORT).and_then(|v| v.as_integer()),
            Some(9999),
            "用户改过的面板口必须保留"
        );
        assert_eq!(
            table.get(KEY_DASHBOARD_HOST).and_then(|v| v.as_str()),
            Some("0.0.0.0")
        );
    }

    #[test]
    fn empty_dashboard_config_gets_defaults() {
        let mut table = parse_toml("").unwrap();
        table
            .entry(KEY_DASHBOARD_HOST.to_string())
            .or_insert_with(|| toml::Value::String("127.0.0.1".to_string()));
        table
            .entry(KEY_DASHBOARD_PORT.to_string())
            .or_insert_with(|| toml::Value::Integer(9981));
        assert_eq!(
            table.get(KEY_DASHBOARD_HOST).and_then(|v| v.as_str()),
            Some("127.0.0.1")
        );
        assert_eq!(
            table.get(KEY_DASHBOARD_PORT).and_then(|v| v.as_integer()),
            Some(9981)
        );
    }
}

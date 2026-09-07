//! NoneBot2Component：一个应用实例目录 = 一个 uv 项目（`.venv` 在项目内）。
//!
//! 安装流程（不引入 nb-cli，脚手架文件由桌面端直接写）：
//! 1. 解析 uv（调用方指定 → 上次安装记录 `.ncd-uv` → PATH），记下所用 uv
//! 2. 写脚手架：`pyproject.toml`（依赖 + `[tool.nonebot]`）、`bot.py`、`.env`、`.env.prod`；已有文件不覆盖
//! 3. `uv sync`：解析 / 下载 Python（uv 托管）、建 `.venv`、装 nonebot2 + onebot 适配器
//! 4. 写 `.env.prod` 的 PORT / HOST
//!
//! 探测：`pyproject.toml` 存在 + `uv.lock` 里 nonebot2 的版本；有 pyproject 无 `.venv` 视为「依赖未同步」（Unusable）。

use async_trait::async_trait;
use ncd_component::{
    ActionCtx, ActionError, Component, ComponentId, DetectOutcome, DetectedVersion, LaunchArgs,
    ProgressKind, Requirement, UnusableInstall, VerifyReport,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use std::time::Duration;

use super::manifest::{
    DRIVER_FASTAPI, ENV_DRIVER, ENV_HOST, ENV_PORT, LOCK_PACKAGE_NONEBOT2, NONEBOT2_BOT_PY,
    NONEBOT2_ENV_FILE, NONEBOT2_ENV_PROD_FILE, NONEBOT2_PYPROJECT, NONEBOT2_PYTHON_REQUIRES,
    NONEBOT2_UV_LOCK, NONEBOT2_UV_VERSION_RANGE, PYPI_ADAPTER_ONEBOT, PYPI_NONEBOT2,
};
use crate::env_file::EnvFile;
use crate::uv_tooling::{read_uv_marker, resolve_uv, venv_python, write_uv_marker};

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

const LONG_STEP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
pub struct NoneBot2Component {
    pub install_dir: HostPath,
    pub port: u16,
    /// 桌面端管理的 uv；None 只看实例标记 / PATH
    pub uv_bin: Option<HostPath>,
    /// PyPI 索引镜像（`uv sync --default-index`）；None 用默认源
    pub pypi_index: Option<String>,
}

impl NoneBot2Component {
    pub fn new(install_dir: HostPath, port: u16) -> Self {
        Self {
            install_dir,
            port,
            uv_bin: None,
            pypi_index: None,
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

    pub fn pyproject(&self) -> HostPath {
        self.install_dir.join(NONEBOT2_PYPROJECT)
    }

    pub fn uv_lock(&self) -> HostPath {
        self.install_dir.join(NONEBOT2_UV_LOCK)
    }

    pub fn bot_py(&self) -> HostPath {
        self.install_dir.join(NONEBOT2_BOT_PY)
    }

    pub fn env_file(&self) -> HostPath {
        self.install_dir.join(NONEBOT2_ENV_FILE)
    }

    pub fn env_prod_file(&self) -> HostPath {
        self.install_dir.join(NONEBOT2_ENV_PROD_FILE)
    }

    pub fn venv_python(&self, os: Os) -> HostPath {
        venv_python(&self.install_dir, os)
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

    /// 运行时启动命令：`.venv` 里的解释器直接跑 `bot.py`，不需要 uv 在 PATH
    pub async fn resolve_launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let python = self.venv_python(host.os());
        if !host.exists(&python).await? {
            return Err(ActionError::other(format!(
                "实例虚拟环境不存在（{}），请先安装 / 重新安装",
                python.as_posix()
            )));
        }
        Ok(self.launch_with(host.os(), args))
    }

    fn launch_with(&self, os: Os, args: &LaunchArgs) -> HostCommand {
        let cmd = HostCommand::new(self.venv_python(os).as_posix())
            .arg("-u")
            .arg(NONEBOT2_BOT_PY)
            .working_dir(self.install_dir.clone())
            .env("ENVIRONMENT", "prod")
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
            return Err(ActionError::install_step(
                name,
                format!(
                    "exit={:?}: {}",
                    out.exit_code,
                    out.stderr.trim().lines().last().unwrap_or_default()
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
        Ok(())
    }

    /// 首装 / 更新共用：解析 uv → 脚手架 → uv sync → 写 .env.prod
    async fn provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
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
        host.create_dir_all(&self.install_dir).await?;
        write_uv_marker(host, &self.install_dir, &uv).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "写入项目脚手架".to_string(),
        })
        .await;
        self.ensure_project_scaffold(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // 3. uv sync：托管 Python + .venv + 依赖，一步到位
        let mut sync = HostCommand::new(uv.uv_bin.as_posix())
            .arg("sync")
            .working_dir(self.install_dir.clone())
            .env("UV_PROJECT_ENVIRONMENT", ".venv");
        if let Some(index) = &self.pypi_index {
            sync = sync.arg("--default-index").arg(index);
        }
        self.run_step(host, ctx, 3, "同步 Python 依赖（uv sync）", sync)
            .await?;

        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "写入实例端口".to_string(),
        })
        .await;
        self.write_env_port(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    fn project_name(&self) -> String {
        self.install_dir
            .file_name()
            .map(|n| format!("nonebot2-{n}"))
            .unwrap_or_else(|| "nonebot2-project".to_string())
    }

    /// 脚手架内容（纯函数，便于测试）
    pub fn render_pyproject(project_name: &str) -> String {
        format!(
            r#"[project]
name = "{project_name}"
version = "0.1.0"
description = "NoneBot2 instance managed by NapCatQQ Desktop"
requires-python = "{NONEBOT2_PYTHON_REQUIRES}"
dependencies = [
    "{PYPI_NONEBOT2}",
    "{PYPI_ADAPTER_ONEBOT}",
]

[tool.nonebot]
adapters = [
    {{ name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" }},
]
plugins = []
plugin_dirs = ["plugins"]
builtin_plugins = ["echo"]
"#
        )
    }

    pub fn render_bot_py() -> &'static str {
        r#"import nonebot
from nonebot.adapters.onebot.v11 import Adapter as OneBotV11Adapter

nonebot.init()

driver = nonebot.get_driver()
driver.register_adapter(OneBotV11Adapter)

nonebot.load_from_toml("pyproject.toml")

if __name__ == "__main__":
    nonebot.run()
"#
    }

    /// 已有文件不覆盖：用户自己加的插件 / 配置不被重装抹掉
    async fn ensure_project_scaffold(&self, host: &dyn Host) -> Result<(), ActionError> {
        let files: [(HostPath, String); 4] = [
            (self.pyproject(), Self::render_pyproject(&self.project_name())),
            (self.bot_py(), Self::render_bot_py().to_string()),
            (self.env_file(), "ENVIRONMENT=prod\n".to_string()),
            (
                self.env_prod_file(),
                format!("{ENV_DRIVER}={DRIVER_FASTAPI}\n{ENV_PORT}={}\n", self.port),
            ),
        ];
        for (path, body) in files {
            if !host.exists(&path).await? {
                host.write_file(&path, body.as_bytes()).await?;
            }
        }
        let plugins = self.install_dir.join("plugins");
        if !host.exists(&plugins).await? {
            host.create_dir_all(&plugins).await?;
        }
        Ok(())
    }

    async fn write_env_port(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.env_prod_file();
        let text = host
            .read_file(&path)
            .await
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| ActionError::install_step("write-env", e.to_string()))?;
        let mut env = EnvFile::parse(&text);
        env.set(ENV_DRIVER, DRIVER_FASTAPI);
        env.set(ENV_PORT, &self.port.to_string());
        if host.locality() == Locality::Local {
            // 本机只需同机对接，不暴露到局域网（上游默认也是 127.0.0.1，这里显式写死）
            env.set(ENV_HOST, "127.0.0.1");
        }
        host.write_file(&path, env.render().as_bytes()).await?;
        Ok(())
    }

    /// 从 `uv.lock` 取某包版本：`[[package]]` 块里 `name = "<pkg>"` 之后的 `version = "…"`
    pub fn lock_package_version(lock_text: &str, package: &str) -> Option<String> {
        let mut in_target = false;
        for raw in lock_text.lines() {
            let line = raw.trim();
            if line == "[[package]]" {
                in_target = false;
                continue;
            }
            if let Some(rest) = line.strip_prefix("name") {
                let value = toml_string_value(rest);
                in_target = value.as_deref() == Some(package);
                continue;
            }
            if in_target {
                if let Some(rest) = line.strip_prefix("version") {
                    if let Some(v) = toml_string_value(rest) {
                        return Some(v);
                    }
                }
            }
        }
        None
    }

    async fn read_installed_version(&self, host: &dyn Host) -> Result<Option<String>, ActionError> {
        let lock = self.uv_lock();
        if !host.exists(&lock).await? {
            return Ok(None);
        }
        let bytes = host.read_file(&lock).await?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(Self::lock_package_version(&text, LOCK_PACKAGE_NONEBOT2))
    }
}

/// `= "value"` → `value`（只处理 uv.lock 用到的基本字符串）
fn toml_string_value(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[async_trait]
impl Component for NoneBot2Component {
    fn id(&self) -> ComponentId {
        ComponentId::NoneBot2
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        vec![Requirement::component_version(
            ComponentId::Uv,
            NONEBOT2_UV_VERSION_RANGE,
        )]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        if !host.exists(&self.pyproject()).await? {
            return Ok(DetectOutcome::NotInstalled);
        }
        let version = self.read_installed_version(host).await?;
        let python = self.venv_python(host.os());
        if !host.exists(&python).await? {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version,
                reason: "项目已创建但依赖未同步（缺 .venv），重新安装可修复".to_string(),
            }));
        }
        let Some(version) = version else {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version: None,
                reason: "uv.lock 里没有 nonebot2，重新安装可修复".to_string(),
            }));
        };
        Ok(DetectOutcome::Installed(DetectedVersion {
            version,
            source: self.uv_lock().as_posix().to_string(),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.provision(host, ctx).await
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        // 同一套流程：pyproject 已存在则不动，uv sync 按 lock 复现；要升级请删 uv.lock 后重装
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
            (NONEBOT2_PYPROJECT, self.pyproject()),
            (NONEBOT2_BOT_PY, self.bot_py()),
            (NONEBOT2_ENV_PROD_FILE, self.env_prod_file()),
            ("venv python", self.venv_python(host.os())),
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
    fn requirements_pin_uv_not_node() {
        let comp = NoneBot2Component::new(HostPath::from_posix("/x"), 8080);
        assert_eq!(
            comp.requirements(Os::Linux, Locality::Remote),
            vec![Requirement::component_version(ComponentId::Uv, ">=0.4")]
        );
        assert_eq!(comp.id(), ComponentId::NoneBot2);
        assert!(comp.supported_targets().contains(&(Os::Windows, Locality::Local)));
        assert!(!comp.supported_targets().contains(&(Os::Windows, Locality::Remote)));
    }

    #[test]
    fn instance_paths_follow_locked_layout() {
        let comp = NoneBot2Component::new(HostPath::from_posix("/home/u/ncd/apps/nonebot2/n1"), 8081);
        assert_eq!(
            comp.env_prod_file().as_posix(),
            "/home/u/ncd/apps/nonebot2/n1/.env.prod"
        );
        assert_eq!(
            comp.venv_python(Os::Linux).as_posix(),
            "/home/u/ncd/apps/nonebot2/n1/.venv/bin/python"
        );
        let cmd = comp.launch_with(Os::Linux, &LaunchArgs::default());
        assert_eq!(cmd.program, "/home/u/ncd/apps/nonebot2/n1/.venv/bin/python");
        assert_eq!(cmd.args, vec!["-u".to_string(), "bot.py".to_string()]);
    }

    #[test]
    fn pyproject_declares_onebot_adapter_and_fastapi_driver() {
        let text = NoneBot2Component::render_pyproject("nonebot2-n1");
        assert!(text.contains("name = \"nonebot2-n1\""));
        assert!(text.contains("\"nonebot2[fastapi]\""));
        assert!(text.contains("\"nonebot-adapter-onebot\""));
        assert!(text.contains("module_name = \"nonebot.adapters.onebot.v11\""));
        assert!(!text.contains("[build-system]"), "无 build-system → uv 视为虚拟项目，不打包");
        assert!(NoneBot2Component::render_bot_py().contains("register_adapter(OneBotV11Adapter)"));
    }

    #[test]
    fn lock_version_reads_only_target_package_block() {
        let lock = r#"version = 1
requires-python = ">=3.10"

[[package]]
name = "fastapi"
version = "0.115.0"
source = { registry = "https://pypi.org/simple" }

[[package]]
name = "nonebot2"
version = "2.4.2"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "pydantic" },
]

[[package]]
name = "nonebot-adapter-onebot"
version = "2.4.6"
"#;
        assert_eq!(
            NoneBot2Component::lock_package_version(lock, "nonebot2"),
            Some("2.4.2".to_string())
        );
        assert_eq!(
            NoneBot2Component::lock_package_version(lock, "nonebot-adapter-onebot"),
            Some("2.4.6".to_string())
        );
        assert_eq!(NoneBot2Component::lock_package_version(lock, "httpx"), None);
        assert_eq!(NoneBot2Component::lock_package_version("", "nonebot2"), None);
    }
}

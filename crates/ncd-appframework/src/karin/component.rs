//! KarinComponent：一个应用实例目录 = 一个 Karin 项目（node-karin 装在项目内）。
//!
//! 安装流程对齐上游 create-karin：
//! 1. 解析 Node（调用方指定 → 上次安装记录 `.ncd-node` → PATH），记下所用 node，装项目私有 pnpm 到 `.ncd-tools/`
//! 2. `pnpm install node-karin@<ver>`（`.npmrc` 预写 `lockfile=false`，避免 karin init 去找 PATH 上的 pnpm）
//! 3. `karin init`（生成 .env / @karinjs/config / index.mjs / pnpm-workspace.yaml）
//! 4. 再跑一次 `pnpm install` 让 onlyBuiltDependencies 里的原生依赖完成构建（上游同样二次 install）
//! 5. 写 `.env` 的 HTTP_PORT / HTTP_HOST
//!
//! 探测：`node_modules/node-karin/package.json` 的 version；有包无 `.env` 视为「未初始化」（Unusable）。

use async_trait::async_trait;
use ncd_component::{
    ActionCtx, ActionError, Component, ComponentId, DetectOutcome, DetectedVersion, LaunchArgs,
    ProgressKind, Requirement, UnusableInstall, VerifyReport,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use std::time::Duration;

use super::manifest::{
    ENV_HTTP_HOST, ENV_HTTP_PORT, KARIN_CLI_ENTRY, KARIN_DIRECT_ENTRY, KARIN_ENTRY_INDEX,
    KARIN_ENV_FILE, KARIN_NODE_VERSION_RANGE, KARIN_NPM_PACKAGE, KARIN_PACKAGE_JSON,
};
use crate::env_file::EnvFile;
use crate::node_tooling::{
    NodeToolchain, TOOLS_DIR, local_path_env, path_prefix, pnpm_command, read_node_marker,
    resolve_node_toolchain, write_node_marker,
};

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

/// pnpm 大版本固定 10：Karin init 写的是 pnpm 10 的 workspace 字段
const PNPM_SPEC: &str = "pnpm@10";
const LONG_STEP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
pub struct KarinComponent {
    pub install_dir: HostPath,
    pub port: u16,
    /// 桌面端管理的 Node；None 只看 PATH
    pub node_bin: Option<HostPath>,
    pub npm_registry: Option<String>,
    /// node-karin 版本规格（默认 latest）
    pub version_spec: String,
    /// 创建时一并装 `@karinjs/plugin-puppeteer`（会下 Chromium）
    pub install_renderer: bool,
}

pub fn renderer_pnpm_args(workspace: bool) -> Vec<String> {
    let mut args = vec![
        "add".into(),
        "@karinjs/plugin-puppeteer".into(),
        "--save".into(),
    ];
    if workspace {
        args.push("-w".into());
    }
    args
}

impl KarinComponent {
    pub fn new(install_dir: HostPath, port: u16) -> Self {
        Self {
            install_dir,
            port,
            node_bin: None,
            npm_registry: None,
            version_spec: "latest".to_string(),
            install_renderer: false,
        }
    }

    pub fn with_install_renderer(mut self, install_renderer: bool) -> Self {
        self.install_renderer = install_renderer;
        self
    }

    pub fn with_node_bin(mut self, node_bin: Option<HostPath>) -> Self {
        self.node_bin = node_bin;
        self
    }

    pub fn with_npm_registry(mut self, registry: Option<String>) -> Self {
        self.npm_registry = registry.filter(|s| !s.trim().is_empty());
        self
    }

    pub fn with_version_spec(mut self, spec: impl Into<String>) -> Self {
        self.version_spec = spec.into();
        self
    }

    pub fn package_json(&self) -> HostPath {
        self.install_dir.join(KARIN_PACKAGE_JSON)
    }

    pub fn env_file(&self) -> HostPath {
        self.install_dir.join(KARIN_ENV_FILE)
    }

    pub fn direct_entry(&self) -> HostPath {
        self.install_dir.join(KARIN_DIRECT_ENTRY)
    }

    fn cli_entry(&self) -> HostPath {
        self.install_dir.join(KARIN_CLI_ENTRY)
    }

    fn tools_dir(&self) -> HostPath {
        self.install_dir.join(TOOLS_DIR)
    }

    /// node 候选顺序：调用方指定（桌面端管理 / 用户覆盖）→ 上次安装记录 → PATH（由 resolve 兜底）
    async fn preferred_nodes(&self, host: &dyn Host) -> Vec<HostPath> {
        let mut out = Vec::with_capacity(2);
        if let Some(p) = &self.node_bin {
            out.push(p.clone());
        }
        if let Some(p) = read_node_marker(host, &self.install_dir).await {
            out.push(p);
        }
        out
    }

    /// 解析工具链并拼出直启命令（运行时调用；比 trait 的同步 launch_command 更准）
    pub async fn resolve_launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let preferred = self.preferred_nodes(host).await;
        let tc = resolve_node_toolchain(host, &preferred).await?;
        Ok(self.launch_with(host, &tc, args))
    }

    fn launch_with(&self, host: &dyn Host, tc: &NodeToolchain, args: &LaunchArgs) -> HostCommand {
        let prefix = path_prefix(tc, &self.install_dir, host.os());
        let path_env = match host.locality() {
            Locality::Local => local_path_env(&prefix, host.os()),
            // 远端守护进程给一份确定的 PATH；私有 pnpm shim 在前，Karin 的插件安装才能找到 pnpm
            Locality::Remote => {
                let mut parts = prefix;
                parts.extend(
                    ["/usr/local/bin", "/usr/bin", "/bin"]
                        .into_iter()
                        .map(String::from),
                );
                parts.join(":")
            }
        };
        let cmd = HostCommand::new(tc.node_bin.as_posix())
            .arg(self.direct_entry().render_for(host.os()))
            .working_dir(self.install_dir.clone())
            .env("NODE_ENV", "production")
            .env("PATH", path_env)
            .long_running();
        args.apply_to(cmd)
    }

    fn registry_arg(&self) -> Vec<String> {
        self.npm_registry
            .as_ref()
            .map(|r| vec![format!("--registry={r}")])
            .unwrap_or_default()
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
            .env("CI", "true")
            .env("npm_config_update_notifier", "false");
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

    /// 首装/更新共用：装私有 pnpm → 装 node-karin → karin init → 二次 install → 写 .env
    async fn provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        let total = if self.install_renderer { 7 } else { 6 };
        ctx.emit(ProgressKind::Started { total_steps: total }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "解析 Node.js".to_string(),
        })
        .await;
        let preferred = self.preferred_nodes(host).await;
        let tc = resolve_node_toolchain(host, &preferred).await?;
        ctx.info(format!("Node.js: {}", tc.node_bin.render_for(host.os()))).await;
        host.create_dir_all(&self.install_dir).await?;
        // 记下本次 node，起停 / 探测复用；远端 SSH 非交互会话 PATH 里往往没有 node
        write_node_marker(host, &self.install_dir, &tc).await?;
        self.ensure_project_scaffold(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // 2. 项目私有 pnpm（npm --prefix，不动全局）
        let tools = self.tools_dir();
        host.create_dir_all(&tools).await?;
        let mut npm_args: Vec<String> = vec![
            "install".into(),
            PNPM_SPEC.into(),
            "--prefix".into(),
            tools.render_for(host.os()),
            "--no-audit".into(),
            "--no-fund".into(),
            "--loglevel=error".into(),
        ];
        npm_args.extend(self.registry_arg());
        let npm_cmd = tc
            .npm(host.os(), &npm_args.iter().map(String::as_str).collect::<Vec<_>>())
            .working_dir(self.install_dir.clone());
        self.run_step(host, ctx, 2, "安装项目私有 pnpm", npm_cmd)
            .await?;

        // 3. node-karin
        let spec = format!("{KARIN_NPM_PACKAGE}@{}", self.version_spec);
        let mut args: Vec<String> = vec!["install".into(), spec];
        args.extend(self.registry_arg());
        let cmd = pnpm_command(
            &tc,
            &self.install_dir,
            host.os(),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        self.run_step(host, ctx, 3, "安装 node-karin", cmd).await?;

        // 4. karin init（生成 .env / @karinjs/config / index.mjs）
        let init = HostCommand::new(tc.node_bin.as_posix())
            .arg(self.cli_entry().render_for(host.os()))
            .arg("init")
            .working_dir(self.install_dir.clone())
            .env("KARIN_CLI", "true")
            .env("INIT_CWD", self.install_dir.render_for(host.os()));
        self.run_step(host, ctx, 4, "初始化 Karin 项目", init).await?;

        // 5. 二次 install：workspace 已声明 onlyBuiltDependencies，原生依赖此时才允许构建
        let mut again: Vec<String> = vec!["install".into()];
        again.extend(self.registry_arg());
        let cmd = pnpm_command(
            &tc,
            &self.install_dir,
            host.os(),
            &again.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        self.run_step(host, ctx, 5, "构建原生依赖", cmd).await?;

        // 6. .env 端口 / 监听地址
        ctx.emit(ProgressKind::StepBegin {
            step: 6,
            message: "写入实例端口".to_string(),
        })
        .await;
        self.write_env_port(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 6, ok: true }).await;

        if self.install_renderer {
            let has_workspace = host
                .exists(&self.install_dir.join("pnpm-workspace.yaml"))
                .await?;
            let mut args = renderer_pnpm_args(has_workspace);
            args.extend(self.registry_arg());
            let cmd = pnpm_command(
                &tc,
                &self.install_dir,
                host.os(),
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            self.run_step(host, ctx, 7, "安装插件版渲染器", cmd).await?;
        }

        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// package.json / .npmrc 预置：没有才写，不覆盖用户已有内容
    async fn ensure_project_scaffold(&self, host: &dyn Host) -> Result<(), ActionError> {
        let pkg = self.install_dir.join("package.json");
        if !host.exists(&pkg).await? {
            let name = self
                .install_dir
                .file_name()
                .map(|n| format!("karin-{n}"))
                .unwrap_or_else(|| "karin-project".to_string());
            let body = serde_json::json!({
                "name": name,
                "version": "1.0.0",
                "private": true,
                "type": "module"
            });
            host.write_file(&pkg, format!("{body:#}\n").as_bytes()).await?;
        }
        let npmrc = self.install_dir.join(".npmrc");
        if !host.exists(&npmrc).await? {
            // lockfile=false：karin init 见到 pnpm-lock.yaml 会去 PATH 找 pnpm 重装，我们没有全局 pnpm
            let mut content = String::from("lockfile=false\n");
            if let Some(r) = &self.npm_registry {
                content.push_str(&format!("registry={r}\n"));
            }
            host.write_file(&npmrc, content.as_bytes()).await?;
        }
        Ok(())
    }

    async fn write_env_port(&self, host: &dyn Host) -> Result<(), ActionError> {
        let env_path = self.env_file();
        let text = host
            .read_file(&env_path)
            .await
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| ActionError::install_step("write-env", e.to_string()))?;
        let mut env = EnvFile::parse(&text);
        env.set(ENV_HTTP_PORT, &self.port.to_string());
        if host.locality() == Locality::Local {
            // 本机只需同机对接 + 本机 WebUI，不暴露到局域网
            env.set(ENV_HTTP_HOST, "127.0.0.1");
        }
        host.write_file(&env_path, env.render().as_bytes()).await?;
        Ok(())
    }

    async fn read_installed_version(&self, host: &dyn Host) -> Result<Option<String>, ActionError> {
        let pkg = self.package_json();
        if !host.exists(&pkg).await? {
            return Ok(None);
        }
        let bytes = host.read_file(&pkg).await?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| ActionError::detect_failed("karin", format!("package.json 解析失败: {e}")))?;
        Ok(value
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string))
    }
}

#[async_trait]
impl Component for KarinComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Karin
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        vec![Requirement::component_version(
            ComponentId::NodeJs,
            KARIN_NODE_VERSION_RANGE,
        )]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        let Some(version) = self.read_installed_version(host).await? else {
            return Ok(DetectOutcome::NotInstalled);
        };
        if !host.exists(&self.env_file()).await? {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version: Some(version),
                reason: "node-karin 已安装但项目未初始化（缺 .env），重新安装可修复".to_string(),
            }));
        }
        Ok(DetectOutcome::Installed(DetectedVersion {
            version,
            source: self.package_json().as_posix().to_string(),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.provision(host, ctx).await
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        // 同一套流程：pnpm install node-karin@latest 会升级到最新，karin init 合并新配置键
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
            ("node-karin package.json", self.package_json()),
            (".env", self.env_file()),
            (KARIN_ENTRY_INDEX, self.install_dir.join(KARIN_ENTRY_INDEX)),
            ("start/app.mjs", self.direct_entry()),
        ] {
            let ok = host.exists(&path).await?;
            report = report.with_check(name, ok, Some(path.as_posix().to_string()));
        }
        Ok(report)
    }

    fn launch_command(&self, host: &dyn Host, args: &LaunchArgs) -> Result<HostCommand, ActionError> {
        // 同步版只能用已知 node 路径或 PATH；运行时请用 resolve_launch_command
        let node = self
            .node_bin
            .as_ref()
            .map(|p| p.as_posix().to_string())
            .unwrap_or_else(|| "node".to_string());
        let npm_cli = crate::node_tooling::npm_cli_for(&HostPath::from_posix(&node), host.os());
        let tc = NodeToolchain {
            node_bin: HostPath::from_posix(node),
            npm_cli,
        };
        Ok(self.launch_with(host, &tc, args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_pnpm_args_adds_workspace_flag() {
        assert_eq!(
            renderer_pnpm_args(false),
            vec!["add".to_string(), "@karinjs/plugin-puppeteer".to_string(), "--save".to_string()]
        );
        assert_eq!(renderer_pnpm_args(true)[3], "-w");
    }

    #[test]
    fn requirements_pin_node_floor_from_upstream() {
        let comp = KarinComponent::new(HostPath::from_posix("/x"), 7777);
        let reqs = comp.requirements(Os::Linux, Locality::Remote);
        assert_eq!(
            reqs,
            vec![Requirement::component_version(ComponentId::NodeJs, ">=18")]
        );
        assert_eq!(comp.id(), ComponentId::Karin);
        assert!(comp.supported_targets().contains(&(Os::Windows, Locality::Local)));
        assert!(!comp.supported_targets().contains(&(Os::Windows, Locality::Remote)));
    }

    #[test]
    fn instance_paths_follow_locked_layout() {
        let comp = KarinComponent::new(HostPath::from_posix("/home/u/ncd/apps/karin/k1"), 7801);
        assert_eq!(
            comp.package_json().as_posix(),
            "/home/u/ncd/apps/karin/k1/node_modules/node-karin/package.json"
        );
        assert_eq!(comp.env_file().as_posix(), "/home/u/ncd/apps/karin/k1/.env");
        assert_eq!(
            comp.direct_entry().as_posix(),
            "/home/u/ncd/apps/karin/k1/node_modules/node-karin/dist/start/app.mjs"
        );
    }
}

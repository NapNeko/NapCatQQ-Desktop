//! YunzaiComponent：一个应用实例目录 = 一份 TRSS-Yunzai 的 git 工作区。
//!
//! 安装（对齐上游 README 的手动安装）：
//! 1. 解析 node / git / redis-server（依赖闭包已经先装好了组件页里的 Git、Redis、Node.js）
//! 2. 几个源并发 `git ls-remote` 挑最快的，`git clone --depth 1` 到实例目录里的 `.ncd-stage/`，
//!    再把顶层条目改名挪上来（实例目录里已经有桌面端的标记文件，不能直接 clone 进去）
//! 3. 项目私有 pnpm（`.ncd-tools/`）→ `pnpm install`。Chrome 一律先跳过：puppeteer 的 postinstall
//!    下载失败会让整个 install 失败；新建时勾了才单独再下，失败只提醒
//! 4. 复制 default_config 到 config/config（上游首启也会做），写监听口、Redis 路径和端口
//!
//! 更新：`git fetch --depth 1` + `reset --hard FETCH_HEAD`（config/config、data、plugins、dump.rdb 都在
//! .gitignore 里，不受影响）→ pnpm install。探测：package.json 的 name 是 trss-yunzai 且 node_modules 在。
//! 启动：`node app.js daemon`，守护进程收 `#重启`（子进程退出再拉起）。

use std::time::Duration;

use async_trait::async_trait;
use ncd_component::{
    ActionCtx, ActionError, Component, ComponentId, DetectOutcome, DetectedVersion, LaunchArgs,
    ProgressKind, Requirement, UnusableInstall, VerifyReport,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use serde_yaml::Value;

use super::config::{config_rel, default_config_rel, url_with_port};
use super::git::{GIT_LONG_TIMEOUT, GitTool, clone_candidates, git_env, last_line};
use super::manifest::{
    YUNZAI_CONFIG_DIR, YUNZAI_CONFIG_NAMES, YUNZAI_DEFAULT_PORT, YUNZAI_ENTRY, YUNZAI_GIT_SOURCES,
    YUNZAI_NODE_VERSION_RANGE, YUNZAI_PACKAGE_JSON, YUNZAI_PACKAGE_NAME,
};
use crate::node_tooling::{
    NodeToolchain, TOOLS_DIR, local_path_env, path_prefix, pnpm_command, read_node_marker,
    resolve_node_toolchain, write_node_marker,
};
use crate::ports::PortUsage;
use crate::yaml_patch;

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

const PNPM_SPEC: &str = "pnpm@10";
const LONG_STEP_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const STAGE_DIR: &str = ".ncd-stage";
/// Redis 口从实例口往上探几个
const REDIS_PORT_PROBES: u16 = 30;

/// Windows 上不下 Chrome 时让 puppeteer 用系统里现成的，按这个顺序找
const WINDOWS_BROWSERS: &[&str] = &[
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
];

#[derive(Debug, Clone)]
pub struct YunzaiComponent {
    pub install_dir: HostPath,
    /// 实例口 = server.yaml 的 port
    pub port: u16,
    pub node_bin: Option<HostPath>,
    pub git_bin: Option<HostPath>,
    pub redis_bin: Option<HostPath>,
    pub npm_registry: Option<String>,
    /// 新建时勾了「一并下载渲染用 Chrome」
    pub download_chrome: bool,
    pub adopt_existing: bool,
}

/// Redis 口的候选：实例口往上几个，再往下几个（实例口各不相同，按它挑两个实例基本不会撞）
pub fn redis_port_candidates(port: u16) -> Vec<u16> {
    let up = (1..=REDIS_PORT_PROBES).filter_map(|i| port.checked_add(i));
    let down = (1..=REDIS_PORT_PROBES).filter_map(|i| port.checked_sub(i));
    up.chain(down).filter(|p| *p > 1024).collect()
}

/// npmmirror 源时 Chrome 也走 npmmirror 的 chrome-for-testing 镜像（Google 的存储国内常连不上）
pub fn chrome_download_base(registry: Option<&str>) -> Option<&'static str> {
    registry
        .filter(|r| r.contains("npmmirror.com") || r.contains("taobao.org"))
        .map(|_| "https://cdn.npmmirror.com/binaries/chrome-for-testing")
}

impl YunzaiComponent {
    pub fn new(install_dir: HostPath, port: u16) -> Self {
        Self {
            install_dir,
            port,
            node_bin: None,
            git_bin: None,
            redis_bin: None,
            npm_registry: None,
            download_chrome: false,
            adopt_existing: false,
        }
    }

    pub fn with_node_bin(mut self, bin: Option<HostPath>) -> Self {
        self.node_bin = bin;
        self
    }

    pub fn with_git_bin(mut self, bin: Option<HostPath>) -> Self {
        self.git_bin = bin;
        self
    }

    pub fn with_redis_bin(mut self, bin: Option<HostPath>) -> Self {
        self.redis_bin = bin;
        self
    }

    pub fn with_npm_registry(mut self, registry: Option<String>) -> Self {
        self.npm_registry = registry.filter(|s| !s.trim().is_empty());
        self
    }

    pub fn with_download_chrome(mut self, on: bool) -> Self {
        self.download_chrome = on;
        self
    }

    pub fn with_adopt_existing(mut self, adopt: bool) -> Self {
        self.adopt_existing = adopt;
        self
    }

    fn package_json(&self) -> HostPath {
        self.install_dir.join(YUNZAI_PACKAGE_JSON)
    }

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

    /// 能写进 redis.yaml 的 redis-server：托管的在就用它，否则 PATH 上的（写绝对路径）
    pub async fn resolve_redis(&self, host: &dyn Host) -> Result<HostPath, ActionError> {
        if let Some(bin) = &self.redis_bin {
            if host.exists(bin).await? {
                return Ok(bin.clone());
            }
        }
        for name in ["redis-server", "valkey-server"] {
            if let Some(found) = host.which(name).await? {
                return Ok(match host.os() {
                    Os::Windows => HostPath::from_windows(&found),
                    _ => HostPath::from_posix(found),
                });
            }
        }
        Err(ActionError::install_step(
            "resolve-redis",
            "没找到 redis-server：先在组件页装好「Redis」再装云崽",
        ))
    }

    async fn resolve_git(&self, host: &dyn Host) -> Result<GitTool, ActionError> {
        let git = GitTool::resolve(host, self.git_bin.as_ref())
            .await
            .map_err(|e| ActionError::install_step("resolve-git", e))?;
        self.write_git_marker(host, &git).await?;
        Ok(git)
    }

    /// 商店装插件时拿不到组件目录：托管的 MinGit 路径记在实例里，PATH 上的 git 不用记
    async fn write_git_marker(&self, host: &dyn Host, git: &GitTool) -> Result<(), ActionError> {
        let marker = self.install_dir.join(super::store::GIT_MARKER_FILE);
        if git.dir.is_some() {
            host.create_dir_all(&self.install_dir).await?;
            host.write_file(&marker, format!("{}\n", git.program).as_bytes())
                .await?;
        } else if host.exists(&marker).await? {
            host.remove_file(&marker).await?;
        }
        Ok(())
    }

    /// 起停 / 终端用：解析工具链拼出 `node app.js daemon`
    pub async fn resolve_launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let preferred = self.preferred_nodes(host).await;
        let tc = resolve_node_toolchain(host, &preferred).await?;
        let git = GitTool::resolve(host, self.git_bin.as_ref()).await.ok();
        Ok(self.launch_with(host, &tc, git.as_ref(), args))
    }

    /// 进程和它自己调的 pnpm / git 用的 PATH：私有 pnpm、node、托管 git 在前
    pub fn path_entries(
        &self,
        host: &dyn Host,
        tc: &NodeToolchain,
        git: Option<&GitTool>,
    ) -> Vec<String> {
        let mut prefix = path_prefix(tc, &self.install_dir, host.os());
        if let Some(dir) = git.and_then(|g| g.dir.as_ref()) {
            prefix.push(dir.render_for(host.os()));
        }
        prefix
    }

    fn launch_with(
        &self,
        host: &dyn Host,
        tc: &NodeToolchain,
        git: Option<&GitTool>,
        args: &LaunchArgs,
    ) -> HostCommand {
        let prefix = self.path_entries(host, tc, git);
        let path_env = match host.locality() {
            Locality::Local => local_path_env(&prefix, host.os()),
            Locality::Remote => {
                let mut parts = prefix;
                parts.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(String::from));
                parts.join(":")
            }
        };
        let cmd = HostCommand::new(tc.node_bin.as_posix())
            .arg(self.install_dir.join(YUNZAI_ENTRY).render_for(host.os()))
            .arg("daemon")
            .working_dir(self.install_dir.clone())
            .env("PATH", path_env)
            .envs(git_env())
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
        let result = self.run_logged(host, ctx, name, cmd).await;
        ctx.emit(ProgressKind::StepEnd {
            step,
            ok: result.is_ok(),
        })
        .await;
        result
    }

    /// 跑一条命令，逐行进安装日志；失败带上最后一行 stderr
    async fn run_logged(
        &self,
        host: &dyn Host,
        ctx: &ActionCtx,
        name: &str,
        cmd: HostCommand,
    ) -> Result<(), ActionError> {
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
            return Err(ActionError::install_step(
                name,
                format!("exit={:?}: {}", out.exit_code, last_line(&out.stderr)),
            ));
        }
        Ok(())
    }

    /// 挑源 → clone 到 stage → 顶层条目挪进实例目录。挑中的源 clone 失败就按顺序试别的
    async fn fetch_sources(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
        git: &GitTool,
    ) -> Result<(), ActionError> {
        let sources: Vec<String> = YUNZAI_GIT_SOURCES.iter().map(|s| s.to_string()).collect();
        let first = match git.pick_fastest(host, &sources).await {
            Ok(url) => url,
            Err(e) => {
                ctx.warn(format!("挑源失败：{e}；按默认顺序逐个试")).await;
                sources[0].clone()
            }
        };
        ctx.info(format!("源：{first}")).await;
        let mut order = vec![first.clone()];
        order.extend(
            clone_candidates(&sources)
                .into_iter()
                .filter(|u| *u != first),
        );

        let stage = self.install_dir.join(STAGE_DIR);
        let checkout = stage.join("Yunzai");
        let mut last_err = None;
        for url in order {
            let _ = host.remove_dir_all(&stage).await;
            host.create_dir_all(&stage).await?;
            let cmd = git
                .command([
                    "clone",
                    "--depth",
                    "1",
                    "--single-branch",
                    url.as_str(),
                    checkout.render_for(host.os()).as_str(),
                ])
                .working_dir(self.install_dir.clone())
                .timeout(GIT_LONG_TIMEOUT);
            match self.run_logged(host, ctx, "git clone", cmd).await {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(e) => {
                    ctx.warn(format!("{url} 拉取失败：{e}")).await;
                    last_err = Some(e);
                }
            }
        }
        if let Some(e) = last_err {
            let _ = host.remove_dir_all(&stage).await;
            return Err(e);
        }
        for entry in host.list_dir(&checkout).await? {
            let dest = self.install_dir.join(&entry.name);
            if host.exists(&dest).await? {
                if entry.is_dir {
                    host.remove_dir_all(&dest).await?;
                } else {
                    host.remove_file(&dest).await?;
                }
            }
            host.rename(&checkout.join(&entry.name), &dest).await?;
        }
        let _ = host.remove_dir_all(&stage).await;
        Ok(())
    }

    async fn install_private_pnpm(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
        tc: &NodeToolchain,
        step: u32,
    ) -> Result<(), ActionError> {
        let tools = self.install_dir.join(TOOLS_DIR);
        host.create_dir_all(&tools).await?;
        let mut args: Vec<String> = vec![
            "install".into(),
            PNPM_SPEC.into(),
            "--prefix".into(),
            tools.render_for(host.os()),
            "--no-audit".into(),
            "--no-fund".into(),
            "--loglevel=error".into(),
        ];
        args.extend(self.registry_arg());
        let cmd = tc
            .npm(
                host.os(),
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            )
            .working_dir(self.install_dir.clone());
        self.run_step(host, ctx, step, "安装项目私有 pnpm", cmd)
            .await
    }

    async fn pnpm_install(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
        tc: &NodeToolchain,
        git: Option<&GitTool>,
        step: u32,
    ) -> Result<(), ActionError> {
        // 没有锁文件，CI 下 pnpm 默认 frozen 会直接报错；plugins/** 是 workspace，插件依赖一起装
        let mut args: Vec<String> = vec!["install".into(), "--no-frozen-lockfile".into()];
        args.extend(self.registry_arg());
        let cmd = pnpm_command(
            tc,
            &self.install_dir,
            host.os(),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .env("PUPPETEER_SKIP_DOWNLOAD", "true")
        .env("PATH", self.tool_path(host, tc, git));
        self.run_step(host, ctx, step, "安装依赖", cmd).await
    }

    /// 装依赖时的 PATH：有的包 postinstall 要调 git / node
    fn tool_path(&self, host: &dyn Host, tc: &NodeToolchain, git: Option<&GitTool>) -> String {
        let prefix = self.path_entries(host, tc, git);
        match host.locality() {
            Locality::Local => local_path_env(&prefix, host.os()),
            Locality::Remote => {
                let mut parts = prefix;
                parts.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(String::from));
                parts.join(":")
            }
        }
    }

    /// 单独下 puppeteer 要的 Chrome；失败不挡安装（shotium 渲染器不要浏览器，静态模板照样出图）
    async fn download_chrome_step(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
        tc: &NodeToolchain,
        step: u32,
    ) -> bool {
        ctx.emit(ProgressKind::StepBegin {
            step,
            message: "下载渲染用 Chrome".into(),
        })
        .await;
        let mut cmd = pnpm_command(
            tc,
            &self.install_dir,
            host.os(),
            &["exec", "puppeteer", "browsers", "install", "chrome"],
        );
        if let Some(base) = chrome_download_base(self.npm_registry.as_deref()) {
            cmd = cmd.env("PUPPETEER_DOWNLOAD_BASE_URL", base);
        }
        let ok = match self.run_logged(host, ctx, "下载 Chrome", cmd).await {
            Ok(()) => true,
            Err(e) => {
                ctx.warn(format!(
                    "Chrome 没下下来（{e}）。不影响启动，要浏览器的模板会退到 shotium 渲染，之后可在「渲染」页填本机浏览器路径"
                ))
                .await;
                false
            }
        };
        // 下失败也算这一步走完了：整体安装照常成功
        ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
        ok
    }

    async fn read_text(&self, host: &dyn Host, rel: &str) -> Result<Option<String>, ActionError> {
        let path = self.install_dir.join(rel);
        if !host.exists(&path).await? {
            return Ok(None);
        }
        let bytes = host.read_file(&path).await?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }

    /// 上游首启会做的那一步先做掉：缺哪份就从 default_config 抄哪份
    async fn copy_default_configs(&self, host: &dyn Host) -> Result<(), ActionError> {
        host.create_dir_all(&self.install_dir.join(YUNZAI_CONFIG_DIR))
            .await?;
        for name in YUNZAI_CONFIG_NAMES {
            let dest = self.install_dir.join(config_rel(name));
            if host.exists(&dest).await? {
                continue;
            }
            if let Some(text) = self.read_text(host, &default_config_rel(name)).await? {
                host.write_file(&dest, text.as_bytes()).await?;
            }
        }
        Ok(())
    }

    async fn patch_config(
        &self,
        host: &dyn Host,
        name: &str,
        edits: &[(&[&str], Value)],
    ) -> Result<(), ActionError> {
        let rel = config_rel(name);
        let mut text = self.read_text(host, &rel).await?.unwrap_or_default();
        for (path, value) in edits {
            text = yaml_patch::set_value(&text, path, value)
                .map_err(|e| ActionError::install_step("write-config", format!("{rel}：{e}")))?;
        }
        host.write_file(&self.install_dir.join(rel), text.as_bytes())
            .await?;
        Ok(())
    }

    async fn pick_redis_port(&self, host: &dyn Host) -> u16 {
        let candidates = redis_port_candidates(self.port);
        if let Some(usage) = PortUsage::probe(host).await
            && let Some(port) = candidates.iter().copied().find(|p| usage.is_free(*p))
        {
            return port;
        }
        candidates.first().copied().unwrap_or(6380)
    }

    /// 新装：监听口、文件链接地址、Redis 路径与端口；没下 Chrome 的 Windows 指一个现成的浏览器
    async fn seed_fresh(
        &self,
        host: &dyn Host,
        ctx: &ActionCtx,
        redis: &HostPath,
        chrome_ok: bool,
    ) -> Result<(), ActionError> {
        self.copy_default_configs(host).await?;
        let url = format!("http://localhost:{}", self.port);
        self.patch_config(
            host,
            "server",
            &[
                (&["port"], Value::Number(self.port.into())),
                (&["url"], Value::String(url)),
            ],
        )
        .await?;
        let redis_port = self.pick_redis_port(host).await;
        self.patch_config(
            host,
            "redis",
            &[
                (&["path"], Value::String(redis.render_for(host.os()))),
                (&["port"], Value::Number(redis_port.into())),
            ],
        )
        .await?;
        ctx.info(format!("云崽端口 {}，Redis 端口 {redis_port}", self.port))
            .await;
        if host.os() == Os::Windows && !chrome_ok {
            for candidate in WINDOWS_BROWSERS {
                let path = HostPath::from_windows(candidate);
                if host.exists(&path).await? {
                    self.patch_config(
                        host,
                        "bot",
                        &[(&["chromium_path"], Value::String((*candidate).into()))],
                    )
                    .await?;
                    ctx.info(format!("渲染用本机浏览器：{candidate}")).await;
                    break;
                }
            }
        }
        Ok(())
    }

    /// 更新 / 导入：只补 redis.yaml 的 path——还是上游默认的 `redis-server` 而 PATH 上没有时指向托管的
    async fn repair_redis_path(&self, host: &dyn Host, ctx: &ActionCtx) -> Result<(), ActionError> {
        self.copy_default_configs(host).await?;
        let text = self
            .read_text(host, &config_rel("redis"))
            .await?
            .unwrap_or_default();
        let current = serde_yaml::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("path").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| "redis-server".into());
        let runnable = if current.contains('/') || current.contains('\\') {
            let path = match host.os() {
                Os::Windows => HostPath::from_windows(&current),
                _ => HostPath::from_posix(current.clone()),
            };
            host.exists(&path).await?
        } else {
            host.command_exists(&current).await
        };
        if runnable {
            return Ok(());
        }
        if let Ok(redis) = self.resolve_redis(host).await {
            self.patch_config(
                host,
                "redis",
                &[(&["path"], Value::String(redis.render_for(host.os())))],
            )
            .await?;
            ctx.info(format!(
                "redis.yaml 的 path 改成 {}",
                redis.render_for(host.os())
            ))
            .await;
        }
        Ok(())
    }

    async fn begin(&self, ctx: &mut ActionCtx, step: u32, message: &str) {
        ctx.emit(ProgressKind::StepBegin {
            step,
            message: message.to_string(),
        })
        .await;
    }

    async fn end(&self, ctx: &mut ActionCtx, step: u32) {
        ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
    }

    async fn provision_fresh(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        let total = if self.download_chrome { 6 } else { 5 };
        ctx.emit(ProgressKind::Started { total_steps: total }).await;

        self.begin(ctx, 1, "解析 Node.js / Git / Redis").await;
        let tc = resolve_node_toolchain(host, &self.preferred_nodes(host).await).await?;
        let git = self.resolve_git(host).await?;
        let redis = self.resolve_redis(host).await?;
        ctx.info(format!(
            "Node.js: {}；git: {}；redis-server: {}",
            tc.node_bin.render_for(host.os()),
            git.program,
            redis.render_for(host.os())
        ))
        .await;
        host.create_dir_all(&self.install_dir).await?;
        write_node_marker(host, &self.install_dir, &tc).await?;
        self.end(ctx, 1).await;

        self.begin(ctx, 2, "拉取 TRSS-Yunzai 源码").await;
        self.fetch_sources(host, ctx, &git).await?;
        self.end(ctx, 2).await;

        self.install_private_pnpm(host, ctx, &tc, 3).await?;
        self.pnpm_install(host, ctx, &tc, Some(&git), 4).await?;

        let mut step = 5;
        let chrome_ok = if self.download_chrome {
            let ok = self.download_chrome_step(host, ctx, &tc, step).await;
            step += 1;
            ok
        } else {
            false
        };

        self.begin(ctx, step, "写入实例配置").await;
        self.seed_fresh(host, ctx, &redis, chrome_ok).await?;
        self.end(ctx, step).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn provision_update(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 5 }).await;
        self.begin(ctx, 1, "解析 Node.js / Git").await;
        let tc = resolve_node_toolchain(host, &self.preferred_nodes(host).await).await?;
        let git = self.resolve_git(host).await?;
        write_node_marker(host, &self.install_dir, &tc).await?;
        self.end(ctx, 1).await;

        self.begin(ctx, 2, "拉取最新代码").await;
        if host.exists(&self.install_dir.join(".git")).await? {
            let dir = self.install_dir.render_for(host.os());
            let fetch = git
                .command(["-C", dir.as_str(), "fetch", "--depth", "1", "origin"])
                .timeout(GIT_LONG_TIMEOUT);
            self.run_logged(host, ctx, "git fetch", fetch).await?;
            let reset = git.command(["-C", dir.as_str(), "reset", "--hard", "FETCH_HEAD"]);
            self.run_logged(host, ctx, "git reset", reset).await?;
        } else {
            ctx.warn("实例目录不是 git 仓库，跳过拉代码，只同步依赖")
                .await;
        }
        self.end(ctx, 2).await;

        self.install_private_pnpm(host, ctx, &tc, 3).await?;
        self.pnpm_install(host, ctx, &tc, Some(&git), 4).await?;
        self.begin(ctx, 5, "检查配置").await;
        self.repair_redis_path(host, ctx).await?;
        self.end(ctx, 5).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 导入已有项目：不拉代码、不改端口，只把依赖和 Redis 路径补齐
    async fn provision_adopt(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;
        self.begin(ctx, 1, "解析 Node.js / Git").await;
        let tc = resolve_node_toolchain(host, &self.preferred_nodes(host).await).await?;
        let git = GitTool::resolve(host, self.git_bin.as_ref()).await.ok();
        write_node_marker(host, &self.install_dir, &tc).await?;
        self.end(ctx, 1).await;
        self.install_private_pnpm(host, ctx, &tc, 2).await?;
        self.pnpm_install(host, ctx, &tc, git.as_ref(), 3).await?;
        self.begin(ctx, 4, "检查配置").await;
        self.repair_redis_path(host, ctx).await?;
        self.end(ctx, 4).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn read_package(&self, host: &dyn Host) -> Result<Option<(String, String)>, ActionError> {
        let Some(text) = self.read_text(host, YUNZAI_PACKAGE_JSON).await? else {
            return Ok(None);
        };
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            ActionError::detect_failed("yunzai", format!("package.json 解析失败: {e}"))
        })?;
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let version = value
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        Ok(Some((name.to_string(), version.to_string())))
    }
}

#[async_trait]
impl Component for YunzaiComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Yunzai
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        vec![
            Requirement::component_version(ComponentId::NodeJs, YUNZAI_NODE_VERSION_RANGE),
            Requirement::component(ComponentId::Git),
            Requirement::component(ComponentId::Redis),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        let Some((name, version)) = self.read_package(host).await? else {
            return Ok(DetectOutcome::NotInstalled);
        };
        if name != YUNZAI_PACKAGE_NAME {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.package_json().as_posix().to_string(),
                version: Some(version),
                reason: format!("不是 TRSS-Yunzai（package.json 的 name 是 {name}）"),
            }));
        }
        if !host.exists(&self.install_dir.join("node_modules")).await? {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version: Some(version),
                reason: "源码在但依赖还没装，重新安装可修复".into(),
            }));
        }
        Ok(DetectOutcome::Installed(DetectedVersion {
            version,
            source: self.package_json().as_posix().to_string(),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        if self.adopt_existing {
            return self.provision_adopt(host, ctx).await;
        }
        // 装到一半重试：代码已经在了就不再 clone（clone 进非空目录会失败），走更新那条
        if matches!(self.read_package(host).await?, Some((name, _)) if name == YUNZAI_PACKAGE_NAME)
        {
            return self.provision_update(host, ctx).await;
        }
        self.provision_fresh(host, ctx).await
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        if self.adopt_existing {
            return self.provision_adopt(host, ctx).await;
        }
        self.provision_update(host, ctx).await
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        self.begin(ctx, 1, &format!("删除 {}", self.install_dir.as_posix()))
            .await;
        if host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        }
        self.end(ctx, 1).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let mut report = VerifyReport::ok();
        for (name, rel) in [
            ("package.json", YUNZAI_PACKAGE_JSON.to_string()),
            (YUNZAI_ENTRY, YUNZAI_ENTRY.to_string()),
            ("node_modules", "node_modules".to_string()),
            ("server.yaml", config_rel("server")),
        ] {
            let path = self.install_dir.join(&rel);
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
        // 同步版只能用已知路径或 PATH；运行时走 resolve_launch_command
        let node = self
            .node_bin
            .as_ref()
            .map(|p| p.as_posix().to_string())
            .unwrap_or_else(|| "node".to_string());
        let node_bin = HostPath::from_posix(node);
        let tc = NodeToolchain {
            npm_cli: crate::node_tooling::npm_cli_for(&node_bin, host.os()),
            node_bin,
        };
        Ok(self.launch_with(host, &tc, None, args))
    }
}

/// 端口没改过的老配置（`http://localhost:2536`）跟着实例口走；给导入项目的探测用
pub fn url_for_port(current: &str, port: u16) -> String {
    url_with_port(current, YUNZAI_DEFAULT_PORT, port).unwrap_or_else(|| current.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_pin_node_git_redis() {
        let comp = YunzaiComponent::new(HostPath::from_posix("/x"), 2536);
        let reqs = comp.requirements(Os::Linux, Locality::Remote);
        assert_eq!(
            reqs,
            vec![
                Requirement::component_version(ComponentId::NodeJs, ">=22.12.0"),
                Requirement::component(ComponentId::Git),
                Requirement::component(ComponentId::Redis),
            ]
        );
        assert_eq!(comp.id(), ComponentId::Yunzai);
        assert!(
            !comp
                .supported_targets()
                .contains(&(Os::Windows, Locality::Remote))
        );
    }

    #[test]
    fn redis_ports_stay_near_instance_port_and_skip_privileged() {
        let c = redis_port_candidates(24100);
        assert_eq!(c[0], 24101);
        assert!(!c.contains(&24100));
        assert!(c.contains(&24099));
        assert!(redis_port_candidates(1030).iter().all(|p| *p > 1024));
        assert!(
            redis_port_candidates(u16::MAX)
                .iter()
                .all(|p| *p < u16::MAX)
        );
    }

    #[test]
    fn chrome_mirror_only_for_npmmirror() {
        assert_eq!(
            chrome_download_base(Some("https://registry.npmmirror.com")),
            Some("https://cdn.npmmirror.com/binaries/chrome-for-testing")
        );
        assert_eq!(
            chrome_download_base(Some("https://registry.npmjs.org")),
            None
        );
        assert_eq!(chrome_download_base(None), None);
    }

    #[test]
    fn url_follows_default_port_only() {
        assert_eq!(
            url_for_port("http://localhost:2536", 24100),
            "http://localhost:24100"
        );
        assert_eq!(
            url_for_port("https://bot.example.com", 24100),
            "https://bot.example.com"
        );
    }
}

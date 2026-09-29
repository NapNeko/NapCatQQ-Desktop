//! 每实例一份 Koishi 官方整包（boilerplate Release 附件，依赖都装好了）。
//!
//! 首装：解析 Node → 挑附件、镜像竞速下到桌面端本机 → 远端先重打 tar.gz 再传 → 解到实例目录的暂存目录
//! → 挪进来 → 种 koishi.yml（口、只绑回环、去 maxPort、别开浏览器）→ `yarn add` OneBot 适配器。
//! 更新：换整包但留下 koishi.yml / data / .env 和用户自己加的依赖（并进新 package.json 后 `yarn install`）。
//!
//! 下载都在桌面端本机做：远端常常连不上 GitHub。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use ncd_component::{
    ActionCtx, ActionError, Component, ComponentId, DetectOutcome, DetectedVersion, DownloadHelper,
    LaunchArgs, ProgressKind, Requirement, UnusableInstall, VerifyReport, probe_remote_arch,
};
use ncd_host::{ArchiveKind, Host, HostCommand, HostPath, Locality, Os};
use ncd_network::build_mirror_urls;
use serde_json::{Map, Value};

use super::bundle::zip_to_tar_gz;
use super::manifest::{
    BOILERPLATE_PACKAGE_NAME, KOISHI_CORE_PACKAGE_JSON, KOISHI_NODE_VERSION_RANGE, KOISHI_YML,
    ONEBOT_ADAPTER_PACKAGE, PACKAGE_JSON, PRESERVED_ON_UPDATE, STAGE_DIR,
};
use super::release::{BoilerplateAsset, fetch_latest_asset, pinned_asset, platform_slug};
use super::yarn::{path_env, resolve_node, resolve_yarn_rel, yarn_command};
use super::yml::KoishiInstanceConfig;
use crate::node_tooling::{NodeToolchain, write_node_marker};

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

const YARN_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone)]
pub struct KoishiComponent {
    pub install_dir: HostPath,
    /// server 插件的口 = 实例 `port`
    pub port: u16,
    pub node_bin: Option<HostPath>,
    pub npm_registry: Option<String>,
}

impl KoishiComponent {
    pub fn new(install_dir: HostPath, port: u16) -> Self {
        Self {
            install_dir,
            port,
            node_bin: None,
            npm_registry: None,
        }
    }

    pub fn with_node_bin(mut self, node_bin: Option<HostPath>) -> Self {
        self.node_bin = node_bin;
        self
    }

    pub fn with_npm_registry(mut self, registry: Option<String>) -> Self {
        self.npm_registry = registry.filter(|s| !s.trim().is_empty());
        self
    }

    fn stage_dir(&self) -> HostPath {
        self.install_dir.join(STAGE_DIR)
    }

    async fn pick_asset(&self, host: &dyn Host, ctx: &ActionCtx) -> BoilerplateAsset {
        let arch = match host.locality() {
            Locality::Remote => probe_remote_arch(host).await.unwrap_or(host.arch()),
            Locality::Local => host.arch(),
        };
        let slug = platform_slug(host.os(), arch);
        match fetch_latest_asset(slug).await {
            Ok(Some(asset)) => asset,
            Ok(None) => {
                ctx.warn("Release 里没有可用的整包，改装内置版本").await;
                pinned_asset(slug)
            }
            Err(e) => {
                ctx.warn(format!("{e}；改装内置版本")).await;
                pinned_asset(slug)
            }
        }
    }

    /// 下载 → （远端重打 tar.gz）→ 传到暂存目录 → 解开，返回解开后的根
    async fn stage_bundle(
        &self,
        host: &dyn Host,
        ctx: &ActionCtx,
        step: u32,
        asset: &BoilerplateAsset,
    ) -> Result<HostPath, ActionError> {
        let mirrors = build_mirror_urls(&asset.url, None);
        ctx.info(format!(
            "下载 {}，候选镜像 {} 个",
            asset.name,
            mirrors.len()
        ))
        .await;
        let base =
            std::env::temp_dir().join(format!("ncd-koishi-{}-{}", std::process::id(), unix_ms()));
        let zip_local = base.with_extension("zip");
        let downloaded = DownloadHelper::new()?
            .download_with_mirrors_no_chunk(&mirrors, &zip_local, None, ctx, step)
            .await;
        if let Err(e) = downloaded {
            let _ = tokio::fs::remove_file(&zip_local).await;
            return Err(e);
        }

        let (local, ext, kind) = match host.locality() {
            Locality::Local => (zip_local.clone(), "zip", ArchiveKind::Zip),
            Locality::Remote => {
                ctx.info("远端改用 tar.gz 传（很多机器没装 unzip）").await;
                let tgz = base.with_extension("tar.gz");
                let (src, dst) = (zip_local.clone(), tgz.clone());
                let repacked = tokio::task::spawn_blocking(move || zip_to_tar_gz(&src, &dst))
                    .await
                    .map_err(|e| ActionError::install_step("repack", e.to_string()))?;
                let _ = tokio::fs::remove_file(&zip_local).await;
                repacked.map_err(|e| ActionError::install_step("repack", e))?;
                (tgz, "tar.gz", ArchiveKind::TarGz)
            }
        };

        let slot = self.stage_dir().join("bundle");
        let _ = host.remove_dir_all(&slot).await;
        host.create_dir_all(&slot).await?;
        let archive = self.stage_dir().join(format!("bundle.{ext}"));
        let uploaded = host.upload(&local, &archive).await;
        let _ = tokio::fs::remove_file(&local).await;
        uploaded?;
        host.extract_archive(&archive, &slot, kind).await?;
        let _ = host.remove_file(&archive).await;

        // 官方包没有顶层目录；万一以后套了一层也认
        if host.exists(&slot.join(PACKAGE_JSON)).await? {
            return Ok(slot);
        }
        let dirs: Vec<String> = host
            .list_dir(&slot)
            .await?
            .into_iter()
            .filter(|e| e.is_dir)
            .map(|e| e.name)
            .collect();
        match dirs.as_slice() {
            [top] if host.exists(&slot.join(top).join(PACKAGE_JSON)).await? => Ok(slot.join(top)),
            _ => Err(ActionError::install_step(
                "extract",
                format!("Koishi 整包里没有 package.json，实际顶层 {dirs:?}"),
            )),
        }
    }

    /// 整包顶层条目挪进实例目录；更新时用户数据那几项原样留下
    async fn place(
        &self,
        host: &dyn Host,
        root: &HostPath,
        updating: bool,
    ) -> Result<(), ActionError> {
        for entry in host.list_dir(root).await? {
            if updating && PRESERVED_ON_UPDATE.contains(&entry.name.as_str()) {
                continue;
            }
            let dest = self.install_dir.join(&entry.name);
            remove_existing(host, &dest, entry.is_dir).await?;
            host.rename(&root.join(&entry.name), &dest).await?;
        }
        Ok(())
    }

    /// 首装种 koishi.yml；更新不碰（用户和控制台改过的都在里面）
    async fn seed_config(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.install_dir.join(KOISHI_YML);
        let text = read_text(host, &path).await?.unwrap_or_default();
        let mut cfg = KoishiInstanceConfig::parse(&text)
            .map_err(|e| ActionError::install_step("seed-config", e))?;
        cfg.pin_server(self.port);
        cfg.disable_console_autoopen();
        let out = cfg
            .render()
            .map_err(|e| ActionError::install_step("seed-config", e))?;
        host.write_file(&path, out.as_bytes()).await?;
        Ok(())
    }

    async fn run_yarn(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
        step: u32,
        name: &str,
        tc: &NodeToolchain,
        args: &[&str],
    ) -> Result<(), ActionError> {
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        let yarn = resolve_yarn_rel(host, &self.install_dir).await?;
        ctx.emit(ProgressKind::StepBegin {
            step,
            message: name.to_string(),
        })
        .await;
        let cmd = yarn_command(
            tc,
            &self.install_dir,
            &yarn,
            host.os(),
            host.locality(),
            self.npm_registry.as_deref(),
            args,
        )
        .timeout(YARN_TIMEOUT)
        .cancel_token(ctx.cancel_token());
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
            let tail = out
                .stdout
                .lines()
                .chain(out.stderr.lines())
                .rfind(|l| l.contains("YN0001") || l.contains("Error") || l.contains("error"))
                .unwrap_or_default()
                .to_string();
            return Err(ActionError::install_step(
                name,
                format!(
                    "exit={:?}: {tail} 确认能连上 npm 源（设置里可换镜像）",
                    out.exit_code
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
        Ok(())
    }

    async fn provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        let updating = host.exists(&self.install_dir.join(KOISHI_YML)).await?
            && host.exists(&self.install_dir.join(PACKAGE_JSON)).await?;
        ctx.emit(ProgressKind::Started { total_steps: 5 }).await;

        begin(ctx, 1, "解析 Node.js").await;
        let tc = resolve_node(host, &self.install_dir, self.node_bin.as_ref()).await?;
        ctx.info(format!("Node.js: {}", tc.node_bin.render_for(host.os())))
            .await;
        host.create_dir_all(&self.install_dir).await?;
        write_node_marker(host, &self.install_dir, &tc).await?;
        end(ctx, 1).await;

        begin(ctx, 2, "下载 Koishi 整包").await;
        let asset = self.pick_asset(host, ctx).await;
        let root = self.stage_bundle(host, ctx, 2, &asset).await?;
        end(ctx, 2).await;

        begin(
            ctx,
            3,
            if updating {
                "替换整包（保留 koishi.yml、数据和自己装的插件）"
            } else {
                "放置文件、写入端口"
            },
        )
        .await;
        let old_pkg = if updating {
            read_text(host, &self.install_dir.join(PACKAGE_JSON)).await?
        } else {
            None
        };
        self.place(host, &root, updating).await?;
        let _ = host.remove_dir_all(&self.stage_dir()).await;
        if let Some(old) = old_pkg {
            let new_path = self.install_dir.join(PACKAGE_JSON);
            let new = read_text(host, &new_path).await?.unwrap_or_default();
            let merged = merge_user_dependencies(&old, &new)
                .map_err(|e| ActionError::install_step("merge-package", e))?;
            host.write_file(&new_path, merged.as_bytes()).await?;
        } else {
            self.seed_config(host).await?;
        }
        ctx.info(format!("Koishi 整包 {}，端口 {}", asset.tag, self.port))
            .await;
        end(ctx, 3).await;

        if updating {
            self.run_yarn(host, ctx, 4, "同步依赖（yarn install）", &tc, &["install"])
                .await?;
        } else {
            begin(ctx, 4, "依赖已随整包装好").await;
            end(ctx, 4).await;
        }
        if !self.has_dependency(host, ONEBOT_ADAPTER_PACKAGE).await? {
            self.run_yarn(
                host,
                ctx,
                5,
                "安装 OneBot 适配器",
                &tc,
                &["add", ONEBOT_ADAPTER_PACKAGE],
            )
            .await?;
        } else {
            begin(ctx, 5, "OneBot 适配器已在").await;
            end(ctx, 5).await;
        }
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn has_dependency(&self, host: &dyn Host, name: &str) -> Result<bool, ActionError> {
        let text = read_text(host, &self.install_dir.join(PACKAGE_JSON))
            .await?
            .unwrap_or_default();
        let deps = package_dependencies(&text);
        Ok(deps.contains_key(name)
            && host
                .exists(
                    &self
                        .install_dir
                        .join(format!("node_modules/{name}/package.json")),
                )
                .await?)
    }

    async fn read_core_version(&self, host: &dyn Host) -> Result<Option<String>, ActionError> {
        let Some(text) = read_text(host, &self.install_dir.join(KOISHI_CORE_PACKAGE_JSON)).await?
        else {
            return Ok(None);
        };
        Ok(serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("version").and_then(Value::as_str).map(str::to_string)))
    }

    /// 上游的 `yarn start`：留着 yarn 这一层，控制台里的插件市场才认得出包管理器、找得到 yarn
    pub async fn resolve_launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let tc = resolve_node(host, &self.install_dir, self.node_bin.as_ref()).await?;
        let yarn = resolve_yarn_rel(host, &self.install_dir).await?;
        Ok(self.launch_with(&tc, &yarn, host.os(), host.locality(), args))
    }

    fn launch_with(
        &self,
        tc: &NodeToolchain,
        yarn_rel: &str,
        os: Os,
        locality: Locality,
        args: &LaunchArgs,
    ) -> HostCommand {
        let cmd = yarn_command(
            tc,
            &self.install_dir,
            yarn_rel,
            os,
            locality,
            self.npm_registry.as_deref(),
            &["start"],
        )
        .long_running();
        args.apply_to(cmd)
    }
}

/// 更新时把旧 package.json 里用户加的依赖并进新的：新包自带的版本优先（升级就是为了它），
/// 新包没有的原样留下
pub fn merge_user_dependencies(old: &str, new: &str) -> Result<String, String> {
    let mut new_v: Value =
        serde_json::from_str(new).map_err(|e| format!("新 package.json 解析失败: {e}"))?;
    let old_deps = package_dependencies(old);
    let obj = new_v
        .as_object_mut()
        .ok_or_else(|| "新 package.json 根不是对象".to_string())?;
    let deps = obj
        .entry("dependencies")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(deps) = deps.as_object_mut() else {
        return Err("新 package.json 的 dependencies 不是对象".to_string());
    };
    for (name, spec) in old_deps {
        deps.entry(name).or_insert(spec);
    }
    let mut sorted: Vec<(String, Value)> = std::mem::take(deps).into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    deps.extend(sorted);
    serde_json::to_string_pretty(&new_v)
        .map(|s| format!("{s}\n"))
        .map_err(|e| e.to_string())
}

pub fn package_dependencies(text: &str) -> Map<String, Value> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("dependencies").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

#[async_trait]
impl Component for KoishiComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Koishi
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        vec![Requirement::component_version(
            ComponentId::NodeJs,
            KOISHI_NODE_VERSION_RANGE,
        )]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        let Some(pkg) = read_text(host, &self.install_dir.join(PACKAGE_JSON)).await? else {
            return Ok(DetectOutcome::NotInstalled);
        };
        let is_koishi = serde_json::from_str::<Value>(&pkg).ok().is_some_and(|v| {
            v.get("name").and_then(Value::as_str) == Some(BOILERPLATE_PACKAGE_NAME)
                || v.get("dependencies")
                    .and_then(|d| d.get("koishi"))
                    .is_some()
        });
        if !is_koishi || !host.exists(&self.install_dir.join(KOISHI_YML)).await? {
            return Ok(DetectOutcome::NotInstalled);
        }
        let Some(version) = self.read_core_version(host).await? else {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version: None,
                reason: "koishi.yml 在，但 node_modules 里没有 koishi，重新安装可修复".to_string(),
            }));
        };
        Ok(DetectOutcome::Installed(DetectedVersion {
            version,
            source: self
                .install_dir
                .join(KOISHI_CORE_PACKAGE_JSON)
                .as_posix()
                .to_string(),
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
        begin(ctx, 1, format!("删除 {}", self.install_dir.as_posix())).await;
        if host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        }
        end(ctx, 1).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let mut report = VerifyReport::ok();
        for (name, path) in [
            (KOISHI_YML, self.install_dir.join(KOISHI_YML)),
            ("koishi", self.install_dir.join(KOISHI_CORE_PACKAGE_JSON)),
            (
                "adapter-onebot",
                self.install_dir.join(format!(
                    "node_modules/{ONEBOT_ADAPTER_PACKAGE}/package.json"
                )),
            ),
        ] {
            let ok = host.exists(&path).await?;
            report = report.with_check(name, ok, Some(path.as_posix().to_string()));
        }
        let yarn = resolve_yarn_rel(host, &self.install_dir).await;
        report = report.with_check("yarn", yarn.is_ok(), yarn.ok());
        Ok(report)
    }

    fn launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // 同步版只能猜 node 和 yarn 的位置；运行时请用 resolve_launch_command
        let node = self
            .node_bin
            .clone()
            .unwrap_or_else(|| HostPath::from_posix("node"));
        let tc = NodeToolchain {
            npm_cli: crate::node_tooling::npm_cli_for(&node, host.os()),
            node_bin: node,
        };
        Ok(self.launch_with(
            &tc,
            ".yarn/releases/yarn-4.12.0.cjs",
            host.os(),
            host.locality(),
            args,
        ))
    }
}

/// 给终端 / 编排层拼 PATH 用
pub fn launch_path_env(tc: &NodeToolchain, os: Os, locality: Locality) -> String {
    path_env(tc, os, locality)
}

async fn begin(ctx: &ActionCtx, step: u32, message: impl Into<String>) {
    ctx.emit(ProgressKind::StepBegin {
        step,
        message: message.into(),
    })
    .await;
}

async fn end(ctx: &ActionCtx, step: u32) {
    ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

pub(crate) async fn read_text(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<String>, ActionError> {
    if !host.exists(path).await? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

async fn remove_existing(
    host: &dyn Host,
    path: &HostPath,
    is_dir: bool,
) -> Result<(), ActionError> {
    if !host.exists(path).await? {
        return Ok(());
    }
    if is_dir {
        host.remove_dir_all(path).await?;
    } else {
        host.remove_file(path).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_user_plugins_and_prefers_new_bundle_versions() {
        let old = r#"{"name":"@koishijs/boilerplate","dependencies":{"koishi":"4.18.0","koishi-plugin-adapter-onebot":"^6.9.4","koishi-plugin-foo":"^1.0.0"}}"#;
        let new = r#"{"name":"@koishijs/boilerplate","version":"1.16.1","dependencies":{"koishi":"4.18.11","@koishijs/plugin-help":"2.4.6"}}"#;
        let merged: Value =
            serde_json::from_str(&merge_user_dependencies(old, new).unwrap()).unwrap();
        let deps = merged["dependencies"].as_object().unwrap();
        assert_eq!(deps["koishi"], "4.18.11");
        assert_eq!(deps["koishi-plugin-foo"], "^1.0.0");
        assert_eq!(deps["koishi-plugin-adapter-onebot"], "^6.9.4");
        let keys: Vec<&String> = deps.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "和上游 market 一样按名字排");
        assert_eq!(merged["version"], "1.16.1");
    }

    #[test]
    fn launch_is_bundled_yarn_start() {
        let comp = KoishiComponent::new(HostPath::from_posix("/home/u/ncd/apps/koishi/k1"), 23140);
        let tc = NodeToolchain {
            node_bin: HostPath::from_posix("/opt/node/bin/node"),
            npm_cli: HostPath::from_posix("/opt/node/lib/node_modules/npm/bin/npm-cli.js"),
        };
        let cmd = comp.launch_with(
            &tc,
            ".yarn/releases/yarn-4.12.0.cjs",
            Os::Linux,
            Locality::Remote,
            &LaunchArgs::default(),
        );
        assert_eq!(cmd.program, "/opt/node/bin/node");
        assert_eq!(
            cmd.args,
            vec![
                "/home/u/ncd/apps/koishi/k1/.yarn/releases/yarn-4.12.0.cjs".to_string(),
                "start".to_string()
            ]
        );
        assert_eq!(
            cmd.working_dir.as_ref().map(|d| d.as_posix()),
            Some("/home/u/ncd/apps/koishi/k1")
        );
    }

    #[test]
    fn requirements_pin_node() {
        let comp = KoishiComponent::new(HostPath::from_posix("/x"), 1);
        assert_eq!(
            comp.requirements(Os::Windows, Locality::Local),
            vec![Requirement::component_version(ComponentId::NodeJs, ">=18")]
        );
        assert_eq!(comp.id(), ComponentId::Koishi);
    }
}

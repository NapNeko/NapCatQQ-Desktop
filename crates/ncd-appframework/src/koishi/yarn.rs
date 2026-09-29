//! 整包自带的 yarn 4：`.yarnrc.yml` 的 `yarnPath` 指着 `.yarn/releases/yarn-*.cjs`，
//! 一律 `node <那个 cjs> ...` 调，不要求主机装 yarn / corepack。
//!
//! 环境变量：CI 下 yarn 默认不许改锁文件（`yarn add` 直接失败），显式关掉；
//! 进度条和颜色在日志面板里只会变成乱码。

use ncd_component::ActionError;
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};

use super::manifest::YARNRC;
use crate::node_tooling::{
    NodeToolchain, local_path_env, read_node_marker, resolve_node_toolchain,
};

pub const YARN_RELEASES_DIR: &str = ".yarn/releases";

/// `.yarnrc.yml` 里的 `yarnPath:` 一行（整包的格式固定，不为这一行引 YAML 解析）
pub fn yarn_path_from_rc(rc: &str) -> Option<String> {
    rc.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("yarnPath:")?.trim();
        let rest = rest.trim_matches(|c| c == '"' || c == '\'');
        (!rest.is_empty()).then(|| rest.to_string())
    })
}

/// 实例里 yarn 入口的相对路径：先看 rc，再在 releases 目录里找唯一一个 cjs
pub async fn resolve_yarn_rel(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<String, ActionError> {
    let rc = install_dir.join(YARNRC);
    if host.exists(&rc).await? {
        let text = String::from_utf8_lossy(&host.read_file(&rc).await?).into_owned();
        if let Some(rel) = yarn_path_from_rc(&text)
            && host.exists(&install_dir.join(&rel)).await?
        {
            return Ok(rel);
        }
    }
    let dir = install_dir.join(YARN_RELEASES_DIR);
    if host.exists(&dir).await? {
        let mut found: Vec<String> = host
            .list_dir(&dir)
            .await?
            .into_iter()
            .filter(|e| !e.is_dir && e.name.ends_with(".cjs"))
            .map(|e| e.name)
            .collect();
        found.sort();
        if let Some(name) = found.pop() {
            return Ok(format!("{YARN_RELEASES_DIR}/{name}"));
        }
    }
    Err(ActionError::other(format!(
        "实例里找不到 yarn（{}），整包可能不完整，重新安装可修复",
        dir.as_posix()
    )))
}

/// node 的候选：桌面端管理的 → 装实例时记下的（其余交给 resolve 去看 PATH）
pub async fn preferred_nodes(
    host: &dyn Host,
    install_dir: &HostPath,
    node_bin: Option<&HostPath>,
) -> Vec<HostPath> {
    let mut out = Vec::with_capacity(2);
    if let Some(p) = node_bin {
        out.push(p.clone());
    }
    if let Some(p) = read_node_marker(host, install_dir).await {
        out.push(p);
    }
    out
}

pub async fn resolve_node(
    host: &dyn Host,
    install_dir: &HostPath,
    node_bin: Option<&HostPath>,
) -> Result<NodeToolchain, ActionError> {
    let preferred = preferred_nodes(host, install_dir, node_bin).await;
    resolve_node_toolchain(host, &preferred).await
}

/// 给 yarn / Koishi 进程的 PATH：node 所在目录在前（yarn 跑脚本、装依赖时的 postinstall 都要找 node）
pub fn path_env(tc: &NodeToolchain, os: Os, locality: Locality) -> String {
    let prefix: Vec<String> = tc
        .node_dir()
        .map(|d| d.render_for(os))
        .into_iter()
        .collect();
    match locality {
        Locality::Local => local_path_env(&prefix, os),
        Locality::Remote => {
            let mut parts = prefix;
            parts.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(String::from));
            parts.join(":")
        }
    }
}

/// `node <yarn.cjs> args...`，cwd = 实例目录
pub fn yarn_command(
    tc: &NodeToolchain,
    install_dir: &HostPath,
    yarn_rel: &str,
    os: Os,
    locality: Locality,
    registry: Option<&str>,
    args: &[&str],
) -> HostCommand {
    let mut cmd = HostCommand::new(tc.node_bin.as_posix())
        .arg(install_dir.join(yarn_rel).render_for(os))
        .args(args.iter().copied())
        .working_dir(install_dir.clone())
        .env("PATH", path_env(tc, os, locality))
        .env("YARN_ENABLE_IMMUTABLE_INSTALLS", "false")
        .env("YARN_ENABLE_PROGRESS_BARS", "false")
        .env("YARN_ENABLE_COLORS", "false")
        .env("YARN_ENABLE_TELEMETRY", "0")
        .env("YARN_ENABLE_HYPERLINKS", "false");
    if let Some(r) = registry.map(str::trim).filter(|r| !r.is_empty()) {
        cmd = cmd.env("YARN_NPM_REGISTRY_SERVER", r);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yarn_path_is_read_from_rc() {
        let rc = "enableTips: false\n\nnodeLinker: node-modules\n\nyarnPath: .yarn/releases/yarn-4.12.0.cjs\n";
        assert_eq!(
            yarn_path_from_rc(rc).as_deref(),
            Some(".yarn/releases/yarn-4.12.0.cjs")
        );
        assert_eq!(
            yarn_path_from_rc("yarnPath: \"a b.cjs\"").as_deref(),
            Some("a b.cjs")
        );
        assert_eq!(yarn_path_from_rc("nodeLinker: pnp"), None);
    }

    #[test]
    fn yarn_command_runs_bundled_cjs_with_node_on_path() {
        let tc = NodeToolchain {
            node_bin: HostPath::from_posix("/opt/node/bin/node"),
            npm_cli: HostPath::from_posix("/opt/node/lib/node_modules/npm/bin/npm-cli.js"),
        };
        let dir = HostPath::from_posix("/home/u/ncd/apps/koishi/k1");
        let cmd = yarn_command(
            &tc,
            &dir,
            ".yarn/releases/yarn-4.12.0.cjs",
            Os::Linux,
            Locality::Remote,
            Some("https://registry.npmmirror.com"),
            &["add", "koishi-plugin-adapter-onebot"],
        );
        assert_eq!(cmd.program, "/opt/node/bin/node");
        assert_eq!(
            cmd.args,
            vec![
                "/home/u/ncd/apps/koishi/k1/.yarn/releases/yarn-4.12.0.cjs".to_string(),
                "add".to_string(),
                "koishi-plugin-adapter-onebot".to_string(),
            ]
        );
        let env = |k: &str| cmd.environment.get(k).map(String::as_str);
        assert_eq!(
            env("PATH"),
            Some("/opt/node/bin:/usr/local/bin:/usr/bin:/bin")
        );
        assert_eq!(env("YARN_ENABLE_IMMUTABLE_INSTALLS"), Some("false"));
        assert_eq!(
            env("YARN_NPM_REGISTRY_SERVER"),
            Some("https://registry.npmmirror.com")
        );
    }
}

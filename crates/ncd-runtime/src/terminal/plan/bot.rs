//! 协议 Bot 的终端：本机进运行目录，远端原生进运行目录，Docker 部署进容器或宿主机上的部署目录

use ncd_domain::remote_paths::docker_bot_project_dir;
use ncd_domain::{
    BackendType, BotId, DeploymentType, RuntimeTarget, TerminalOpenRequest, TerminalSnippet,
    derive_remote_linux_paths, desktop_default_install_paths,
};

use super::{
    DesktopTerminalPlanner, RemoteStart, TerminalLaunchPlan, dir_line, linux_snippets,
    local_dir_prefix, posix_parent, remote_host_id, with_host_label,
};
use crate::terminal::TerminalError;

impl DesktopTerminalPlanner {
    pub(super) async fn plan_bot(
        &self,
        request: &TerminalOpenRequest,
        bot_id: &str,
        host_dir: bool,
    ) -> Result<TerminalLaunchPlan, TerminalError> {
        let config = self
            .bots
            .bot_config(&BotId::new(bot_id))
            .await
            .map_err(TerminalError::Plan)?
            .ok_or_else(|| TerminalError::Plan(format!("找不到 Bot {bot_id}")))?;
        let basic = &config.bot;
        let name = if basic.name.trim().is_empty() {
            basic.qq_id.to_string()
        } else {
            basic.name.trim().to_string()
        };
        let backend = match basic.backend_type {
            BackendType::NapCat => "NapCat",
            BackendType::SnowLuma => "SnowLuma",
        };

        match (&basic.runtime_target, basic.deployment_type) {
            (RuntimeTarget::Local, DeploymentType::Native) => {
                let (dir, path_prefix) = match basic.backend_type {
                    BackendType::NapCat => (self.paths.napcat_install_dir(), Vec::new()),
                    BackendType::SnowLuma => {
                        let dir = self.paths.snowluma_install_dir();
                        let node_dir = self.paths.components_dir().join("NodeJs");
                        let prefix = local_dir_prefix(
                            &[&dir, &node_dir, &node_dir.join("bin")],
                            if cfg!(windows) { "node.exe" } else { "node" },
                        );
                        (dir, prefix)
                    }
                };
                if !dir.is_dir() {
                    return Err(TerminalError::Plan(format!(
                        "{backend} 还没装好（{} 不存在）",
                        dir.display()
                    )));
                }
                let banner = vec![dir_line(
                    &format!("{backend} 运行目录"),
                    &dir.to_string_lossy(),
                )];
                let host = self.resolve(&RuntimeTarget::Local).await?;
                self.local_plan(
                    host,
                    request,
                    Some(with_host_label(&name, "本机")),
                    Some(dir),
                    path_prefix,
                    Vec::new(),
                    banner,
                    Vec::new(),
                )
            }
            (RuntimeTarget::Local, DeploymentType::Docker) => {
                let host = self.resolve(&RuntimeTarget::Local).await?;
                if host.os() != ncd_host::Os::Linux {
                    return Err(TerminalError::Plan("Docker Bot 仅支持 Linux 主机".into()));
                }
                let names =
                    ncd_deploy::bot_docker_container_candidates(basic.backend_type, basic.qq_id)
                        .to_vec();
                let engine = ncd_deploy::docker::engine::engine_for_bot(host.as_ref(), &config);
                if host_dir {
                    let dir = names
                        .iter()
                        .map(|name| self.paths.root().join("docker").join(name))
                        .find(|dir| dir.is_dir())
                        .ok_or_else(|| {
                            TerminalError::Plan(
                                "尚未创建此 Bot 的 Docker 部署目录，请先启动 Bot".into(),
                            )
                        })?;
                    let snippets = bot_compose_snippets(host.as_ref(), &config, false).await;
                    self.local_plan(
                        host,
                        request,
                        Some(format!("{name} 部署目录")),
                        Some(dir),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        snippets,
                    )
                } else {
                    Ok(TerminalLaunchPlan {
                        host,
                        host_id: ncd_domain::LOCAL_HOST_ID.into(),
                        host_label: "本机".into(),
                        host_os: ncd_domain::TerminalHostOs::Linux,
                        title: format!("{name} 容器"),
                        program: ncd_host::PtyProgram::Script(super::docker_exec_script(
                            &names, engine,
                        )),
                        cwd: None,
                        env: std::collections::BTreeMap::new(),
                        cwd_display: None,
                        shell: None,
                        features: ncd_domain::TerminalFeatures::default(),
                        snippets: Vec::new(),
                        banner: vec![format!("进 {name} 的容器；在容器里 exit 这个终端就结束了")],
                    })
                }
            }
            (RuntimeTarget::Server(server_id), DeploymentType::Native) => {
                let host = self.resolve(&basic.runtime_target).await?;
                if host.os() == ncd_host::Os::Windows {
                    let info = host
                        .remote_windows_info()
                        .await
                        .map_err(|e| TerminalError::Plan(e.to_string()))?;
                    let selected = self
                        .profile(server_id)
                        .await
                        .and_then(|p| p.inventory)
                        .map(|inv| inv.selected)
                        .unwrap_or_default();
                    let root = host.target_managed_root().ok_or_else(|| {
                        TerminalError::Plan("无法解析 Windows 用户受管目录".into())
                    })?;
                    let dir = match basic.backend_type {
                        BackendType::NapCat => selected.napcat_root.unwrap_or_else(|| {
                            root.join("components/NapCatQQ")
                                .render_for(ncd_host::Os::Windows)
                        }),
                        BackendType::SnowLuma => selected.snowluma_dir.unwrap_or_else(|| {
                            root.join("components/SnowLuma")
                                .render_for(ncd_host::Os::Windows)
                        }),
                    };
                    let prefix = selected
                        .node_bin
                        .and_then(|p| ncd_host::HostPath::from_windows(&p).parent())
                        .map(|p| p.render_for(ncd_host::Os::Windows))
                        .into_iter()
                        .collect();
                    let _ = info;
                    return self
                        .remote_plan(
                            host,
                            server_id,
                            Some(name),
                            RemoteStart::Dir(vec![dir]),
                            prefix,
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                        )
                        .await;
                }
                let info = self
                    .remote_shell(&remote_host_id(server_id), host.as_ref())
                    .await?;
                // 路径以远端页探测过的为准（用户可能改过安装位置），没探测过按桌面端默认安装布局猜
                let selected = self
                    .profile(server_id)
                    .await
                    .and_then(|p| p.inventory)
                    .map(|inv| inv.selected)
                    .unwrap_or_default();
                let found = derive_remote_linux_paths(&selected);
                let defaults = desktop_default_install_paths(&info.home).unwrap_or_default();
                let (dir, prefix) = match basic.backend_type {
                    BackendType::NapCat => (found.napcat_root.or(defaults.napcat_root), Vec::new()),
                    BackendType::SnowLuma => (
                        found.snowluma_dir.or(defaults.snowluma_dir),
                        found
                            .node_bin
                            .as_deref()
                            .and_then(posix_parent)
                            .into_iter()
                            .collect(),
                    ),
                };
                let dir = dir.unwrap_or_else(|| info.home.clone());
                let banner = vec![dir_line(&format!("{backend} 运行目录"), &dir)];
                self.remote_plan(
                    host,
                    server_id,
                    Some(name),
                    RemoteStart::Dir(vec![dir]),
                    prefix,
                    Vec::new(),
                    banner,
                    linux_snippets(),
                )
                .await
            }
            (RuntimeTarget::Server(server_id), DeploymentType::Docker) => {
                let names =
                    ncd_deploy::bot_docker_container_candidates(basic.backend_type, basic.qq_id)
                        .to_vec();
                let host = self.resolve(&basic.runtime_target).await?;
                let engine = ncd_deploy::docker::engine::engine_for_bot(host.as_ref(), &config);
                if host_dir {
                    let info = self
                        .remote_shell(&remote_host_id(server_id), host.as_ref())
                        .await?;
                    let dirs: Vec<String> = names
                        .iter()
                        .map(|n| docker_bot_project_dir(&info.home, n))
                        .collect();
                    let snippets = bot_compose_snippets(host.as_ref(), &config, true).await;
                    self.remote_plan(
                        host,
                        server_id,
                        Some(format!("{name} 部署目录")),
                        RemoteStart::Dir(dirs),
                        Vec::new(),
                        Vec::new(),
                        vec![format!(
                            "{name} 的部署目录，docker-compose.yml 和数据卷在这里"
                        )],
                        snippets,
                    )
                    .await
                } else {
                    self.remote_plan(
                        host,
                        server_id,
                        Some(format!("{name} 容器")),
                        RemoteStart::Container(names, engine),
                        Vec::new(),
                        Vec::new(),
                        vec![format!("进 {name} 的容器；在容器里 exit 这个终端就结束了")],
                        Vec::new(),
                    )
                    .await
                }
            }
        }
    }
}

async fn bot_compose_snippets(
    host: &dyn ncd_host::Host,
    config: &ncd_domain::BotConfig,
    follow: bool,
) -> Vec<TerminalSnippet> {
    let cli = ncd_deploy::DockerCli::for_bot(host, config);
    let Ok(command) = cli.compose_command().await else {
        return vec![TerminalSnippet::new(
            "引擎诊断",
            format!("{} info", cli.engine().program()),
        )];
    };
    let compose = std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .map(ncd_host::shell_single_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let mut snippets = vec![
        TerminalSnippet::new("容器状态", format!("{compose} ps")),
        TerminalSnippet::new("最近日志", format!("{compose} logs --tail 200")),
    ];
    if follow {
        snippets.push(TerminalSnippet::new(
            "跟着看日志",
            format!("{compose} logs -f --tail 50"),
        ));
    }
    snippets
}

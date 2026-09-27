//! 协议 Bot 的终端：本机进运行目录，远端原生进运行目录，Docker 部署进容器或宿主机上的部署目录

use ncd_domain::{BackendType, BotId, DeploymentType, RuntimeTarget, TerminalOpenRequest, TerminalSnippet};

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
                        let prefix = local_dir_prefix(&[&dir, &node_dir], "node.exe");
                        (dir, prefix)
                    }
                };
                if !dir.is_dir() {
                    return Err(TerminalError::Plan(format!(
                        "{backend} 还没装好（{} 不存在）",
                        dir.display()
                    )));
                }
                let banner = vec![dir_line(&format!("{backend} 运行目录"), &dir.to_string_lossy())];
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
                Err(TerminalError::Plan("本机不跑 Docker 部署的 Bot".into()))
            }
            (RuntimeTarget::Server(server_id), DeploymentType::Native) => {
                let host = self.resolve(&basic.runtime_target).await?;
                let info = self
                    .remote_shell(&remote_host_id(server_id), host.as_ref())
                    .await?;
                // 路径以远端页探测过的为准（用户可能改过安装位置），没探测过按默认布局猜
                let selected = self
                    .profile(server_id)
                    .await
                    .and_then(|p| p.inventory)
                    .map(|inv| inv.selected)
                    .unwrap_or_default();
                let (dir, prefix) = match basic.backend_type {
                    BackendType::NapCat => (
                        selected
                            .napcat_root
                            .unwrap_or_else(|| format!("{}/Napcat", info.home)),
                        Vec::new(),
                    ),
                    BackendType::SnowLuma => (
                        selected.snowluma_dir.unwrap_or_else(|| {
                            format!("{}/snowluma-remote/workspace/snowluma", info.home)
                        }),
                        selected
                            .node_bin
                            .as_deref()
                            .and_then(posix_parent)
                            .into_iter()
                            .collect(),
                    ),
                };
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
                let primary = ncd_deploy::bot_docker_container_name(basic.backend_type, basic.qq_id);
                // SnowLuma 的容器以前也叫 ncbot-，两个名字都试
                let other = match basic.backend_type {
                    BackendType::NapCat => format!("slbot-{}", basic.qq_id),
                    BackendType::SnowLuma => format!("ncbot-{}", basic.qq_id),
                };
                let names = vec![primary, other];
                let host = self.resolve(&basic.runtime_target).await?;
                if host_dir {
                    let info = self
                        .remote_shell(&remote_host_id(server_id), host.as_ref())
                        .await?;
                    let dirs: Vec<String> = names
                        .iter()
                        .map(|n| format!("{}/.napcat-bots/{n}", info.home))
                        .collect();
                    let snippets = vec![
                        TerminalSnippet::new("容器状态", "docker compose ps"),
                        TerminalSnippet::new("最近日志", "docker compose logs --tail 200"),
                        TerminalSnippet::new("跟着看日志", "docker compose logs -f --tail 50"),
                    ];
                    self.remote_plan(
                        host,
                        server_id,
                        Some(format!("{name} 部署目录")),
                        RemoteStart::Dir(dirs),
                        Vec::new(),
                        Vec::new(),
                        vec![format!("{name} 的部署目录，docker-compose.yml 和数据卷在这里")],
                        snippets,
                    )
                    .await
                } else {
                    self.remote_plan(
                        host,
                        server_id,
                        Some(format!("{name} 容器")),
                        RemoteStart::Container(names),
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

//! 终端开到哪：目标 → 哪台主机、跑什么、进哪个目录、带什么环境

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use ncd_domain::{
    AppInstanceId, LocalShellKind, LocalShellOption, RuntimeTarget, TerminalFeatures,
    TerminalHostOs, TerminalOpenRequest, TerminalSnippet, TerminalTarget, server_id_of_host,
};
use ncd_host::{Host, HostCommand, HostPath, PathStyle, PtyProgram};
use tokio::sync::Mutex;

use crate::app_framework::{AppManager, BotConfigPort};
use crate::data_paths::DataPaths;
use crate::host_resolver::HostResolver;
use crate::server_manager::{ServerManager, ServerProfile};

use super::TerminalError;
use super::integration::{bash_env_section, bash_rc, git_bash_rc, remote_bash_exec_line, sh_quote};
use super::shells::{detect_local_shells, fresh_local_path, launch_for, pick_shell};

mod bot;

/// 一次开终端的完整方案
pub struct TerminalLaunchPlan {
    pub host: Arc<dyn Host>,
    pub host_id: String,
    pub host_label: String,
    pub host_os: TerminalHostOs,
    pub title: String,
    pub program: PtyProgram,
    /// 本机是进程的起始目录；远端 Program 会拼成 cd
    pub cwd: Option<HostPath>,
    /// 本机是进程环境；远端 Program 拼成行内变量（Script 自己带）
    pub env: BTreeMap<String, String>,
    /// 给前端看的起始目录（文件栏一开始列哪）
    pub cwd_display: Option<String>,
    pub shell: Option<LocalShellKind>,
    pub features: TerminalFeatures,
    pub snippets: Vec<TerminalSnippet>,
    /// 开头打的几行说明（灰字）
    pub banner: Vec<String>,
}

/// 会话管理层需要的外部能力：规划、sudo 密码、本机 shell 列表
#[async_trait]
pub trait TerminalPlanner: Send + Sync {
    async fn plan(&self, request: &TerminalOpenRequest) -> Result<TerminalLaunchPlan, TerminalError>;
    /// 这台主机存的提权密码；只在用户点了「填入密码」时取，直接写进终端
    fn sudo_password(&self, host_id: &str) -> Option<String>;
    fn local_shells(&self) -> Vec<LocalShellOption>;
}

/// 远端登录 shell 的情况，按主机记一次
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteShellInfo {
    /// 登录 shell 能读 POSIX 语法（bash / zsh / dash 等）；fish、csh 这类不能
    posix: bool,
    login_is_bash: bool,
    has_bash: bool,
    home: String,
}

const REMOTE_SHELL_PROBE: &str = r#"printf '%s\n%s\n' "${SHELL:-}" "$HOME"; command -v bash || true"#;

fn parse_remote_shell_probe(stdout: &str) -> RemoteShellInfo {
    let mut lines = stdout.lines().map(str::trim);
    let shell = lines.next().unwrap_or_default();
    let home = lines.next().unwrap_or_default();
    let bash = lines.next().unwrap_or_default();
    let name = shell.rsplit('/').next().unwrap_or_default();
    let posix = matches!(
        name,
        "" | "bash" | "sh" | "dash" | "zsh" | "ksh" | "mksh" | "ash" | "busybox"
    );
    RemoteShellInfo {
        posix,
        login_is_bash: name == "bash",
        has_bash: !bash.is_empty(),
        home: if home.is_empty() { "~".to_string() } else { home.to_string() },
    }
}

fn linux_snippets() -> Vec<TerminalSnippet> {
    vec![
        TerminalSnippet::new("磁盘", "df -h"),
        TerminalSnippet::new("内存", "free -h"),
        TerminalSnippet::new("进程", "top"),
        TerminalSnippet::new("监听端口", "ss -lntp"),
    ]
}

/// 进容器：先看要不要 sudo，再按名字找在跑的那个（SnowLuma 有旧版留下的 ncbot 名字）
fn docker_exec_script(names: &[String]) -> String {
    let list: Vec<String> = names.iter().map(|n| sh_quote(n)).collect();
    let shown = names.join(" / ");
    format!(
        r#"if docker info >/dev/null 2>&1; then D=docker; else D="sudo docker"; fi
C=
for n in {list}; do
  if $D inspect --format '{{{{.State.Running}}}}' "$n" 2>/dev/null | grep -q true; then C=$n; break; fi
done
if [ -z "$C" ]; then echo "没找到在运行的容器（{shown}），先在桌面端把这个 Bot 启动起来"; exit 1; fi
exec $D exec -it "$C" sh -c 'if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi'
"#,
        list = list.join(" "),
    )
}

/// 目录说明行
fn dir_line(label: &str, dir: &str) -> String {
    format!("{label} · {dir}")
}

/// 标签名后面接主机名。实例名常常已经带了主机（新建时默认给的就是「麦麦 · 本机」这种），
/// 带了就不再接一遍
pub(super) fn with_host_label(name: &str, host_label: &str) -> String {
    if name.contains(host_label) {
        name.to_string()
    } else {
        format!("{name} · {host_label}")
    }
}

/// 远端的起始位置
enum RemoteStart {
    Home,
    /// 进第一个存在的目录，带上环境
    Dir(Vec<String>),
    /// 进容器（带 sudo 判断和按名字找）
    Container(Vec<String>),
}

pub struct DesktopTerminalPlanner {
    hosts: Arc<dyn HostResolver>,
    servers: Arc<ServerManager>,
    bots: Arc<dyn BotConfigPort>,
    apps: Arc<AppManager>,
    paths: DataPaths,
    remote_shells: Mutex<HashMap<String, RemoteShellInfo>>,
}

impl DesktopTerminalPlanner {
    pub fn new(
        hosts: Arc<dyn HostResolver>,
        servers: Arc<ServerManager>,
        bots: Arc<dyn BotConfigPort>,
        apps: Arc<AppManager>,
        data_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            hosts,
            servers,
            bots,
            apps,
            paths: DataPaths::new(data_root),
            remote_shells: Mutex::new(HashMap::new()),
        }
    }

    async fn resolve(&self, target: &RuntimeTarget) -> Result<Arc<dyn Host>, TerminalError> {
        self.hosts
            .resolve(target)
            .await
            .map_err(|e| TerminalError::Plan(format!("连不上主机：{e}")))
    }

    async fn profile(&self, server_id: &str) -> Option<ServerProfile> {
        self.servers
            .list_servers()
            .await
            .into_iter()
            .find(|p| p.id == server_id)
    }

    fn server_label(profile: Option<&ServerProfile>, server_id: &str) -> String {
        profile
            .map(|p| if p.name.trim().is_empty() { p.host.clone() } else { p.name.clone() })
            .unwrap_or_else(|| server_id.to_string())
    }

    async fn remote_shell(
        &self,
        host_id: &str,
        host: &dyn Host,
    ) -> Result<RemoteShellInfo, TerminalError> {
        if let Some(info) = self.remote_shells.lock().await.get(host_id) {
            return Ok(info.clone());
        }
        let out = host
            .run_to_string(HostCommand::new("sh").arg("-c").arg(REMOTE_SHELL_PROBE))
            .await
            .map_err(|e| TerminalError::Plan(format!("探测远端 shell 失败：{e}")))?;
        let info = parse_remote_shell_probe(&out.stdout);
        self.remote_shells
            .lock()
            .await
            .insert(host_id.to_string(), info.clone());
        Ok(info)
    }

    /// Git Bash 的 rc 放数据根的缓存目录；写不了就退回普通登录 shell
    fn git_bash_rc_path(&self) -> Option<String> {
        let dir = self.paths.cache_dir().join("terminal");
        let file = dir.join("ncd-bashrc.sh");
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&file, git_bash_rc()).ok()?;
        Some(HostPath::from_windows(&file.to_string_lossy()).as_posix().to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn local_plan(
        &self,
        host: Arc<dyn Host>,
        request: &TerminalOpenRequest,
        title: Option<String>,
        cwd: Option<PathBuf>,
        path_prefix: Vec<String>,
        env: Vec<(String, String)>,
        banner: Vec<String>,
        snippets: Vec<TerminalSnippet>,
    ) -> Result<TerminalLaunchPlan, TerminalError> {
        let shells = detect_local_shells();
        let shell = pick_shell(&shells, request.shell)
            .ok_or_else(|| TerminalError::Plan("这台电脑上没找到能用的 shell".into()))?;
        let rc = (shell.kind == LocalShellKind::GitBash)
            .then(|| self.git_bash_rc_path())
            .flatten();
        let launch = launch_for(&shell, rc.as_deref());

        let mut env_map = BTreeMap::new();
        let base_path = fresh_local_path();
        let path = if path_prefix.is_empty() {
            base_path
        } else {
            format!("{};{base_path}", path_prefix.join(";"))
        };
        env_map.insert("PATH".to_string(), path);
        env_map.extend(env);
        env_map.extend(launch.env.clone());

        let cwd = cwd.or_else(dirs::home_dir);
        let cwd_display = cwd.as_ref().map(|p| p.to_string_lossy().into_owned());
        Ok(TerminalLaunchPlan {
            host,
            host_id: ncd_domain::LOCAL_HOST_ID.to_string(),
            host_label: "本机".to_string(),
            host_os: TerminalHostOs::Windows,
            title: title.unwrap_or_else(|| format!("本机 · {}", shell.label)),
            program: PtyProgram::Program {
                program: launch.program,
                args: launch.args,
            },
            cwd: cwd.map(|p| HostPath::from_windows(&p.to_string_lossy())),
            env: env_map,
            cwd_display,
            shell: Some(shell.kind),
            features: TerminalFeatures {
                files: true,
                stats: false,
                sudo_fill: false,
                shell_integration: launch.integration,
                external: true,
            },
            snippets,
            banner,
        })
    }

    /// `title_prefix`：标签名前半截，后面自动接主机名；None 时标签名就是主机名
    #[allow(clippy::too_many_arguments)]
    async fn remote_plan(
        &self,
        host: Arc<dyn Host>,
        server_id: &str,
        title_prefix: Option<String>,
        start: RemoteStart,
        path_prefix: Vec<String>,
        env: Vec<(String, String)>,
        mut banner: Vec<String>,
        snippets: Vec<TerminalSnippet>,
    ) -> Result<TerminalLaunchPlan, TerminalError> {
        let host_id = remote_host_id(server_id);
        let info = self.remote_shell(&host_id, host.as_ref()).await?;
        let profile = self.profile(server_id).await;
        let host_label = Self::server_label(profile.as_ref(), server_id);
        let title = match title_prefix {
            Some(prefix) => with_host_label(&prefix, &host_label),
            None => host_label.clone(),
        };
        let sudo_fill = self.servers.sudo_password(server_id).is_some();

        let mut files = true;
        let (program, integration, cwd_display) = match &start {
            RemoteStart::Home if info.login_is_bash => {
                let rc = bash_rc(&bash_env_section(&path_prefix, &env, &[]));
                (PtyProgram::Script(remote_bash_exec_line(&rc, None)), true, info.home.clone())
            }
            RemoteStart::Home => (PtyProgram::LoginShell, false, info.home.clone()),
            RemoteStart::Dir(dirs) if !info.posix => {
                banner.push(format!(
                    "登录 shell 不是 bash 这一类，没法自动进目录、接环境；目录在 {}",
                    dirs.first().map(String::as_str).unwrap_or("~")
                ));
                (PtyProgram::LoginShell, false, info.home.clone())
            }
            RemoteStart::Dir(dirs) => {
                let first = dirs.first().cloned().unwrap_or_else(|| info.home.clone());
                if info.has_bash {
                    let rc = bash_rc(&bash_env_section(&path_prefix, &env, dirs));
                    (
                        PtyProgram::Script(remote_bash_exec_line(&rc, Some(&first))),
                        true,
                        first,
                    )
                } else {
                    let section = bash_env_section(&path_prefix, &env, dirs);
                    let script = format!("{section}exec \"${{SHELL:-/bin/sh}}\" -l\n");
                    (PtyProgram::Script(script), false, first)
                }
            }
            RemoteStart::Container(names) => {
                files = false;
                if info.posix {
                    (PtyProgram::Script(docker_exec_script(names)), false, info.home.clone())
                } else {
                    banner.push(format!(
                        "登录 shell 不是 bash 这一类，没法自动进容器；手动 docker exec -it {} sh",
                        names.first().map(String::as_str).unwrap_or_default()
                    ));
                    (PtyProgram::LoginShell, false, info.home.clone())
                }
            }
        };
        Ok(TerminalLaunchPlan {
            host,
            host_id,
            host_label,
            host_os: TerminalHostOs::Linux,
            title,
            program,
            cwd: None,
            env: BTreeMap::new(),
            cwd_display: Some(cwd_display),
            shell: None,
            features: TerminalFeatures {
                files,
                stats: true,
                sudo_fill,
                shell_integration: integration,
                external: false,
            },
            snippets,
            banner,
        })
    }
}

fn remote_host_id(server_id: &str) -> String {
    format!("{}{server_id}", ncd_domain::REMOTE_HOST_ID_PREFIX)
}

/// POSIX 路径的上一级；`node` 这种裸名字没有
fn posix_parent(path: &str) -> Option<String> {
    let (dir, _) = path.rsplit_once('/')?;
    (!dir.is_empty()).then(|| dir.to_string())
}

fn local_dir_prefix(dirs: &[&Path], exe: &str) -> Vec<String> {
    dirs.iter()
        .filter(|d| d.join(exe).is_file())
        .map(|d| d.to_string_lossy().into_owned())
        .collect()
}

impl DesktopTerminalPlanner {
    async fn plan_local_home(
        &self,
        request: &TerminalOpenRequest,
    ) -> Result<TerminalLaunchPlan, TerminalError> {
        let host = self.resolve(&RuntimeTarget::Local).await?;
        self.local_plan(host, request, None, None, Vec::new(), Vec::new(), Vec::new(), Vec::new())
    }

    async fn plan_server(&self, server_id: &str) -> Result<TerminalLaunchPlan, TerminalError> {
        let host = self
            .resolve(&RuntimeTarget::Server(server_id.to_string()))
            .await?;
        self.remote_plan(
            host,
            server_id,
            None,
            RemoteStart::Home,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            linux_snippets(),
        )
        .await
    }

    async fn plan_app(
        &self,
        request: &TerminalOpenRequest,
        instance_id: &str,
    ) -> Result<TerminalLaunchPlan, TerminalError> {
        let ctx = self
            .apps
            .terminal_context(&AppInstanceId::new(instance_id))
            .await
            .map_err(|e| TerminalError::Plan(e.to_string()))?;
        let instance = &ctx.instance;
        let dir = HostPath::from_posix(&instance.install_dir);
        let profile = &ctx.profile;
        match server_id_of_host(&instance.host_id) {
            None => {
                let local_dir = dir.render(PathStyle::Windows);
                let mut banner = vec![dir_line(&ctx.framework_name, &local_dir)];
                banner.extend(profile.hint.clone());
                let prefix = profile
                    .path_prefix
                    .iter()
                    .map(|p| p.render(PathStyle::Windows))
                    .collect();
                self.local_plan(
                    Arc::clone(&ctx.host),
                    request,
                    Some(with_host_label(&instance.display_name, "本机")),
                    Some(PathBuf::from(local_dir)),
                    prefix,
                    profile.env.clone(),
                    banner,
                    profile.snippets.clone(),
                )
            }
            Some(server_id) => {
                let mut banner = vec![dir_line(&ctx.framework_name, dir.as_posix())];
                banner.extend(profile.hint.clone());
                let prefix = profile
                    .path_prefix
                    .iter()
                    .map(|p| p.as_posix().to_string())
                    .collect();
                self.remote_plan(
                    Arc::clone(&ctx.host),
                    server_id,
                    Some(instance.display_name.clone()),
                    RemoteStart::Dir(vec![dir.as_posix().to_string()]),
                    prefix,
                    profile.env.clone(),
                    banner,
                    profile.snippets.clone(),
                )
                .await
            }
        }
    }
}

#[async_trait]
impl TerminalPlanner for DesktopTerminalPlanner {
    async fn plan(&self, request: &TerminalOpenRequest) -> Result<TerminalLaunchPlan, TerminalError> {
        match &request.target {
            TerminalTarget::Local => self.plan_local_home(request).await,
            TerminalTarget::Server { server_id } => self.plan_server(server_id).await,
            TerminalTarget::Bot { bot_id, host_dir } => {
                self.plan_bot(request, bot_id, *host_dir).await
            }
            TerminalTarget::AppInstance { instance_id } => self.plan_app(request, instance_id).await,
        }
    }

    fn sudo_password(&self, host_id: &str) -> Option<String> {
        server_id_of_host(host_id).and_then(|id| self.servers.sudo_password(id))
    }

    fn local_shells(&self) -> Vec<LocalShellOption> {
        detect_local_shells()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_recognises_shell_kinds() {
        let bash = parse_remote_shell_probe("/bin/bash\n/home/u\n/usr/bin/bash\n");
        assert_eq!(
            bash,
            RemoteShellInfo {
                posix: true,
                login_is_bash: true,
                has_bash: true,
                home: "/home/u".into()
            }
        );
        let fish = parse_remote_shell_probe("/usr/bin/fish\n/root\n/bin/bash\n");
        assert!(!fish.posix);
        assert!(fish.has_bash);
        let alpine = parse_remote_shell_probe("/bin/ash\n/home/a\n");
        assert!(alpine.posix);
        assert!(!alpine.has_bash);
        assert!(!alpine.login_is_bash);
        let empty = parse_remote_shell_probe("");
        assert_eq!(empty.home, "~");
        assert!(empty.posix);
    }

    #[test]
    fn docker_script_tries_each_name_and_escapes_templates() {
        let script = docker_exec_script(&["ncbot-1".into(), "slbot-1".into()]);
        assert!(script.contains("for n in 'ncbot-1' 'slbot-1'; do"));
        assert!(script.contains("--format '{{.State.Running}}'"));
        assert!(script.contains("（ncbot-1 / slbot-1）"));
        assert!(script.contains("exec $D exec -it \"$C\" sh -c"));
    }

    #[test]
    fn title_skips_host_already_in_name() {
        assert_eq!(with_host_label("麦麦 · 本机", "本机"), "麦麦 · 本机");
        assert_eq!(with_host_label("Karin · production", "production"), "Karin · production");
        assert_eq!(with_host_label("AstrBot a1b2", "vps1"), "AstrBot a1b2 · vps1");
        assert_eq!(with_host_label("10001", "本机"), "10001 · 本机");
    }

    #[test]
    fn posix_parent_of_node_bin() {
        assert_eq!(
            posix_parent("/home/u/snowluma-remote/workspace/node/bin/node"),
            Some("/home/u/snowluma-remote/workspace/node/bin".into())
        );
        assert_eq!(posix_parent("node"), None);
        assert_eq!(posix_parent("/node"), None);
    }
}

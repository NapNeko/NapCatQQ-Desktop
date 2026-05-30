//! DockerCli:docker 命令封装,跑在任意 Host 上(本地 Windows / 远端 Linux)。
//!
//! 设计:只持有 &dyn Host,每个方法拼一条 docker 命令交给 host.run_to_string。
//! 命令参数全部走 HostCommand::arg 分开传,由 shell 层做转义,杜绝把用户输入
//! (容器名 / 端口)拼进命令字符串导致注入。
//!
//! 解析策略:用 `--format '{{json .}}'` 让 docker 自己吐 JSON,逐行 serde 解析,
//! 不靠脆弱的列宽切分。

use ncd_domain::{ContainerInfo, ContainerState, DockerStatus};
use ncd_host::{Host, HostCommand, HostError};

/// DockerCli 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum DockerCliError {
    /// host 层调用失败(SSH 中断 / 进程起不来)。
    #[error("host error: {0}")]
    Host(#[from] HostError),

    /// docker 命令跑了但退出码非 0。stderr 给上层拼错误文案。
    #[error("docker command failed: {command}: exit={exit_code:?}: {stderr}")]
    CommandFailed {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    /// 解析 docker 输出失败(JSON 格式不对等)。
    #[error("failed to parse docker output: {0}")]
    ParseFailed(String),
}

/// docker CLI 封装。轻量,无状态,每次操作临时构造或复用都可以。
pub struct DockerCli<'h> {
    host: &'h dyn Host,
}

impl<'h> DockerCli<'h> {
    pub fn new(host: &'h dyn Host) -> Self {
        Self { host }
    }

    /// 探测 docker 是否可用。任何一步失败都退化成"未装/未就绪",不报错——
    /// 探测本身不该把"没装 docker"当异常。
    pub async fn probe(&self) -> DockerStatus {
        // docker 客户端版本。拿不到直接判定未装。
        let version = match self.docker_client_version().await {
            Some(v) => v,
            None => return DockerStatus::absent(),
        };

        // daemon 是否在跑:docker info 成功即说明 daemon 可达。
        let daemon_running = self.docker_info_ok().await;

        // compose v2 插件。
        let compose_available = self.docker_compose_ok().await;

        DockerStatus {
            installed: true,
            version,
            compose_available,
            daemon_running,
        }
    }

    /// `docker version --format '{{.Client.Version}}'`,失败返回 None。
    async fn docker_client_version(&self) -> Option<String> {
        let cmd = HostCommand::new("docker")
            .arg("version")
            .arg("--format")
            .arg("{{.Client.Version}}");
        let out = self.host.run_to_string(cmd).await.ok()?;
        if !out.success() {
            return None;
        }
        let v = out.stdout.trim();
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    }

    /// `docker info --format '{{.ServerVersion}}'`,daemon 不可达时退出码非 0。
    async fn docker_info_ok(&self) -> bool {
        let cmd = HostCommand::new("docker")
            .arg("info")
            .arg("--format")
            .arg("{{.ServerVersion}}");
        matches!(self.host.run_to_string(cmd).await, Ok(out) if out.success())
    }

    /// `docker compose version`,compose v2 插件存在时退出码 0。
    async fn docker_compose_ok(&self) -> bool {
        let cmd = HostCommand::new("docker").arg("compose").arg("version");
        matches!(self.host.run_to_string(cmd).await, Ok(out) if out.success())
    }
}

impl<'h> DockerCli<'h> {
    /// 列所有容器(含已停止)。`docker ps -a --format '{{json .}}'` 逐行 JSON。
    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>, DockerCliError> {
        let cmd = HostCommand::new("docker")
            .arg("ps")
            .arg("-a")
            .arg("--format")
            .arg("{{json .}}");
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: "docker ps -a".to_string(),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        parse_ps_json(&out.stdout)
    }

    /// 对单个容器执行 start / stop / restart。命令名固定,容器名走 arg 转义。
    pub async fn lifecycle(
        &self,
        action: &str,
        container: &str,
    ) -> Result<(), DockerCliError> {
        let cmd = HostCommand::new("docker").arg(action).arg(container);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker {action} {container}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// 删除容器。默认带 -f 强制删(运行中也删),避免用户先 stop 再 remove 两步。
    pub async fn remove(&self, container: &str) -> Result<(), DockerCliError> {
        let cmd = HostCommand::new("docker")
            .arg("rm")
            .arg("-f")
            .arg(container);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker rm -f {container}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// 取容器最近 tail 行日志。stdout + stderr 合并返回(docker logs 两路都吐)。
    pub async fn logs(&self, container: &str, tail: u32) -> Result<String, DockerCliError> {
        let cmd = HostCommand::new("docker")
            .arg("logs")
            .arg("--tail")
            .arg(tail.to_string())
            .arg(container);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker logs --tail {tail} {container}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        // docker logs 把容器 stdout 走 stdout、stderr 走 stderr;合并保留时序近似。
        let mut combined = out.stdout;
        if !out.stderr.trim().is_empty() {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&out.stderr);
        }
        Ok(combined)
    }
}

impl<'h> DockerCli<'h> {
    /// `docker compose up -d`,在 project_dir 下跑(compose 会读那里的
    /// docker-compose.yml)。pull 由 compose 自己按需做;这里加 --pull missing
    /// 让首次部署自动拉镜像。
    pub async fn compose_up(&self, project_dir: &str) -> Result<(), DockerCliError> {
        let cmd = HostCommand::new("docker")
            .arg("compose")
            .arg("up")
            .arg("-d")
            .arg("--pull")
            .arg("missing")
            .working_dir(ncd_host::HostPath::from_posix(project_dir))
            .timeout(std::time::Duration::from_secs(900));
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker compose up -d (in {project_dir})"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// `docker compose down`,可选 -v 连卷一起删(彻底清理时用)。
    pub async fn compose_down(
        &self,
        project_dir: &str,
        remove_volumes: bool,
    ) -> Result<(), DockerCliError> {
        let mut cmd = HostCommand::new("docker")
            .arg("compose")
            .arg("down")
            .working_dir(ncd_host::HostPath::from_posix(project_dir))
            .timeout(std::time::Duration::from_secs(300));
        if remove_volumes {
            cmd = cmd.arg("-v");
        }
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker compose down (in {project_dir})"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// `docker pull <image>`,部署前显式拉一次让进度有反馈。
    pub async fn pull(&self, image: &str) -> Result<(), DockerCliError> {
        let cmd = HostCommand::new("docker")
            .arg("pull")
            .arg(image)
            .timeout(std::time::Duration::from_secs(900));
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker pull {image}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }
}

/// 解析 `docker ps --format '{{json .}}'` 的多行 JSON 输出。
/// 每行一个容器对象;空行跳过;单行解析失败时整体报 ParseFailed。
fn parse_ps_json(stdout: &str) -> Result<Vec<ContainerInfo>, DockerCliError> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let raw: PsLine = serde_json::from_str(line)
            .map_err(|e| DockerCliError::ParseFailed(format!("{e}: {line}")))?;
        out.push(raw.into_info());
    }
    Ok(out)
}

/// `docker ps --format '{{json .}}'` 单行的字段子集。docker 这个格式的字段名
/// 是固定的(ID / Names / Image / State / Status / Ports)。
#[derive(serde::Deserialize)]
struct PsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Names")]
    names: String,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Ports", default)]
    ports: String,
}

impl PsLine {
    fn into_info(self) -> ContainerInfo {
        // Ports 形如 "0.0.0.0:6099->6099/tcp, :::6099->6099/tcp";按逗号拆开,
        // 去重空白。Names 多名时 docker 用逗号分隔,取第一个作主名。
        let ports: Vec<String> = self
            .ports
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let name = self
            .names
            .split(',')
            .next()
            .unwrap_or(&self.names)
            .trim()
            .to_string();
        ContainerInfo {
            id: self.id,
            name,
            image: self.image,
            state: ContainerState::parse(&self.state),
            status: self.status,
            ports,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ps_json_single_running_container() {
        let line = r#"{"ID":"abc123def456","Names":"napcat","Image":"mlikiowa/napcat-docker:latest","State":"running","Status":"Up 3 hours","Ports":"0.0.0.0:6099->6099/tcp, :::6099->6099/tcp"}"#;
        let parsed = parse_ps_json(line).unwrap();
        assert_eq!(parsed.len(), 1);
        let c = &parsed[0];
        assert_eq!(c.id, "abc123def456");
        assert_eq!(c.name, "napcat");
        assert_eq!(c.state, ContainerState::Running);
        assert_eq!(c.ports.len(), 2);
    }

    #[test]
    fn parse_ps_json_multiple_lines_and_blanks() {
        let stdout = "\n\
{\"ID\":\"a1\",\"Names\":\"napcat\",\"Image\":\"img:1\",\"State\":\"running\",\"Status\":\"Up\",\"Ports\":\"\"}\n\
\n\
{\"ID\":\"b2\",\"Names\":\"snowluma\",\"Image\":\"img:2\",\"State\":\"exited\",\"Status\":\"Exited (0)\",\"Ports\":\"\"}\n";
        let parsed = parse_ps_json(stdout).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "napcat");
        assert_eq!(parsed[1].state, ContainerState::Exited);
        // 空 Ports 字段解析成空 vec,不是 [""]。
        assert!(parsed[0].ports.is_empty());
    }

    #[test]
    fn parse_ps_json_empty_output_is_empty_vec() {
        assert!(parse_ps_json("").unwrap().is_empty());
        assert!(parse_ps_json("\n\n").unwrap().is_empty());
    }

    #[test]
    fn parse_ps_json_bad_line_errors() {
        let err = parse_ps_json("not json at all").unwrap_err();
        assert!(matches!(err, DockerCliError::ParseFailed(_)));
    }

    #[test]
    fn parse_ps_json_takes_first_name_when_multiple() {
        let line = r#"{"ID":"x","Names":"primary,alias","Image":"i","State":"created","Status":"Created","Ports":""}"#;
        let parsed = parse_ps_json(line).unwrap();
        assert_eq!(parsed[0].name, "primary");
        assert_eq!(parsed[0].state, ContainerState::Created);
    }
}

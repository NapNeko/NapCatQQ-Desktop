//! Docker 项目目录与配置渲染

use std::collections::HashMap;

use ncd_deploy::{DockerCli, DockerCliError, DockerDeployment};
use ncd_domain::ids::BotId;
use ncd_domain::{BackendType, BotConfig, DeploymentType, ImportedNetworkConfig};
use ncd_host::{Host, HostError, HostPath};
use ncd_traits::runtime_backend::BotBackendError;
use serde_json::Value;

use crate::backend_config_renderer::{
    render_napcat_docker_config_payloads, render_snowluma_docker_config_payloads,
};

fn docker_container_name(config: &BotConfig) -> String {
    DockerDeployment::container_name(config)
}

pub(crate) async fn docker_project_dir(
    host: &dyn Host,
    name: &str,
) -> Result<String, BotBackendError> {
    ncd_deploy::docker_project_dir(host, name)
        .await
        .map_err(BotBackendError::Io)
}

fn docker_config_file_names(bot_id: &BotId) -> [String; 3] {
    [
        format!("onebot11_{}.json", bot_id.as_str()),
        format!("napcat_{}.json", bot_id.as_str()),
        "napcat.json".to_string(),
    ]
}

/// 容器内回环无法接收宿主机 DNAT。只改派生配置，保留 bot.json 的用户输入。
pub(crate) fn config_with_docker_listener_hosts(config: &BotConfig) -> BotConfig {
    let mut config = config.clone();
    if config.bot.deployment_type != DeploymentType::Docker {
        return config;
    }
    fn normalize(enabled: bool, host: &mut String) {
        let value = host.trim();
        if enabled
            && (value.is_empty()
                || value.eq_ignore_ascii_case("localhost")
                || matches!(value, "127.0.0.1" | "::1" | "[::1]"))
        {
            *host = "0.0.0.0".into();
        }
    }
    for server in &mut config.connect.http_servers {
        normalize(server.base.enable, &mut server.host);
    }
    for server in &mut config.connect.http_sse_servers {
        normalize(server.base.enable, &mut server.host);
    }
    for server in &mut config.connect.websocket_servers {
        normalize(server.base.enable, &mut server.host);
    }
    config
}

pub(crate) async fn render_docker_config_on_host(
    host: &dyn Host,
    bot_id: &BotId,
    config: &BotConfig,
) -> Result<(), BotBackendError> {
    let config = config_with_docker_listener_hosts(config);
    let config = &config;
    let name = docker_container_name(config);
    let project_dir = docker_project_dir(host, &name).await?;
    match config.bot.backend_type {
        BackendType::NapCat => {
            let config_dir = format!("{project_dir}/napcat/config");
            let config_dir_path = HostPath::from_posix(&config_dir);
            host.create_dir_all(&config_dir_path)
                .await
                .map_err(|error| {
                    BotBackendError::Io(format!("创建 Docker 配置目录失败: {error}"))
                })?;

            let existing = read_existing_docker_napcat_config(host, bot_id, &config_dir).await?;
            for item in render_napcat_docker_config_payloads(bot_id, config, &existing) {
                let bytes = serde_json::to_vec_pretty(&item.payload)
                    .map_err(|error| BotBackendError::Json(error.to_string()))?;
                let path = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
                host.write_file(&path, &bytes).await.map_err(|error| {
                    BotBackendError::Io(format!("写 Docker 配置文件失败: {error}"))
                })?;
            }
        }
        BackendType::SnowLuma => {
            let config_dir = format!("{project_dir}/snowluma-data/config");
            let config_dir_path = HostPath::from_posix(&config_dir);
            host.create_dir_all(&config_dir_path)
                .await
                .map_err(|error| {
                    BotBackendError::Io(format!("创建 Docker 配置目录失败: {error}"))
                })?;

            let existing = read_existing_docker_snowluma_config(host, bot_id, &config_dir).await?;
            for item in render_snowluma_docker_config_payloads(bot_id, config, &existing) {
                let bytes = serde_json::to_vec_pretty(&item.payload)
                    .map_err(|error| BotBackendError::Json(error.to_string()))?;
                let path = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
                host.write_file(&path, &bytes).await.map_err(|error| {
                    BotBackendError::Io(format!("写 Docker 配置文件失败: {error}"))
                })?;
            }
        }
    }
    Ok(())
}

/// SnowLuma 的运行数据在 named volume，宿主机暂存文件需显式拷入容器。
/// 只替换当前账号的 OneBot 文件，不覆盖 WebUI 凭据或其它账号配置。
pub(crate) async fn sync_snowluma_docker_config(
    host: &dyn Host,
    cli: &DockerCli<'_>,
    config: &BotConfig,
    container: &str,
) -> Result<(), BotBackendError> {
    let project_dir = docker_project_dir(host, &docker_container_name(config)).await?;
    let file = format!("onebot_{}.json", config.bot.qq_id);
    let source = format!("{project_dir}/snowluma-data/config/{file}");
    let target_dir = "/app/snowluma-data/config";
    let target = format!("{target_dir}/{file}");
    let mkdir = host
        .run_to_string(
            cli.command()
                .arg("exec")
                .arg("--user")
                .arg("0")
                .arg(container)
                .arg("mkdir")
                .arg("-p")
                .arg(target_dir),
        )
        .await
        .map_err(|error| BotBackendError::Io(error.to_string()))?;
    if !mkdir.success() {
        return Err(BotBackendError::Io(format!(
            "创建 SnowLuma 容器配置目录失败：{}",
            mkdir.stderr.trim()
        )));
    }
    cli.copy_to_container(container, &source, &target)
        .await
        .map_err(|error| BotBackendError::Io(error.to_string()))?;
    // docker cp 默认归 root；与 compose 的运行用户保持一致，允许 WebUI 后续保存。
    let (uid, gid) = DockerDeployment::container_uid_gid(host);
    let chown = host
        .run_to_string(
            cli.command()
                .arg("exec")
                .arg("--user")
                .arg("0")
                .arg(container)
                .arg("chown")
                .arg(format!("{uid}:{gid}"))
                .arg("--")
                .arg(target_dir)
                .arg(&target),
        )
        .await
        .map_err(|error| BotBackendError::Io(error.to_string()))?;
    if !chown.success() {
        return Err(BotBackendError::Io(format!(
            "设置 SnowLuma 容器配置属主失败：{}",
            chown.stderr.trim()
        )));
    }
    // 宿主机 SFTP 文件可能使用 0600，显式保证协议进程可读。
    let chmod = host
        .run_to_string(
            cli.command()
                .arg("exec")
                .arg("--user")
                .arg("0")
                .arg(container)
                .arg("chmod")
                .arg("644")
                .arg("--")
                .arg(&target),
        )
        .await
        .map_err(|error| BotBackendError::Io(error.to_string()))?;
    if !chmod.success() {
        return Err(BotBackendError::Io(format!(
            "设置 SnowLuma 容器配置权限失败：{}",
            chmod.stderr.trim()
        )));
    }
    Ok(())
}

async fn read_existing_docker_napcat_config(
    host: &dyn Host,
    bot_id: &BotId,
    config_dir: &str,
) -> Result<HashMap<String, Value>, BotBackendError> {
    let mut existing = HashMap::new();
    for file_name in docker_config_file_names(bot_id) {
        let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
        match host.read_file(&path).await {
            Ok(bytes) => {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    existing.insert(file_name, value);
                }
            }
            Err(HostError::PathNotFound { .. }) => {}
            Err(error) => return Err(BotBackendError::Io(error.to_string())),
        }
    }
    Ok(existing)
}

/// 导入迁移：从桌面 compose 项目目录或容器内读取网络配置。
/// NapCat 先读 host bind，没有再 `docker cp`；SnowLuma 以容器 named volume 为权威。
pub(crate) async fn read_docker_imported_network(
    host: &dyn Host,
    home: &str,
    docker_name: &str,
    backend: BackendType,
    qq_id: &str,
) -> Result<Option<ImportedNetworkConfig>, String> {
    let project_dir = ncd_domain::remote_paths::docker_bot_project_dir(home, docker_name);
    match backend {
        BackendType::NapCat => {
            let host_path = format!("{project_dir}/napcat/config/onebot11_{qq_id}.json");
            if let Some(value) = read_json_if_exists(host, &host_path).await? {
                return parse_napcat_file(&host_path, &value);
            }
            match copy_container_json(
                host,
                docker_name,
                &format!("/app/napcat/config/onebot11_{qq_id}.json"),
                qq_id,
            )
            .await?
            {
                Some(value) => parse_napcat_file(
                    &format!("docker://{docker_name}/app/napcat/config/onebot11_{qq_id}.json"),
                    &value,
                ),
                None => Ok(None),
            }
        }
        BackendType::SnowLuma => {
            match copy_container_json(
                host,
                docker_name,
                &format!("/app/snowluma-data/config/onebot_{qq_id}.json"),
                qq_id,
            )
            .await?
            {
                Some(value) => parse_snowluma_file(
                    &format!("docker://{docker_name}/app/snowluma-data/config/onebot_{qq_id}.json"),
                    &value,
                ),
                None => {
                    let host_path =
                        format!("{project_dir}/snowluma-data/config/onebot_{qq_id}.json");
                    if let Some(value) = read_json_if_exists(host, &host_path).await? {
                        return parse_snowluma_file(&host_path, &value);
                    }
                    Ok(None)
                }
            }
        }
    }
}

/// NapCat 镜像里 webui.json 的固定位置（entrypoint.sh CONFIG_PATH）
pub(crate) const NAPCAT_DOCKER_WEBUI_JSON: &str = "/app/napcat/config/webui.json";

/// 容器里 NapCat 实际认的 WebUI token。entrypoint 只在 webui.json 不存在时写
/// WEBUI_TOKEN，导入 / 重装 / 换电脑后本机 SecretStore 那份多半对不上。
/// 先 docker exec（宿主机目录可能是旧部署残留），再读宿主机 bind 目录；都没有返回 None。
/// 失败只打 debug，任何分支都不记 token 本身
pub(crate) async fn read_napcat_docker_webui_token(
    host: &dyn Host,
    cli: Option<&DockerCli<'_>>,
    container: &str,
) -> Option<String> {
    if let Some(cli) = cli {
        match cli
            .exec_read_file(container, NAPCAT_DOCKER_WEBUI_JSON)
            .await
        {
            Ok(text) => {
                if let Some(token) = ncd_backend_napcat::parse_webui_json_token(&text) {
                    return Some(token);
                }
                tracing::debug!(
                    target: "ncd_runtime::remote",
                    %container,
                    "容器内 webui.json 没有可用 token"
                );
            }
            Err(error) => tracing::debug!(
                target: "ncd_runtime::remote",
                %container,
                %error,
                "docker exec 读 webui.json 失败，改读宿主机目录"
            ),
        }
    }
    let project_dir = ncd_deploy::docker_project_dir(host, container).await.ok()?;
    let path = format!("{project_dir}/napcat/config/webui.json");
    match host.read_file(&HostPath::from_posix(&path)).await {
        Ok(bytes) => ncd_backend_napcat::parse_webui_json_token(&String::from_utf8_lossy(&bytes)),
        Err(error) => {
            tracing::debug!(
                target: "ncd_runtime::remote",
                %path,
                %error,
                "宿主机 webui.json 不可读"
            );
            None
        }
    }
}

async fn read_json_if_exists(host: &dyn Host, path: &str) -> Result<Option<Value>, String> {
    match host.read_file(&HostPath::from_posix(path)).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("解析 {path} 失败: {e}")),
        Err(HostError::PathNotFound { .. }) => Ok(None),
        Err(err) => {
            tracing::info!(
                target: "ncd_runtime::remote",
                %path,
                %err,
                "远端 Docker 配置不存在或不可读，跳过网络配置迁移"
            );
            Ok(None)
        }
    }
}

fn parse_napcat_file(path: &str, value: &Value) -> Result<Option<ImportedNetworkConfig>, String> {
    ncd_deploy::backend_config_renderer::parse_napcat_onebot_connect(value)
        .map(Some)
        .ok_or_else(|| format!("{path} 缺少 network 字段，无法迁移网络配置"))
}

fn parse_snowluma_file(path: &str, value: &Value) -> Result<Option<ImportedNetworkConfig>, String> {
    ncd_deploy::backend_config_renderer::parse_snowluma_onebot_connect(value)
        .map(Some)
        .ok_or_else(|| format!("{path} 缺少 networks 字段，无法迁移网络配置"))
}

async fn copy_container_json(
    host: &dyn Host,
    docker_name: &str,
    src: &str,
    qq_id: &str,
) -> Result<Option<Value>, String> {
    let cli = DockerCli::new(host);
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪，无法从容器读取网络配置: {e}"))?;
    let dest = import_tmp_path(docker_name, qq_id);
    match cli.copy_from_container(docker_name, src, &dest).await {
        Ok(()) => {
            let result = match host.read_file(&HostPath::from_posix(&dest)).await {
                Ok(bytes) => serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(|e| format!("解析 {dest} 失败: {e}")),
                Err(HostError::PathNotFound { .. }) => Ok(None),
                Err(err) => Err(format!("读取 {dest} 失败: {err}")),
            };
            cli.remove_copied_file(&dest).await;
            result
        }
        Err(DockerCliError::CommandFailed { stderr, .. }) => {
            tracing::info!(
                target: "ncd_runtime::remote",
                container = %docker_name,
                %src,
                %stderr,
                "容器内没有可迁移的网络配置文件"
            );
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn import_tmp_path(docker_name: &str, qq_id: &str) -> String {
    let safe_name: String = docker_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe_qq: String = qq_id.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("/tmp/ncd-import-{safe_name}-{safe_qq}.json")
}

async fn read_existing_docker_snowluma_config(
    host: &dyn Host,
    bot_id: &BotId,
    config_dir: &str,
) -> Result<HashMap<String, Value>, BotBackendError> {
    let mut existing = HashMap::new();
    let file_name = format!("onebot_{}.json", bot_id.as_str());
    let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
    match host.read_file(&path).await {
        Ok(bytes) => {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                existing.insert(file_name, value);
            }
        }
        Err(HostError::PathNotFound { .. }) => {}
        Err(error) => return Err(BotBackendError::Io(error.to_string())),
    }
    Ok(existing)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;

    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostProcess, HostShell, Locality,
        Os, ShellKind,
    };

    struct NoopShell;
    impl HostShell for NoopShell {
        fn kind(&self) -> ShellKind {
            ShellKind::Bash
        }
        fn escape(&self, arg: &str) -> String {
            arg.to_string()
        }
        fn line_separator(&self) -> &'static str {
            "\n"
        }
    }
    static NOOP_SHELL: NoopShell = NoopShell;

    /// exec_stdout=None 表示 docker exec 失败（容器没起 / 没有该文件）
    pub(crate) struct TokenHost {
        exec_stdout: Option<String>,
        files: HashMap<String, Vec<u8>>,
    }

    const CONTAINER: &str = "ncbot-10001";

    fn bind_path() -> String {
        format!(
            "{}/napcat/config/webui.json",
            ncd_domain::remote_paths::docker_bot_project_dir("/home/u", CONTAINER)
        )
    }

    fn output(code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(code),
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    #[async_trait]
    impl Host for TokenHost {
        fn os(&self) -> Os {
            Os::Linux
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            "token-mock"
        }
        fn shell(&self) -> &dyn HostShell {
            &NOOP_SHELL
        }
        async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
            self.files
                .get(path.as_posix())
                .cloned()
                .map(Bytes::from)
                .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
            Ok(self.files.contains_key(path.as_posix()))
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            if cmd.program == "sh" && cmd.args == ["-c", "echo $HOME"] {
                return Ok(output(0, "/home/u\n"));
            }
            if cmd.program != "docker" {
                return Ok(output(0, ""));
            }
            match cmd.args.first().map(String::as_str) {
                Some("version") | Some("info") => Ok(output(0, "27.0.0\n")),
                Some("exec") => {
                    assert_eq!(
                        cmd.args,
                        ["exec", CONTAINER, "cat", NAPCAT_DOCKER_WEBUI_JSON]
                    );
                    match &self.exec_stdout {
                        Some(stdout) => Ok(output(0, stdout)),
                        None => Ok(output(1, "")),
                    }
                }
                _ => Ok(output(0, "")),
            }
        }
    }

    pub(crate) fn host(exec: Option<&str>, bind: Option<&str>) -> TokenHost {
        let mut files = HashMap::new();
        if let Some(body) = bind {
            files.insert(bind_path(), body.as_bytes().to_vec());
        }
        TokenHost {
            exec_stdout: exec.map(str::to_string),
            files,
        }
    }

    #[tokio::test]
    async fn read_token_prefers_docker_exec() {
        let h = host(
            Some(r#"{"token":"in-container","loginRate":3}"#),
            Some(r#"{"token":"stale-bind"}"#),
        );
        let cli = DockerCli::new(&h);
        assert_eq!(
            read_napcat_docker_webui_token(&h, Some(&cli), CONTAINER)
                .await
                .as_deref(),
            Some("in-container")
        );
    }

    #[tokio::test]
    async fn read_token_falls_back_to_host_bind() {
        let h = host(None, Some(r#"{"token":"from-bind"}"#));
        let cli = DockerCli::new(&h);
        assert_eq!(
            read_napcat_docker_webui_token(&h, Some(&cli), CONTAINER)
                .await
                .as_deref(),
            Some("from-bind")
        );
        // daemon 没就绪(cli=None)时直接读宿主机
        assert_eq!(
            read_napcat_docker_webui_token(&h, None, CONTAINER)
                .await
                .as_deref(),
            Some("from-bind")
        );
    }

    #[tokio::test]
    async fn read_token_none_when_both_missing() {
        let h = host(None, None);
        let cli = DockerCli::new(&h);
        assert_eq!(
            read_napcat_docker_webui_token(&h, Some(&cli), CONTAINER).await,
            None
        );
    }

    #[tokio::test]
    async fn read_token_none_for_blank_token() {
        let h = host(Some(r#"{"token":""}"#), Some(r#"{"token":"  "}"#));
        let cli = DockerCli::new(&h);
        assert_eq!(
            read_napcat_docker_webui_token(&h, Some(&cli), CONTAINER).await,
            None
        );
    }
}

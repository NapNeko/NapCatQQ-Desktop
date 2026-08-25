//! 远端 daemon 前置条件、配置渲染与场景判定

use std::collections::HashMap;

use ncd_deploy::backend_config_renderer::render_snowluma_docker_config_payloads;
use ncd_domain::{BackendType, BotConfig, BotId, RuntimeScenario, SnowLumaStartMode};
use ncd_host::{Host, HostCommand, HostError, HostPath};
use ncd_traits::runtime_backend::BotBackendError;
use serde_json::{Value, json};

use super::layout::{DEFAULT_WEBUI_PORT, SnowLumaRemotePaths, napcat_layout_qq_executable};
use super::orchestrator::resolve_remote_bash;
use crate::snowluma::session::{
    build_webui_json_payload, generate_strong_password, verify_webui_password,
};

use super::helpers::host_file_nonempty;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebuiCredentialSync {
    pub plaintext: String,
    pub wrote: bool,
}

pub(crate) async fn ensure_remote_daemon_prereqs(
    host: &dyn Host,
    home: &str,
    paths: &SnowLumaRemotePaths,
    qq_bin: &str,
) -> Result<WebuiCredentialSync, BotBackendError> {
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;

    let runtime_path = format!("{}/runtime.json", paths.config_dir);
    if !host_file_nonempty(host, &runtime_path).await {
        let runtime_json = serde_json::to_vec_pretty(&json!({ "webuiPort": DEFAULT_WEBUI_PORT }))
            .map_err(|e| BotBackendError::Json(e.to_string()))?;
        host.write_file(&HostPath::from_posix(&runtime_path), &runtime_json)
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }

    let creds = sync_remote_webui_credentials(host, paths).await?;

    if !host_file_nonempty(host, &paths.vnc_secret).await {
        let vnc_pwd = generate_strong_password(8);
        host.write_file(&HostPath::from_posix(&paths.vnc_secret), vnc_pwd.as_bytes())
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }

    resolve_remote_bash(host).await?;

    let qq = if qq_bin.trim().is_empty() {
        napcat_layout_qq_executable(home)
    } else {
        qq_bin.to_string()
    };
    let qq_q = qq.replace('\'', "'\"'\"'");
    let check = HostCommand::new("sh").arg("-c").arg(format!(
        "command -v Xvfb >/dev/null && command -v x11vnc >/dev/null && \
         command -v websockify >/dev/null && command -v dbus-launch >/dev/null && \
         test -x '{qq_q}'"
    ));
    let out = host
        .run_to_string(check)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        if host
            .run_to_string(
                HostCommand::new("sh")
                    .arg("-c")
                    .arg(format!("test -x '{qq_q}'")),
            )
            .await
            .ok()
            .is_some_and(|o| !o.success())
        {
            return Err(BotBackendError::InvalidConfig(format!(
                "远端未找到可执行的 QQ（期望 {qq}）。请先在同一 SSH 主机安装 QQ 组件。"
            )));
        }
        return Err(BotBackendError::InvalidConfig(
            "远端缺少 SnowLuma 图形栈（Xvfb、x11vnc、websockify、dbus-launch）。\
             请到「组件」页为该主机安装 noVNC；完整版 SnowLuma 不需要单独装 Node.js。"
                .into(),
        ));
    }

    Ok(creds)
}

/// 对齐上游 `WebuiAuth.load`：`mustChangePassword=true` 时 node 会丢掉明文、自己换密。
/// 桌面生成/接管的密码必须写成 false，且在启动 node 之前落盘。
async fn sync_remote_webui_credentials(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<WebuiCredentialSync, BotBackendError> {
    let webui_json_path = format!("{}/webui.json", paths.config_dir);
    let existing = read_webui_disk_creds(host, &webui_json_path).await;
    let secret = if host_file_nonempty(host, &paths.webui_secret).await {
        let bytes = host
            .read_file(&HostPath::from_posix(&paths.webui_secret))
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
        String::from_utf8_lossy(&bytes).trim().to_string()
    } else {
        String::new()
    };

    if let Some(disk) = existing {
        let matches = !secret.is_empty() && verify_webui_password(&secret, &disk.hash, &disk.salt);
        if matches && !disk.must_change {
            return Ok(WebuiCredentialSync {
                plaintext: secret,
                wrote: false,
            });
        }
        if matches && disk.must_change {
            write_webui_pair(host, paths, &webui_json_path, &secret).await?;
            return Ok(WebuiCredentialSync {
                plaintext: secret,
                wrote: true,
            });
        }
        if disk.must_change {
            let plain = if secret.is_empty() {
                let pwd = generate_strong_password(16);
                host.write_file(&HostPath::from_posix(&paths.webui_secret), pwd.as_bytes())
                    .await
                    .map_err(|e| BotBackendError::Io(e.to_string()))?;
                pwd
            } else {
                secret
            };
            write_webui_pair(host, paths, &webui_json_path, &plain).await?;
            return Ok(WebuiCredentialSync {
                plaintext: plain,
                wrote: true,
            });
        }
        return Ok(WebuiCredentialSync {
            plaintext: secret,
            wrote: false,
        });
    }

    let plain = if secret.is_empty() {
        let pwd = generate_strong_password(16);
        host.write_file(&HostPath::from_posix(&paths.webui_secret), pwd.as_bytes())
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
        pwd
    } else {
        secret
    };
    write_webui_pair(host, paths, &webui_json_path, &plain).await?;
    Ok(WebuiCredentialSync {
        plaintext: plain,
        wrote: true,
    })
}

async fn write_webui_pair(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    webui_json_path: &str,
    plain: &str,
) -> Result<(), BotBackendError> {
    let webui_payload =
        build_webui_json_payload(plain, false).map_err(|e| BotBackendError::Io(e.to_string()))?;
    let webui_json = serde_json::to_vec_pretty(&webui_payload)
        .map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(&HostPath::from_posix(&paths.webui_secret), plain.as_bytes())
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    host.write_file(&HostPath::from_posix(webui_json_path), &webui_json)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(())
}

struct WebuiDiskCreds {
    hash: String,
    salt: String,
    must_change: bool,
}

async fn read_webui_disk_creds(host: &dyn Host, path: &str) -> Option<WebuiDiskCreds> {
    let bytes = host.read_file(&HostPath::from_posix(path)).await.ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    let hash = v.get("passwordHash")?.as_str()?.trim().to_string();
    let salt = v.get("passwordSalt")?.as_str()?.trim().to_string();
    if hash.is_empty() || salt.is_empty() {
        return None;
    }
    let must_change = v
        .get("mustChangePassword")
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    Some(WebuiDiskCreds {
        hash,
        salt,
        must_change,
    })
}

/// 接管用明文：全局固定密码非空则用它，否则每次启动生成新密码。
pub(crate) fn resolve_webui_takeover_password(override_pwd: Option<&str>) -> String {
    match override_pwd.map(str::trim).filter(|s| !s.is_empty()) {
        Some(value) => value.to_string(),
        None => generate_strong_password(16),
    }
}

/// 接管模式：覆盖远端 WebUI 凭据并同步写入 webui.json 与 webui.secret。
/// 固定密码（`password_override` 非空）优先，否则每次调用生成新密码。
/// 仅写磁盘；调用方必须在已有 node 上重启进程，SnowLuma 只在启动时加载哈希。
pub async fn take_over_remote_webui_credentials(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    password_override: Option<&str>,
) -> Result<String, BotBackendError> {
    let pwd = resolve_webui_takeover_password(password_override);
    let payload =
        build_webui_json_payload(&pwd, false).map_err(|e| BotBackendError::Io(e.to_string()))?;
    let webui_json =
        serde_json::to_vec_pretty(&payload).map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(&HostPath::from_posix(&paths.webui_secret), pwd.as_bytes())
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    let webui_json_path = format!("{}/webui.json", paths.config_dir);
    host.write_file(&HostPath::from_posix(&webui_json_path), &webui_json)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(pwd)
}

/// 导入迁移：读取远端 onebot_<qq>.json 并反向映射为桌面网络配置。
/// 文件不存在返回 Ok(None)（该实例没有可迁移的网络配置）；
/// 存在但解析失败返回 Err，由上层提示用户。
pub async fn read_remote_onebot_connect(
    host: &dyn Host,
    snowluma_dir: &str,
    qq_id: &str,
) -> Result<Option<ncd_domain::ImportedNetworkConfig>, String> {
    let path = format!(
        "{}/config/onebot_{}.json",
        snowluma_dir.trim_end_matches('/'),
        qq_id
    );
    let bytes = match host.read_file(&HostPath::from_posix(&path)).await {
        Ok(bytes) => bytes,
        Err(HostError::PathNotFound { .. }) => {
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                %path,
                "远端 onebot 配置不存在，跳过网络配置迁移"
            );
            return Ok(None);
        }
        Err(err) => {
            return Err(format!("读取 {path} 失败: {err}"));
        }
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析 {path} 失败: {e}"))?;
    ncd_deploy::backend_config_renderer::parse_snowluma_onebot_connect(&value)
        .map(Some)
        .ok_or_else(|| format!("{path} 缺少 networks 字段，无法迁移网络配置"))
}

pub async fn render_native_snowluma_config_on_host(
    host: &dyn Host,
    bot_id: &BotId,
    config: &BotConfig,
    paths: &SnowLumaRemotePaths,
) -> Result<(), BotBackendError> {
    if config.bot.backend_type != BackendType::SnowLuma {
        return Err(BotBackendError::InvalidConfig(
            "render_native_snowluma_config_on_host 仅支持 SnowLuma".into(),
        ));
    }
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    let config_dir = &paths.config_dir;
    let mut existing = HashMap::new();
    let file_name = format!("onebot_{}.json", bot_id.as_str());
    let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
    if let Ok(bytes) = host.read_file(&path).await {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            existing.insert(file_name.clone(), value);
        }
    }
    for item in render_snowluma_docker_config_payloads(bot_id, config, &existing) {
        let bytes = serde_json::to_vec_pretty(&item.payload)
            .map_err(|e| BotBackendError::Json(e.to_string()))?;
        let p = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
        host.write_file(&p, &bytes)
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn resolve_start_mode(config: &BotConfig) -> SnowLumaStartMode {
    config
        .bot
        .snowluma_start_mode
        .unwrap_or(SnowLumaStartMode::ColdStart)
}

/// 远端 Native + SnowLuma + SSH 主机
pub fn is_remote_native_snowluma_config(config: &BotConfig) -> bool {
    RuntimeScenario::from_config(config)
        .map(|scenario| scenario.is_remote_native_snowluma())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostProcess, HostShell,
        Locality, Os, PackageManager, ShellKind,
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

    struct FileHost {
        files: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl FileHost {
        fn new() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl Host for FileHost {
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
            "webui-takeover-mock"
        }
        fn shell(&self) -> &dyn HostShell {
            &NOOP_SHELL
        }
        fn pkg_manager(&self) -> Option<&dyn PackageManager> {
            None
        }
        async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
            self.files
                .lock()
                .unwrap()
                .get(path.as_posix())
                .cloned()
                .map(Bytes::from)
                .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
        }
        async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
            self.files
                .lock()
                .unwrap()
                .insert(path.as_posix().to_string(), bytes.to_vec());
            Ok(())
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
            Ok(self.files.lock().unwrap().contains_key(path.as_posix()))
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
        async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
            Ok(CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn takeover_password_uses_fixed_override() {
        assert_eq!(
            resolve_webui_takeover_password(Some("FixedPass!1")),
            "FixedPass!1"
        );
        assert_eq!(
            resolve_webui_takeover_password(Some("  KeepMe@9  ")),
            "KeepMe@9"
        );
    }

    #[test]
    fn takeover_password_generates_when_override_blank() {
        for empty in [None, Some(""), Some("  "), Some("\t\n")] {
            let pwd = resolve_webui_takeover_password(empty);
            assert!(
                pwd.len() >= 10,
                "generated password too short for {empty:?}: {pwd}"
            );
            assert_ne!(pwd, empty.unwrap_or_default().trim());
        }
        let a = resolve_webui_takeover_password(None);
        let b = resolve_webui_takeover_password(None);
        assert_ne!(
            a, b,
            "each start without override should get a new password"
        );
    }

    #[tokio::test]
    async fn takeover_writes_override_over_existing_hash() {
        let host = FileHost::new();
        let paths = SnowLumaRemotePaths::from_remote_home("/home/u");
        let first = take_over_remote_webui_credentials(&host, &paths, None)
            .await
            .expect("first generate");
        let second = take_over_remote_webui_credentials(&host, &paths, Some("Override#12"))
            .await
            .expect("override");
        assert_ne!(first, second);
        assert_eq!(second, "Override#12");

        let secret = host
            .read_file(&HostPath::from_posix(&paths.webui_secret))
            .await
            .expect("secret");
        assert_eq!(String::from_utf8_lossy(&secret).trim(), "Override#12");

        let webui_json_path = format!("{}/webui.json", paths.config_dir);
        let json_bytes = host
            .read_file(&HostPath::from_posix(&webui_json_path))
            .await
            .expect("webui.json");
        let v: serde_json::Value = serde_json::from_slice(&json_bytes).expect("parse");
        let hash = v["passwordHash"].as_str().expect("hash");
        let salt = v["passwordSalt"].as_str().expect("salt");
        assert!(verify_webui_password("Override#12", hash, salt));
        assert!(!verify_webui_password(&first, hash, salt));
    }

    #[tokio::test]
    async fn sync_rewrites_bootstrap_must_change_so_node_will_not_rotate() {
        let host = FileHost::new();
        let paths = SnowLumaRemotePaths::from_remote_home("/home/u");
        let bootstrap = build_webui_json_payload("Bootstrap#12", true).expect("payload");
        let webui_json_path = format!("{}/webui.json", paths.config_dir);
        host.write_file(
            &HostPath::from_posix(&webui_json_path),
            &serde_json::to_vec_pretty(&bootstrap).unwrap(),
        )
        .await
        .unwrap();

        let sync = sync_remote_webui_credentials(&host, &paths)
            .await
            .expect("sync");
        assert!(sync.wrote);
        assert!(!sync.plaintext.is_empty());

        let json_bytes = host
            .read_file(&HostPath::from_posix(&webui_json_path))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(v["mustChangePassword"], serde_json::Value::Bool(false));
        let hash = v["passwordHash"].as_str().unwrap();
        let salt = v["passwordSalt"].as_str().unwrap();
        assert!(verify_webui_password(&sync.plaintext, hash, salt));
        assert_eq!(hash.len(), 128);
        assert_eq!(salt.len(), 32);
    }
}

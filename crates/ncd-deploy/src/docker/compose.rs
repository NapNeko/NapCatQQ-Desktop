//! 把 DockerDeploySpec 渲染成 docker-compose.yml 文本。
//!
//! 两种口味的差异(来自官方镜像 README / compose):
//! - NapCat:bind 挂载 ./napcat/config 和 ./ntqq 到 project 目录,环境变量
//!   WEBUI_TOKEN(我们生成)+ 可选 ACCOUNT。
//! - SnowLuma:named volume,且必须带 shm_size / cap_add SYS_PTRACE /
//!   security_opt seccomp=unconfined,环境变量 VNC_PASSWD(我们生成)。
//!
//! 凭据(token / vnc 密码)由调用方生成后通过 secret 参数传进来,这里只负责把它
//! 拼进 yaml,不自己造随机值(纯函数好测)。

use ncd_domain::{DockerDeploySpec, DockerFlavor};

/// compose 中凭据的来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeSecret<'a> {
    /// 直接把调用方提供的值写进 compose 环境变量。
    Literal(&'a str),
    /// 通过 compose 的变量替换从同目录 .env 读取。
    EnvRef { variable: &'a str },
}

impl<'a> ComposeSecret<'a> {
    fn napcat_webui_token(&self) -> String {
        match self {
            Self::Literal(token) => format!("\"{token}\""),
            Self::EnvRef { variable } => format!("\"${{{}:?{} is required}}\"", variable, variable),
        }
    }

    fn snowluma_vnc_passwd(&self) -> String {
        match self {
            Self::Literal(passwd) => format!("\"{}\"", escape_yaml_double_quoted(passwd)),
            Self::EnvRef { variable } => format!("\"${{{variable}}}\"")
        }
    }

    fn snowluma_webui_bootstrap(&self) -> String {
        match self {
            Self::Literal(passwd) => format!("\"{}\"", escape_yaml_double_quoted(passwd)),
            Self::EnvRef { variable } => format!("\"${{{variable}}}\"")
        }
    }
}

/// 写入 YAML 双引号字符串内的字面量（非 compose 插值）。
fn escape_yaml_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// 渲染 compose 文件文本。
///
/// secret 是调用方生成的凭据:NapCat 当 WEBUI_TOKEN,SnowLuma 当 VNC_PASSWD。
/// uid / gid 写进对应的 *_UID / *_GID 环境变量(Linux 文件属主对齐);
/// 本地 Windows Docker Desktop 传 0 即可。
pub fn render_compose(spec: &DockerDeploySpec, secret: &str, uid: u32, gid: u32) -> String {
    render_compose_with_secret(spec, ComposeSecret::Literal(secret), uid, gid, None)
}

/// 渲染引用同目录 .env 的 compose 文件文本。
pub fn render_compose_with_env(
    spec: &DockerDeploySpec,
    variable: &str,
    uid: u32,
    gid: u32,
) -> String {
    render_compose_with_secret(spec, ComposeSecret::EnvRef { variable }, uid, gid, None)
}

/// SnowLuma：VNC 与 WebUI bootstrap 分别来自 .env 中两个变量。
pub fn render_snowluma_compose_with_env(
    spec: &DockerDeploySpec,
    vnc_var: &str,
    webui_bootstrap_var: &str,
    uid: u32,
    gid: u32,
) -> String {
    render_compose_with_secret(
        spec,
        ComposeSecret::EnvRef { variable: vnc_var },
        uid,
        gid,
        Some(ComposeSecret::EnvRef {
            variable: webui_bootstrap_var,
        }),
    )
}

fn render_compose_with_secret(
    spec: &DockerDeploySpec,
    secret: ComposeSecret<'_>,
    uid: u32,
    gid: u32,
    snowluma_webui_bootstrap: Option<ComposeSecret<'_>>,
) -> String {
    match spec.flavor {
        DockerFlavor::NapCat => render_napcat(spec, secret, uid, gid),
        DockerFlavor::SnowLuma => render_snowluma(spec, secret, snowluma_webui_bootstrap, uid, gid),
    }
}

/// 把端口列表渲染成 yaml 的 ports 块行(已带缩进)。
fn render_ports(spec: &DockerDeploySpec) -> String {
    let mut out = String::new();
    for p in &spec.ports {
        out.push_str(&format!("      - \"{}:{}\"\n", p.host, p.container));
    }
    out
}

fn render_napcat(spec: &DockerDeploySpec, secret: ComposeSecret<'_>, uid: u32, gid: u32) -> String {
    let name = &spec.container_name;
    let image = DockerFlavor::NapCat.default_image();
    let ports = render_ports(spec);
    let token = secret.napcat_webui_token();
    // ACCOUNT 仅在用户预绑了 QQ 号时才写,避免空值干扰镜像 entrypoint 分支。
    let account_line = match spec.qq_id {
        Some(qq) if qq != 0 => format!("      ACCOUNT: \"{qq}\"\n"),
        _ => String::new(),
    };
    format!(
        "services:\n\
         \x20 napcat:\n\
         \x20   image: {image}\n\
         \x20   container_name: {name}\n\
         \x20   restart: always\n\
         \x20   environment:\n\
         \x20     NAPCAT_UID: \"{uid}\"\n\
         \x20     NAPCAT_GID: \"{gid}\"\n\
         \x20     WEBUI_TOKEN: {token}\n\
         {account_line}\
         \x20   ports:\n\
         {ports}\
         \x20   volumes:\n\
         \x20     - ./napcat/config:/app/napcat/config\n\
         \x20     - ./ntqq:/app/.config/QQ\n"
    )
}

fn render_snowluma(
    spec: &DockerDeploySpec,
    vnc_secret: ComposeSecret<'_>,
    webui_bootstrap: Option<ComposeSecret<'_>>,
    uid: u32,
    gid: u32,
) -> String {
    let name = &spec.container_name;
    let image = DockerFlavor::SnowLuma.default_image();
    let ports = render_ports(spec);
    let vnc_passwd = vnc_secret.snowluma_vnc_passwd();
    let webui_env = webui_bootstrap
        .map(|s| format!("      SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD: {}\n", s.snowluma_webui_bootstrap()))
        .unwrap_or_default();
    // SnowLuma 必须的安全选项 + named volume,照官方 docker-compose.yml。
    format!(
        "services:\n\
         \x20 snowluma:\n\
         \x20   image: \"{image}\"\n\
         \x20   container_name: {name}\n\
         \x20   restart: unless-stopped\n\
         \x20   shm_size: 1gb\n\
         \x20   cap_add:\n\
         \x20     - SYS_PTRACE\n\
         \x20   security_opt:\n\
         \x20     - \"seccomp=unconfined\"\n\
         \x20   environment:\n\
         \x20     SNOWLUMA_UID: \"{uid}\"\n\
         \x20     SNOWLUMA_GID: \"{gid}\"\n\
         \x20     VNC_PASSWD: {vnc_passwd}\n\
         {webui_env}\
         \x20     SNOWLUMA_WEBUI_PORT: \"5099\"\n\
         \x20     SNOWLUMA_HOOK_AUTOLOAD: \"1\"\n\
         \x20   ports:\n\
         {ports}\
         \x20   volumes:\n\
         \x20     - {name}-data:/app/snowluma-data\n\
         \x20     - {name}-qq-config:/app/.config\n\
         \x20     - {name}-qq-data:/app/.local/share\n\
         volumes:\n\
         \x20 {name}-data:\n\
         \x20 {name}-qq-config:\n\
         \x20 {name}-qq-data:\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn napcat_compose_has_image_ports_token() {
        let spec = DockerDeploySpec::napcat_default();
        let yaml = render_compose(&spec, "secret-token-123", 1000, 1000);
        assert!(yaml.contains("mlikiowa/napcat-docker:latest"));
        assert!(yaml.contains("container_name: napcat"));
        assert!(yaml.contains("WEBUI_TOKEN: \"secret-token-123\""));
        assert!(yaml.contains("\"6099:6099\""));
        assert!(yaml.contains("./napcat/config:/app/napcat/config"));
        // 没预绑 QQ 时不写 ACCOUNT。
        assert!(!yaml.contains("ACCOUNT"));
    }

    #[test]
    fn napcat_compose_can_reference_env_token() {
        let spec = DockerDeploySpec::napcat_default();
        let yaml = render_compose_with_env(&spec, "WEBUI_TOKEN", 1000, 1000);
        assert!(yaml.contains("WEBUI_TOKEN: \"${WEBUI_TOKEN:?WEBUI_TOKEN is required}\""));
        assert!(!yaml.contains("ncbot10001"));
    }

    #[test]
    fn napcat_compose_writes_account_when_qq_present() {
        let mut spec = DockerDeploySpec::napcat_default();
        spec.qq_id = Some(10001);
        let yaml = render_compose(&spec, "t", 0, 0);
        assert!(yaml.contains("ACCOUNT: \"10001\""));
    }

    #[test]
    fn napcat_compose_skips_account_when_zero() {
        let mut spec = DockerDeploySpec::napcat_default();
        spec.qq_id = Some(0);
        let yaml = render_compose(&spec, "t", 0, 0);
        assert!(!yaml.contains("ACCOUNT"));
    }

    #[test]
    fn snowluma_compose_is_valid_yaml() {
        let mut spec = DockerDeploySpec::snowluma_default();
        spec.container_name = "slbot-572381217".to_string();
        let yaml = render_snowluma_compose_with_env(
            &spec,
            "VNC_PASSWD",
            "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD",
            1000,
            1000,
        );
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).unwrap_or_else(|e| {
            eprintln!("--- compose yaml ---\n{yaml}\n--- end ---");
            panic!("snowluma compose must parse as YAML: {e}");
        });
        assert!(parsed.get("services").is_some());
        for (lineno, line) in yaml.lines().enumerate() {
            if lineno == 14 {
                // 0-based line 14 = file line 15
                assert!(
                    !line.contains("mapping values"),
                    "line 15 should be valid: {line:?}"
                );
            }
        }
    }

    #[test]
    fn snowluma_compose_yaml_environment_block_parses() {
        let spec = DockerDeploySpec::snowluma_default();
        let yaml = render_snowluma_compose_with_env(
            &spec,
            "VNC_PASSWD",
            "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD",
            1000,
            1000,
        );
        for (i, line) in yaml.lines().enumerate() {
            if line.contains("environment:") {
                // 下一行起每个 env 键应同为 6 空格缩进（services 下 environment 子项）
                for env_line in yaml.lines().skip(i + 1) {
                    if env_line.trim_start().starts_with("SNOWLUMA_")
                        || env_line.trim_start().starts_with("VNC_PASSWD")
                    {
                        assert!(
                            env_line.starts_with("      "),
                            "bad indent at line {}: {:?}",
                            i + 1,
                            env_line
                        );
                    }
                    if env_line.trim_start().starts_with("ports:") {
                        break;
                    }
                }
                break;
            }
        }
        assert!(yaml.contains("SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD: \"${SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD}\""));
    }

    #[test]
    fn snowluma_compose_can_reference_env_webui_bootstrap() {
        let spec = DockerDeploySpec::snowluma_default();
        let yaml = render_snowluma_compose_with_env(
            &spec,
            "VNC_PASSWD",
            "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD",
            1000,
            1000,
        );
        assert!(yaml.contains("VNC_PASSWD: \"${VNC_PASSWD}\""));
        assert!(yaml.contains(
            "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD: \"${SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD}\""
        ));
    }

    #[test]
    fn snowluma_compose_has_security_options_and_volumes() {
        let spec = DockerDeploySpec::snowluma_default();
        let yaml = render_compose(&spec, "vncpass", 1000, 1000);
        assert!(yaml.contains("motricseven7/snowluma:latest"));
        assert!(yaml.contains("shm_size: 1gb"));
        assert!(yaml.contains("- SYS_PTRACE"));
        assert!(yaml.contains("seccomp=unconfined"));
        assert!(yaml.contains("VNC_PASSWD: \"vncpass\""));
        // named volume 以容器名为前缀,避免多容器撞卷。
        assert!(yaml.contains("snowluma-data:/app/snowluma-data"));
        assert!(yaml.contains("volumes:\n  snowluma-data:"));
    }

    #[test]
    fn snowluma_compose_uses_custom_container_name_for_volumes() {
        let mut spec = DockerDeploySpec::snowluma_default();
        spec.container_name = "sl2".to_string();
        let yaml = render_compose(&spec, "p", 0, 0);
        assert!(yaml.contains("container_name: sl2"));
        assert!(yaml.contains("- sl2-data:/app/snowluma-data"));
        assert!(yaml.contains("volumes:\n  sl2-data:"));
    }
}

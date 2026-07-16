//! 把 DockerDeploySpec 渲染成 docker-compose.yml 文本
//!
//! 两种口味的差异(来自官方镜像 README / compose):
//! - NapCat:bind 挂载 ./napcat/config 和 ./ntqq 到 project 目录,环境变量
//!   WEBUI_TOKEN(我们生成)+ 可选 ACCOUNT
//! - SnowLuma:named volume,且必须带 shm_size / cap_add SYS_PTRACE /
//!   security_opt seccomp=unconfined,环境变量 VNC_PASSWD(我们生成)
//!
//! 凭据(token / vnc 密码)由调用方生成后通过 secret 参数传进来,这里只负责把它
//! 拼进 yaml,不自己造随机值(纯函数好测)
//!
//! 可选 DockerMetricsOverlay:实例指标探针(NCD_METRICS_* + bind 卷)。
//! 关指标时不传 overlay,输出与现网基线一致。
//!
//! 模板渲染走 ncd-template(MiniJinja),凭据字段用 |tojson 过滤器做 YAML 安全
//! 转义,避免含特殊字符的值破坏 compose 结构

use std::sync::OnceLock;

use ncd_domain::{DockerDeploySpec, DockerFlavor};
use ncd_template::TemplateEngine;
use serde::Serialize;

/// 容器内 metrics 挂载点(宿主机 ncd-watch/metrics 整目录 bind)
pub const DOCKER_METRICS_CONTAINER_ROOT: &str = "/ncd-metrics";
/// 官方 NapCat-Docker 镜像内 load 入口(Dockerfile 写死)
pub const DOCKER_NAPCAT_LOAD_CONTAINER_PATH: &str = "/opt/QQ/resources/app/loadNapCat.js";
/// 官方镜像内 napcat 入口 URI
pub const DOCKER_NAPCAT_MJS_URI: &str = "file:///app/napcat/napcat.mjs";

/// compose 可选 metrics 注入(路径均为容器内语义; volume 行为 host:container)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DockerMetricsOverlay {
    /// NCD_METRICS_OUT(容器路径)
    pub out: String,
    /// NCD_METRICS_NODES_PATH(容器路径)
    pub nodes: String,
    pub interval_ms: u64,
    /// 例如 `/home/u/ncd-watch/metrics:/ncd-metrics`
    pub metrics_volume: String,
    /// NapCat: 覆盖 loadNapCat.js 的 bind; SnowLuma 为 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_volume: Option<String>,
    /// SnowLuma: NODE_OPTIONS=--require ...; NapCat 必须为 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_options: Option<String>,
}

impl DockerMetricsOverlay {
    /// 容器内路径:(out, nodes, probe)；整目录挂载 /ncd-metrics 时使用
    pub fn container_paths(bot_id: &str) -> (String, String, String) {
        let root = DOCKER_METRICS_CONTAINER_ROOT;
        (
            format!("{root}/{bot_id}/net-stats.json"),
            format!("{root}/{bot_id}/nodes.json"),
            format!("{root}/ncd-ob11-stats.cjs"),
        )
    }

    pub fn for_napcat(
        host_metrics_root: &str,
        host_load_script: &str,
        bot_id: &str,
        interval_ms: u64,
    ) -> Self {
        let (out, nodes, _) = Self::container_paths(bot_id);
        let host_root = host_metrics_root.trim_end_matches('/');
        Self {
            out,
            nodes,
            interval_ms,
            metrics_volume: format!("{host_root}:{DOCKER_METRICS_CONTAINER_ROOT}"),
            load_volume: Some(format!(
                "{host_load_script}:{DOCKER_NAPCAT_LOAD_CONTAINER_PATH}:ro"
            )),
            node_options: None,
        }
    }

    pub fn for_snowluma(host_metrics_root: &str, bot_id: &str, interval_ms: u64) -> Self {
        let (out, nodes, probe) = Self::container_paths(bot_id);
        let host_root = host_metrics_root.trim_end_matches('/');
        Self {
            out,
            nodes,
            interval_ms,
            metrics_volume: format!("{host_root}:{DOCKER_METRICS_CONTAINER_ROOT}"),
            load_volume: None,
            node_options: Some(format!("--require \"{probe}\"")),
        }
    }
}

/// compose 中凭据的来源
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeSecret<'a> {
    /// 直接把调用方提供的值写进 compose 环境变量
    Literal(&'a str),
    /// 通过 compose 的变量替换从同目录 .env 读取
    EnvRef { variable: &'a str },
}

impl<'a> ComposeSecret<'a> {
    /// NapCat WEBUI_TOKEN 的原始值(模板用 tojson 加引号和转义)
    fn napcat_webui_token(&self) -> String {
        match self {
            Self::Literal(token) => token.to_string(),
            Self::EnvRef { variable } => format!("${{{}:?{} is required}}", variable, variable),
        }
    }

    /// SnowLuma VNC_PASSWD 的原始值
    fn snowluma_vnc_passwd(&self) -> String {
        match self {
            Self::Literal(passwd) => passwd.to_string(),
            Self::EnvRef { variable } => format!("${{{variable}}}"),
        }
    }

    /// SnowLuma WebUI bootstrap 密码的原始值
    fn snowluma_webui_bootstrap(&self) -> String {
        match self {
            Self::Literal(passwd) => passwd.to_string(),
            Self::EnvRef { variable } => format!("${{{variable}}}"),
        }
    }
}

/// 模板引擎单例(内置模板编译期嵌入,初始化一次后只读复用)
fn engine() -> &'static TemplateEngine {
    static ENGINE: OnceLock<TemplateEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        #[allow(clippy::expect_used)]
        TemplateEngine::with_builtin_templates().expect("内置 compose 模板语法不应错误")
    })
}

/// 渲染 compose 文件文本
///
/// secret 是调用方生成的凭据:NapCat 当 WEBUI_TOKEN,SnowLuma 当 VNC_PASSWD
/// uid / gid 写进对应的 *_UID / *_GID 环境变量(Linux 文件属主对齐)
pub fn render_compose(spec: &DockerDeploySpec, secret: &str, uid: u32, gid: u32) -> String {
    render_compose_with_secret(spec, ComposeSecret::Literal(secret), uid, gid, None, None)
}

/// 渲染引用同目录 .env 的 compose 文件文本
pub fn render_compose_with_env(
    spec: &DockerDeploySpec,
    variable: &str,
    uid: u32,
    gid: u32,
) -> String {
    render_compose_with_metrics(spec, variable, uid, gid, None)
}

/// 同 render_compose_with_env,可附带 metrics overlay
pub fn render_compose_with_metrics(
    spec: &DockerDeploySpec,
    variable: &str,
    uid: u32,
    gid: u32,
    metrics: Option<&DockerMetricsOverlay>,
) -> String {
    render_compose_with_secret(
        spec,
        ComposeSecret::EnvRef { variable },
        uid,
        gid,
        None,
        metrics,
    )
}

/// SnowLuma:VNC 与 WebUI bootstrap 分别来自 .env 中两个变量
pub fn render_snowluma_compose_with_env(
    spec: &DockerDeploySpec,
    vnc_var: &str,
    webui_bootstrap_var: &str,
    uid: u32,
    gid: u32,
) -> String {
    render_snowluma_compose_with_metrics(spec, vnc_var, webui_bootstrap_var, uid, gid, None)
}

/// 同 render_snowluma_compose_with_env,可附带 metrics overlay
pub fn render_snowluma_compose_with_metrics(
    spec: &DockerDeploySpec,
    vnc_var: &str,
    webui_bootstrap_var: &str,
    uid: u32,
    gid: u32,
    metrics: Option<&DockerMetricsOverlay>,
) -> String {
    render_compose_with_secret(
        spec,
        ComposeSecret::EnvRef { variable: vnc_var },
        uid,
        gid,
        Some(ComposeSecret::EnvRef {
            variable: webui_bootstrap_var,
        }),
        metrics,
    )
}

fn render_compose_with_secret(
    spec: &DockerDeploySpec,
    secret: ComposeSecret<'_>,
    uid: u32,
    gid: u32,
    snowluma_webui_bootstrap: Option<ComposeSecret<'_>>,
    metrics: Option<&DockerMetricsOverlay>,
) -> String {
    match spec.flavor {
        DockerFlavor::NapCat => render_napcat(spec, secret, uid, gid, metrics),
        DockerFlavor::SnowLuma => {
            render_snowluma(spec, secret, snowluma_webui_bootstrap, uid, gid, metrics)
        }
    }
}

/// 把端口列表序列化为模板可消费的格式
#[derive(Serialize)]
struct PortView {
    host: u16,
    container: u16,
}

fn port_views(spec: &DockerDeploySpec) -> Vec<PortView> {
    spec.ports
        .iter()
        .map(|p| PortView {
            host: p.host,
            container: p.container,
        })
        .collect()
}

fn render_napcat(
    spec: &DockerDeploySpec,
    secret: ComposeSecret<'_>,
    uid: u32,
    gid: u32,
    metrics: Option<&DockerMetricsOverlay>,
) -> String {
    #[derive(Serialize)]
    struct Ctx<'a> {
        image: &'static str,
        name: String,
        uid: u32,
        gid: u32,
        token: String,
        qq_id: Option<u64>,
        ports: Vec<PortView>,
        metrics: Option<&'a DockerMetricsOverlay>,
    }

    let ctx = Ctx {
        image: DockerFlavor::NapCat.default_image(),
        name: spec.container_name.clone(),
        uid,
        gid,
        token: secret.napcat_webui_token(),
        qq_id: spec.qq_id.filter(|&qq| qq != 0),
        ports: port_views(spec),
        metrics,
    };

    #[allow(clippy::expect_used)]
    engine()
        .render("napcat.yml", &ctx)
        .expect("napcat 模板渲染不应失败(上下文由代码完全控制)")
}

fn render_snowluma(
    spec: &DockerDeploySpec,
    vnc_secret: ComposeSecret<'_>,
    webui_bootstrap: Option<ComposeSecret<'_>>,
    uid: u32,
    gid: u32,
    metrics: Option<&DockerMetricsOverlay>,
) -> String {
    #[derive(Serialize)]
    struct Ctx<'a> {
        image: &'static str,
        name: String,
        uid: u32,
        gid: u32,
        vnc_passwd: String,
        webui_bootstrap: Option<String>,
        ports: Vec<PortView>,
        metrics: Option<&'a DockerMetricsOverlay>,
    }

    let ctx = Ctx {
        image: DockerFlavor::SnowLuma.default_image(),
        name: spec.container_name.clone(),
        uid,
        gid,
        vnc_passwd: vnc_secret.snowluma_vnc_passwd(),
        webui_bootstrap: webui_bootstrap.map(|s| s.snowluma_webui_bootstrap()),
        ports: port_views(spec),
        metrics,
    };

    #[allow(clippy::expect_used)]
    engine()
        .render("snowluma.yml", &ctx)
        .expect("snowluma 模板渲染不应失败(上下文由代码完全控制)")
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
        // 没预绑 QQ 时不写 ACCOUNT
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
                // 下一行起每个 env 键应同为 6 空格缩进(services 下 environment 子项)
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
        assert!(yaml.contains(
            "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD: \"${SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD}\""
        ));
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
        // named volume 以容器名为前缀,避免多容器撞卷
        assert!(yaml.contains("snowluma-data:/app/snowluma-data"));
        // Windows checkout 可能把模板变成 CRLF,断言按行比较避免 \n/\r\n 差异
        assert!(
            yaml_has_top_level_named_volume(&yaml, "snowluma-data"),
            "missing top-level named volume snowluma-data in:\n{yaml}"
        );
    }

    #[test]
    fn snowluma_compose_uses_custom_container_name_for_volumes() {
        let mut spec = DockerDeploySpec::snowluma_default();
        spec.container_name = "sl2".to_string();
        let yaml = render_compose(&spec, "p", 0, 0);
        assert!(yaml.contains("container_name: sl2"));
        assert!(yaml.contains("- sl2-data:/app/snowluma-data"));
        assert!(
            yaml_has_top_level_named_volume(&yaml, "sl2-data"),
            "missing top-level named volume sl2-data in:\n{yaml}"
        );
    }

    /// 顶层 `volumes:` 下是否声明了 `name:`(兼容 LF/CRLF)。
    fn yaml_has_top_level_named_volume(yaml: &str, name: &str) -> bool {
        let mut in_top_volumes = false;
        for line in yaml.lines() {
            if line == "volumes:" {
                in_top_volumes = true;
                continue;
            }
            if in_top_volumes {
                if line.starts_with(' ') || line.starts_with('\t') {
                    if line.trim() == format!("{name}:") {
                        return true;
                    }
                } else if !line.trim().is_empty() {
                    // 离开顶层 volumes 块
                    in_top_volumes = false;
                }
            }
        }
        false
    }

    #[test]
    fn napcat_compose_without_metrics_has_no_ncd_env() {
        let spec = DockerDeploySpec::napcat_default();
        let yaml = render_compose_with_env(&spec, "WEBUI_TOKEN", 1000, 1000);
        assert!(!yaml.contains("NCD_METRICS"));
        assert!(!yaml.contains("/ncd-metrics"));
        assert!(!yaml.contains("loadNapCat.js"));
    }

    #[test]
    fn napcat_compose_with_metrics_binds_probe_and_load() {
        let mut spec = DockerDeploySpec::napcat_default();
        spec.qq_id = Some(10001);
        let overlay = DockerMetricsOverlay::for_napcat(
            "/home/u/ncd-watch/metrics",
            "/home/u/.napcat-bots/ncbot-10001/ncd-metrics-load/loadNapCat.js",
            "10001",
            3000,
        );
        let yaml = render_compose_with_metrics(&spec, "WEBUI_TOKEN", 1000, 1000, Some(&overlay));
        assert!(yaml.contains("NCD_METRICS_ENABLED: \"1\""));
        assert!(yaml.contains("/ncd-metrics/10001/net-stats.json"));
        assert!(yaml.contains("/home/u/ncd-watch/metrics:/ncd-metrics"));
        assert!(yaml.contains(
            "/home/u/.napcat-bots/ncbot-10001/ncd-metrics-load/loadNapCat.js:/opt/QQ/resources/app/loadNapCat.js:ro"
        ));
        assert!(!yaml.contains("NODE_OPTIONS"));
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("yaml");
        assert!(parsed.get("services").is_some());
    }

    #[test]
    fn snowluma_compose_with_metrics_sets_node_options() {
        let mut spec = DockerDeploySpec::snowluma_default();
        spec.container_name = "slbot-10001".to_string();
        let overlay =
            DockerMetricsOverlay::for_snowluma("/home/u/ncd-watch/metrics", "10001", 5000);
        let yaml = render_snowluma_compose_with_metrics(
            &spec,
            "VNC_PASSWD",
            "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD",
            1000,
            1000,
            Some(&overlay),
        );
        assert!(yaml.contains("NCD_METRICS_ENABLED: \"1\""));
        assert!(
            yaml.contains("NODE_OPTIONS: \"--require \\\"/ncd-metrics/ncd-ob11-stats.cjs\\\"\"")
                || yaml
                    .contains("NODE_OPTIONS: \"--require \\\"/ncd-metrics/ncd-ob11-stats.cjs\\\"")
                || yaml.contains("--require \\\"/ncd-metrics/ncd-ob11-stats.cjs\\\"")
                || yaml.contains("--require \"/ncd-metrics/ncd-ob11-stats.cjs\"")
        );
        assert!(yaml.contains("/home/u/ncd-watch/metrics:/ncd-metrics"));
        assert!(!yaml.contains("loadNapCat.js"));
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("yaml");
        assert!(parsed.get("services").is_some());
    }
}

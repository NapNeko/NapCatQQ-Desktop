//! Deployment trait:bot 部署形态的统一抽象
//!
//! 一个 Deployment 表达"在某台 Host 上怎么把 bot 跑起来"Host (where) 与
//! Deployment (how) 是正交两层:
//!
//! - Host:LocalWindowsHost / RemoteLinuxHost / 未来 DockerHost / WslHost
//! - Deployment:NativeDeployment / DockerDeployment / ExternalDeployment
//!
//! 两者两两组合,比如 RemoteLinuxHost + DockerDeployment = "用 SSH 在远端跑
//! docker compose"
//! 组合不合法时由 [Deployment::supports] 静态判定,让 UI
//! 在用户连进去之前就能区分能不能选
//!
//! 详见仓库内远端架构重构开发文档

use async_trait::async_trait;
use ncd_domain::{BotConfig, BotFlavor, BotId, StopMode};
use ncd_host::{Host, HostError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 部署形态错误,所有 Deployment 实装的失败模式都收敛到这里
#[derive(Debug, thiserror::Error)]
pub enum DeploymentError {
    /// host trait 调用失败:SSH 中断 / 文件 IO / 命令执行错误等
    #[error("host error: {0}")]
    Host(#[from] HostError),

    /// 这种 deployment 不支持指定 flavor
    /// 例:DockerDeployment 收到 SnowLuma flavor 但暂未做 SL 镜像
    #[error("flavor not supported by deployment: {flavor}")]
    UnsupportedFlavor { flavor: String },

    /// host 上没有需要的运行时(docker daemon / 外部服务等)
    #[error("runtime unavailable on host: {kind}")]
    RuntimeUnavailable { kind: &'static str },

    /// install 阶段失败:下载 / 解压 / 写配置文件等
    #[error("install failed: {0}")]
    InstallFailed(String),

    /// launch 阶段失败:spawn 进程 / docker compose up / endpoint 注册
    #[error("launch failed: {0}")]
    LaunchFailed(String),

    /// stop 阶段失败:进程不肯退 / 容器无法 stop
    #[error("stop failed: {0}")]
    StopFailed(String),

    /// uninstall 阶段失败:清理残留时 IO 错误
    #[error("uninstall failed: {0}")]
    UninstallFailed(String),

    /// 用户给的 BotConfig 不能被这种 deployment 消费
    #[error("config invalid: {0}")]
    ConfigInvalid(String),

    /// 操作不被支持(一般是 stub 实装的占位返回值)
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

/// 部署当前观察到的状态
///
/// 三种形态各自塌缩到这个枚举供 BotActor 状态机消费
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DeploymentState {
    /// 尚未启动
    Stopped,
    /// 启动中
    Starting,
    /// 正在运行,Native:进程活着;Docker:container running;External:endpoint reachable
    Running,
    /// 停止中
    Stopping,
    /// 异常状态,reason 给 UI / 日志展示用
    Failed { reason: String },
}

/// launch 返回的部署句柄,形态特定的 metadata 通过 enum 表达
///
/// 用 enum 而非 trait object:BotActor 状态机层只关心"是否在跑 + PID-like
/// 信息",三种形态都能塌缩到 [crate::result::DeployOutcome] 类似的简单结构
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DeploymentHandle {
    /// 原生进程:宿主机上跑的 NapCat / SL 进程
    Native {
        /// 进程 PID
        pid: u32,
        /// 启动时间 unix epoch 秒
        started_at: u64,
    },
    /// Docker 容器:compose service 内的容器
    Docker {
        /// docker container id(前 12 位短 id)
        container_id: String,
        /// 启动时间 unix epoch 秒
        started_at: u64,
    },
    /// 外接 OneBot endpoint:用户自己的服务
    External {
        /// HTTP/WS 端点 URL
        endpoint: String,
        /// 最近一次 health check 通过时间
        last_seen_at: u64,
    },
}

/// install / launch 时把进度往外吐的 sink
///
/// 设计为 trait object 让 ServerManager 实装能把 sink 包装成
/// event_bus.publish(DomainEvent::DeploymentProgress { ... }),测试时给 mock
/// sink 收集进度断言
pub trait DeploymentProgressSink: Send + Sync {
    /// 报告一次进度,stage 标识当前在哪个阶段("download" / "extract" /
    /// "render-config" / "launch" 等),message 是给用户看的文案,
    /// percent 0-100
    fn report(&self, stage: &str, message: &str, percent: u8);

    /// 一行原始日志,用户看 console 时实时上报
    fn log(&self, line: &str);
}

/// 不上报任何进度的占位 sink,用于测试 / 不关心进度的调用方
pub struct NullProgressSink;

impl DeploymentProgressSink for NullProgressSink {
    fn report(&self, _stage: &str, _message: &str, _percent: u8) {}
    fn log(&self, _line: &str) {}
}

/// 原生进程启动命令——NativeDeployment 真正用得上的字段
///
/// 调用方把 BotConfig 喂给 NativeLaunchTranslator,
/// 拿到一个 NativeLaunchCommand,然后 NativeDeployment 用 host.spawn 起进程
///
/// 字段刻意比 BotRuntimeConfig 简化:丢掉 config_path / log_path / runtime_target
/// 这些 spawn 不直接消费的元数据,只保留"起进程要敲的命令 + 工作目录 + 环境变量"
/// 三件套:
/// - 跟 BotRuntimeConfig 解耦
/// - Docker / External 部署不需要这个结构
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLaunchCommand {
    pub program: String,
    pub args: Vec<String>,
    /// 工作目录为 None 时由 host 决定(一般继承当前进程)
    pub working_dir: Option<std::path::PathBuf>,
    /// 环境变量BTreeMap 保证字典序,方便日志可重现
    pub environment: std::collections::BTreeMap<String, String>,
}

/// NativeLaunchTranslator:把用户配置翻译成原生进程启动命令
///
/// 实装住在调用方那边,ncd-deploy 这边只看到 trait object
/// 这样避免循环依赖,每种 Deployment 形态又能自带自己的翻译能力
#[async_trait]
pub trait NativeLaunchTranslator: Send + Sync {
    /// 把用户配置翻译成原生进程启动命令
    async fn translate(&self, config: &BotConfig) -> Result<NativeLaunchCommand, DeploymentError>;
}

/// 部署形态的统一接口
///
/// 实装注意:
/// - 所有方法应当幂等(重复 install / stop / uninstall 不报错)
/// - install 的进度分阶段上报,让 UI 显示有意义的文案
/// - launch 失败时的错误必须包含足够上下文让用户知道哪步坏了
/// - observe 是高频轮询,必须快返回(< 500ms 量级)
#[async_trait]
pub trait Deployment: Send + Sync {
    /// 部署形态标识固定字符串字面量,用于持久化 / 日志 / UI 展示
    /// 当前合法值:"native" / "docker" / "external"
    fn id(&self) -> &str;

    /// 这种部署形态支持的 bot flavor 列表
    fn supported_flavors(&self) -> &[BotFlavor];

    /// 这种部署形态能否在给定 host 上运行
    ///
    /// 实装应该轻量,不发起 SSH / 网络调用——仅基于 host 的静态信息
    /// (os / arch / locality)判断例如:DockerDeployment 现阶段只支持 Linux
    /// 远端 + Windows 本地(Docker Desktop),其它返回 false
    ///
    /// 真正的"docker daemon 是否运行"留给 install 阶段动态探测
    fn supports(&self, host: &dyn Host) -> bool;

    /// 安装/准备阶段可重入幂等
    ///
    /// - Native: detect 现有版本 → 必要时下载 + 解压 → 渲染配置文件
    /// - Docker: 写 compose.yml → docker pull
    /// - External: HTTP probe endpoint
    async fn install(
        &self,
        host: &dyn Host,
        config: &BotConfig,
        progress: &dyn DeploymentProgressSink,
    ) -> Result<(), DeploymentError>;

    /// 启动 bot,返回的句柄给 BotActor 用来观察状态
    ///
    /// - Native: spawn 进程
    /// - Docker: docker compose up -d <service>
    /// - External: verify endpoint + register 监控
    async fn launch(
        &self,
        host: &dyn Host,
        config: &BotConfig,
    ) -> Result<DeploymentHandle, DeploymentError>;

    /// 观察 bot 当前状态,高频轮询入口
    async fn observe(
        &self,
        host: &dyn Host,
        bot_id: &BotId,
    ) -> Result<DeploymentState, DeploymentError>;

    /// 停止 bot,可重入幂等
    async fn stop(
        &self,
        host: &dyn Host,
        bot_id: &BotId,
        mode: StopMode,
    ) -> Result<(), DeploymentError>;

    /// 完全卸载:清进程树 / 容器 / 配置文件
    /// External 实装应该是 no-op(user 自己管)
    async fn uninstall(&self, host: &dyn Host, config: &BotConfig) -> Result<(), DeploymentError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_state_serialization_uses_kind_tag() {
        let json = serde_json::to_string(&DeploymentState::Running).expect("serialize");
        assert!(json.contains("\"kind\""));
        assert!(json.contains("\"running\""));
    }

    #[test]
    fn deployment_state_failed_carries_reason() {
        let state = DeploymentState::Failed {
            reason: "process died".into(),
        };
        let json = serde_json::to_string(&state).expect("serialize");
        assert!(json.contains("process died"));

        let back: DeploymentState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(state, back);
    }

    #[test]
    fn deployment_handle_native_round_trip() {
        let handle = DeploymentHandle::Native {
            pid: 1234,
            started_at: 1700000000,
        };
        let json = serde_json::to_string(&handle).expect("serialize");
        let back: DeploymentHandle = serde_json::from_str(&json).expect("deserialize");
        match back {
            DeploymentHandle::Native { pid, started_at } => {
                assert_eq!(pid, 1234);
                assert_eq!(started_at, 1700000000);
            }
            _ => panic!("expected Native variant"),
        }
    }

    #[test]
    fn null_progress_sink_is_silent() {
        let sink = NullProgressSink;
        sink.report("install", "hello", 50);
        sink.log("some line");
        // 仅断言不 panic
    }

    #[test]
    fn deployment_error_unsupported_flavor_displays_kind() {
        let err = DeploymentError::UnsupportedFlavor {
            flavor: "snowluma".into(),
        };
        assert_eq!(
            err.to_string(),
            "flavor not supported by deployment: snowluma"
        );
    }

    #[test]
    fn deployment_trait_is_object_safe() {
        // 编译期断言:Deployment 必须能被 Arc<dyn Deployment> 持有,
        // ServerManager 才能注册多种实装
        fn assert_object_safe(_d: &dyn Deployment) {}
        let _ = assert_object_safe;
    }
}

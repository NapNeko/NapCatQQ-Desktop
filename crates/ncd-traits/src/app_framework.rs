//! 应用端框架契约：Integration（每框架一个，纯计算）+ Runtime（编排层实现，骨架共用）。
//!
//! - Deploy 安装/更新/检测走既有 Component × Host × Action（R12），不在此重复定义。
//! - 本 crate 不依赖 ncd-host（host 依赖 traits，反向会成环），所以「把计划写进应用端文件」
//!   这一步由 Layer3 的框架适配器承担；这里只定义不碰 IO 的计划函数。

use async_trait::async_trait;
use ncd_domain::{
    AppConfigIssue, AppFrameworkId, AppFrameworkManifest, AppInstance, AppInstanceId, BotConfig,
    OneBotLinkPlan, StopMode,
};

/// 把协议 Bot 的 OneBot 出口翻译成该应用端的对接计划（差异最大的扩展点）。
///
/// 产品语义: 全自动对接；`plan_link` 是纯函数，预览与应用共用同一份计划，
/// 失败可解释、不静默写坏已有配置由适配器写盘一侧保证。
pub trait AppIntegration: Send + Sync {
    fn framework_id(&self) -> &AppFrameworkId;

    fn manifest(&self) -> &AppFrameworkManifest;

    /// 计算对接计划：协议侧要加的连接 + 应用端侧要改的文件摘要。
    /// `access_token` 由编排层决定（复用应用端已有 / 新生成），保证两侧一致。
    fn plan_link(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        access_token: &str,
    ) -> Result<OneBotLinkPlan, AppFrameworkError>;

    /// 该实例的 WebUI 地址；无 WebUI 返回 None。`public_host` 是从 Desktop 侧可达的主机名
    ///（本机 127.0.0.1，远端为 server 地址）。
    fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String>;
}

/// 应用端运行时最小面：启停 / 状态 / 日志。由编排层（AppManager）实现，框架无关。
/// 不负责 QQ 登录；不替代上游应用端业务 UI。
#[async_trait]
pub trait AppRuntime: Send + Sync {
    async fn start(&self, instance_id: &AppInstanceId) -> Result<AppInstance, AppFrameworkError>;

    async fn stop(
        &self,
        instance_id: &AppInstanceId,
        mode: StopMode,
    ) -> Result<AppInstance, AppFrameworkError>;

    async fn status(&self, instance_id: &AppInstanceId) -> Result<AppInstance, AppFrameworkError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AppFrameworkError {
    #[error("未注册的应用端框架: {0}")]
    NotRegistered(String),

    #[error("应用实例不存在: {0}")]
    InstanceNotFound(String),

    #[error("该部署形态本期未开: {0}")]
    PlacementUnsupported(String),

    #[error("对接拓扑未开: {0}")]
    LinkModeUnsupported(String),

    #[error("{0}")]
    Validation(String),

    #[error("OneBot 出口不可用: {0}")]
    OneBotExport(String),

    #[error("写入应用端配置失败: {0}")]
    Integration(String),

    #[error("应用端运行失败: {0}")]
    Runtime(String),

    #[error("主机不可达: {0}")]
    Host(String),

    #[error("保存失败: {0}")]
    Store(String),

    /// 配置文件在读取后被别处改过（版本号不匹配），UI 应提示重新加载或覆盖
    #[error("配置已被修改（{0}），请重新加载后再保存")]
    ConfigConflict(String),

    /// 配置校验失败；UI 按 `path` 定位到字段
    #[error("配置校验未通过：{}", format_issues(.0))]
    ConfigInvalid(Vec<AppConfigIssue>),

    /// 该框架没有类型化配置（只能走原始文件）
    #[error("该应用端暂不支持类型化配置: {0}")]
    ConfigUnsupported(String),

    #[error("该应用端暂不支持插件代管: {0}")]
    PluginUnsupported(String),

    /// 实例没在跑，运行期 API（人格 / 知识库 / 会话）不可用
    #[error("{0}")]
    NotRunning(String),

    /// Dashboard 登录失败、没记住密码、或上游开了 2FA
    #[error("{0}")]
    DashboardAuth(String),

    /// 本机 loopback / 隧道打不到 WebUI
    #[error("{0}")]
    DashboardUnreachable(String),

    /// 写之前状态变了（启停竞态），禁止改走另一条写路径
    #[error("{0}")]
    StateChanged(String),
}

fn format_issues(issues: &[AppConfigIssue]) -> String {
    issues
        .iter()
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect::<Vec<_>>()
        .join("; ")
}

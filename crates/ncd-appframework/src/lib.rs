//! ncd-appframework：应用端框架适配器集合。
//!
//! 每个框架一个子目录（`karin/`、`nonebot2/`、`astrbot/`、`maibot/`、`koishi/`、`yunzai/`、`neobot/`），各自提供：
//! - `manifest`：静态清单（UI 直接消费）
//! - `component`：实现 `ncd_component::Component`，安装 / 探测 / 启动命令走 Component × Host × Action
//! - `integration`：实现 `ncd_traits::AppIntegration`（纯计划）+ 写应用端配置（备份 → 写 → 失败还原）
//!
//! 编排（实例表、起停、调 BotManager 热推）在 ncd-runtime；这里不碰进程生命周期。
//! 衡量标准：接第二个框架只加一个子目录 + 注册一行。

pub mod adapter;
pub mod adopt;
pub mod astrbot;
pub mod config_backup;
pub mod config_doc;
pub mod env_file;
pub mod karin;
pub mod koishi;
pub mod maibot;
pub mod neobot;
pub mod node_tooling;

pub(crate) fn remote_windows_requirements(
    mut requirements: Vec<ncd_component::Requirement>,
    os: ncd_host::Os,
    locality: ncd_host::Locality,
) -> Vec<ncd_component::Requirement> {
    if os == ncd_host::Os::Windows && locality == ncd_host::Locality::Remote {
        requirements.push(ncd_component::Requirement::component(
            ncd_component::ComponentId::WindowsAgent,
        ));
    }
    requirements
}
pub mod nonebot2;
pub mod ports;
pub mod registry;
pub mod store;
pub mod terminal;
pub mod toml_patch;
pub mod uv_tooling;
pub mod yaml_patch;
pub mod yunzai;

pub use adapter::{
    AppComponentSpec, AppFrameworkAdapter, AppPanelOutcomeKind, AppPanelResult, PluginLogSink,
};
pub use adopt::{
    AdoptRestoreScope, AdoptedFile, capture_adopted_files,
    remove_ncd_debris, restore_adopted_files,
};
pub use astrbot::{
    AstrBotAbconfInfo, AstrBotDashboardStatus, AstrBotKbCreate, AstrBotKnowledgeBase,
    AstrBotPersona, AstrBotRuntimeApi, AstrBotSession, AstrBotSessionRule, join_webui_url,
};
pub use config_doc::{AppConfigWriteResult, AppInstanceConfig, AppInstanceConfigEnvelope};
pub use karin::plugin::{
    KARIN_PLUGINS_LIST_URL, KarinPluginInstalled, KarinPluginMarketEntry, app_file_basename,
};
pub use karin::KarinAdapter;
pub use koishi::{
    KoishiBotState, KoishiBotStatus, KoishiCommandRow,
    KoishiComponent, KoishiDatabaseTable, KoishiFileContent, KoishiFileEntry, KoishiInstanceConfig,
    KoishiPackageInfo, KoishiPluginNode, KoishiPluginSchema, KoishiRuntimeApi,
    KoishiRuntimeGate, KoishiRuntimeStatus, KoishiSandboxMessage,
};
pub use maibot::api::{MaiBotProviderSource, MaiBotRuntimeApi, MaiBotSession};
pub use maibot::resources::behavior::{
    MaiBotBehaviorDetail, MaiBotBehaviorOverview, MaiBotBehaviorPage, MaiBotBehaviorQuery,
};
pub use maibot::resources::chat::MaiBotChatTicket;
pub use maibot::resources::emoji::{
    MaiBotEmojiAction, MaiBotEmojiImage, MaiBotEmojiOverview,
    MaiBotEmojiPage, MaiBotEmojiQuery, MaiBotEmojiUpload,
    MaiBotEmojiUploadDone, MaiBotLocalImage, inspect_local_images,
};
pub use maibot::resources::expression::{
    MaiBotExpressionAction, MaiBotExpressionOverview, MaiBotExpressionPage, MaiBotExpressionQuery,
};
pub use maibot::resources::jargon::{
    MaiBotJargonAction, MaiBotJargonOverview, MaiBotJargonPage, MaiBotJargonQuery,
};
pub use maibot::resources::memory::{
    MaiBotLocalTextFile, MaiBotMemoryDeleteAction, MaiBotMemoryDeleteOp, MaiBotMemoryDeleteResult,
    MaiBotMemoryGraph, MaiBotMemoryGraphHit, MaiBotMemoryImport, MaiBotMemoryImportSetup,
    MaiBotMemoryNodeDetail, MaiBotMemoryQuery, MaiBotMemoryRecordDetail,
    MaiBotMemoryRecordKind, MaiBotMemoryRecordPage, MaiBotMemorySource,
    MaiBotMemoryStatus, MaiBotMemoryTask, MaiBotMemoryTaskAction, MaiBotMemoryTaskDetail,
    inspect_local_texts,
};
pub use maibot::resources::person::{
    MaiBotPersonAction, MaiBotPersonOverview, MaiBotPersonPage, MaiBotPersonQuery,
};
pub use maibot::resources::prompts::{
    MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile, MaiBotPromptTarget,
};
pub use maibot::resources::MaiBotResourceDone;
pub use maibot::runtime::{
    MaiBotChatSession, MaiBotMcpStatus, MaiBotMcpTest,
    MaiBotProviderCheck, MaiBotProviderModel, MaiBotRuntimeGate, MaiBotRuntimeStatus,
    MaiBotStatsSummary,
};
pub use maibot::schema::{MaiBotAPIProvider, MaiBotMCPServerItemConfig};
pub use maibot::MaiBotAdapterConfig;
pub use neobot::PackageVersions;
pub use nonebot2::{
    NONEBOT_ADAPTERS_URL, NONEBOT_PLUGINS_URL, NoneBot2Adapter, nonebot_registry_urls,
};
pub use ports::remote_listening_ports;
pub use registry::AppFrameworkRegistry;
pub use store::{
    AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry, StoreMarketPart, StoreMarketText,
};
pub use terminal::AppTerminalProfile;

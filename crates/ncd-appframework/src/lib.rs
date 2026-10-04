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
pub mod config_doc;
pub mod env_file;
pub mod karin;
pub mod koishi;
pub mod maibot;
pub mod neobot;
pub mod node_tooling;
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
    AppComponentSpec, AppFrameworkAdapter, PluginLogSink, apply_with_backup, apply_with_backup_ex,
    restore_from_backup,
};
pub use adopt::{
    AdoptRestoreScope, AdoptedFile, capture_adopted_files, list_dotenv_rels, merge_rels,
    remove_ncd_debris, restore_adopted_files, write_project_sidecar,
};
pub use astrbot::{
    ASTRBOT_FRAMEWORK_ID, ASTRBOT_PLUGINS_URL, AstrBotAbconfInfo, AstrBotAdapter,
    AstrBotAiSettings, AstrBotComponent, AstrBotDashboardGate, AstrBotDashboardStatus,
    AstrBotInstanceConfig, AstrBotIntegration, AstrBotKbBind, AstrBotKbCreate,
    AstrBotKnowledgeBase, AstrBotOneBotRow, AstrBotPersona, AstrBotPlatformGates,
    AstrBotProviderModel, AstrBotProviderSource, AstrBotRuntimeApi, AstrBotSession,
    AstrBotSessionRule, AstrBotSttSettings, AstrBotSubagentConfig, AstrBotSubagentRow,
    AstrBotTtsSettings, AstrBotWebSearchSettings, astrbot_manifest, astrbot_plugin_market_urls,
    join_webui_url, parse_astrbot_plugins_json,
};
pub use config_doc::{
    AppConfigDocumentRevision, AppConfigWriteResult, AppInstanceConfig, AppInstanceConfigEnvelope,
    DocumentSnapshot, MISSING_REVISION, combined_revision, revision_of,
};
pub use env_file::{EnvEntry, EnvFile, EnvWrite};
pub use karin::config::{KarinEnv, KarinInstanceConfig};
pub use karin::plugin::{
    KARIN_PLUGINS_LIST_URL, KarinPluginAppFile, KarinPluginAuthor, KarinPluginInstalled,
    KarinPluginKind, KarinPluginMarketEntry, KarinPluginRepo, app_file_basename,
    apply_plugin_enabled, confirm_plugin_on_disk, git_clone_url, parse_karin_plugins_list,
    write_app_file_bytes,
};
pub use karin::{
    KARIN_FRAMEWORK_ID, KarinAdapter, KarinComponent, KarinIntegration, karin_manifest,
};
pub use koishi::{
    KOISHI_FRAMEWORK_ID, KoishiAdapter, KoishiBotState, KoishiBotStatus, KoishiCommandRow,
    KoishiComponent, KoishiDatabaseTable, KoishiFileContent, KoishiFileEntry, KoishiInstanceConfig,
    KoishiIntegration, KoishiPackageInfo, KoishiPluginNode, KoishiPluginSchema, KoishiRuntimeApi,
    KoishiRuntimeGate, KoishiRuntimeStatus, KoishiSandboxMessage, koishi_manifest,
};
pub use maibot::api::{MaiBotProviderSource, MaiBotRuntimeApi, MaiBotSession};
pub use maibot::resources::behavior::{
    MaiBotBehavior, MaiBotBehaviorActor, MaiBotBehaviorChat, MaiBotBehaviorDetail,
    MaiBotBehaviorEvidence, MaiBotBehaviorFeedback, MaiBotBehaviorFeedbackKind,
    MaiBotBehaviorFilter, MaiBotBehaviorOrigin, MaiBotBehaviorOverview, MaiBotBehaviorPage,
    MaiBotBehaviorQuery, MaiBotBehaviorSort, MaiBotBehaviorTag, MaiBotBehaviorTagKind,
};
pub use maibot::resources::chat::MaiBotChatTicket;
pub use maibot::resources::emoji::{
    MaiBotEmoji, MaiBotEmojiAction, MaiBotEmojiFilter, MaiBotEmojiImage, MaiBotEmojiOverview,
    MaiBotEmojiPage, MaiBotEmojiQuery, MaiBotEmojiSort, MaiBotEmojiStatus, MaiBotEmojiUpload,
    MaiBotEmojiUploadDone, MaiBotLocalImage, MaiBotUploadFailure, inspect_local_images,
};
pub use maibot::resources::expression::{
    MaiBotExpression, MaiBotExpressionAction, MaiBotExpressionFilter, MaiBotExpressionOverview,
    MaiBotExpressionPage, MaiBotExpressionQuery,
};
pub use maibot::resources::jargon::{
    MaiBotJargon, MaiBotJargonAction, MaiBotJargonFilter, MaiBotJargonOverview, MaiBotJargonPage,
    MaiBotJargonQuery,
};
pub use maibot::resources::memory::{
    MaiBotLocalTextFile, MaiBotMemoryCounts, MaiBotMemoryDeleteAction, MaiBotMemoryDeleteKind,
    MaiBotMemoryDeleteOp, MaiBotMemoryDeleteResult, MaiBotMemoryDeleteSample,
    MaiBotMemoryDeleteTarget, MaiBotMemoryGraph, MaiBotMemoryGraphEdge, MaiBotMemoryGraphHit,
    MaiBotMemoryGraphNode, MaiBotMemoryGraphParagraph, MaiBotMemoryGraphRelation,
    MaiBotMemoryImport, MaiBotMemoryImportKind, MaiBotMemoryImportLimits,
    MaiBotMemoryImportOptions, MaiBotMemoryImportSetup, MaiBotMemoryKindCounts,
    MaiBotMemoryNodeDetail, MaiBotMemoryQuery, MaiBotMemoryRecord, MaiBotMemoryRecordDetail,
    MaiBotMemoryRecordKind, MaiBotMemoryRecordPage, MaiBotMemorySource, MaiBotMemoryState,
    MaiBotMemoryStatus, MaiBotMemoryTask, MaiBotMemoryTaskAction, MaiBotMemoryTaskDetail,
    MaiBotMemoryTaskFile, MaiBotMemoryTaskStatus, inspect_local_texts,
};
pub use maibot::resources::person::{
    MaiBotGroupCard, MaiBotPerson, MaiBotPersonAction, MaiBotPersonFilter, MaiBotPersonOverview,
    MaiBotPersonPage, MaiBotPersonQuery,
};
pub use maibot::resources::prompts::{
    MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile, MaiBotPromptInfo,
    MaiBotPromptLanguage, MaiBotPromptTarget, MaiBotPromptVersion,
};
pub use maibot::resources::{MaiBotLearningChat, MaiBotResourceDone};
pub use maibot::runtime::{
    MaiBotChatSession, MaiBotMcpServerStatus, MaiBotMcpStatus, MaiBotMcpTest, MaiBotMcpTool,
    MaiBotProviderCheck, MaiBotProviderModel, MaiBotRuntimeGate, MaiBotRuntimeStatus,
    MaiBotStatsSummary,
};
pub use maibot::schema::{MaiBotAPIProvider, MaiBotMCPServerItemConfig};
pub use maibot::{
    MAIBOT_FRAMEWORK_ID, MaiBotAdapter, MaiBotAdapterConfig, MaiBotChatFilter, MaiBotComponent,
    MaiBotInstanceConfig, MaiBotIntegration, MaiBotListMode, maibot_manifest,
};
pub use neobot::{
    NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT, NEOBOT_FRAMEWORK_ID,
    NeoBotComponent, PackageVersions, neobot_manifest,
};
pub use nonebot2::{
    NONEBOT_ADAPTERS_URL, NONEBOT_PLUGINS_URL, NONEBOT2_FRAMEWORK_ID, NoneBot2Adapter,
    NoneBot2Component, NoneBot2EnvEntry, NoneBot2EnvProd, NoneBot2InstanceConfig,
    NoneBot2Integration, nonebot_registry_urls, nonebot2_manifest, parse_nonebot_adapters_json,
    parse_nonebot_plugins_json,
};
pub use ports::{PortUsage, local_port_free, parse_proc_net_listen, remote_listening_ports};
pub use registry::AppFrameworkRegistry;
pub use store::{
    AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry, StoreMarketPart, StoreMarketText,
};
pub use terminal::AppTerminalProfile;
pub use yunzai::{
    YUNZAI_FRAMEWORK_ID, YunzaiAdapter, YunzaiComponent, YunzaiInstanceConfig, YunzaiIntegration,
    yunzai_manifest,
};

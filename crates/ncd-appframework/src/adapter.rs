//! 框架适配器契约（Layer3）：把 Component / Integration / 应用端文件写入捏在一起。
//!
//! `ncd_traits::AppIntegration` 是纯计划；碰 Host 的部分（读 token / 写配置 / 回滚）
//! 放这里，因为 ncd-traits 不能依赖 ncd-host。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppPendingTerms, AppProjectProbe,
    OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

/// 为某个应用实例构造 Component 所需的输入（编排层填）
#[derive(Debug, Clone)]
pub struct AppComponentSpec {
    /// 实例目录（HostPath）
    pub install_dir: HostPath,
    /// 实例监听端口（安装时写进应用端配置）
    pub port: u16,
    /// 桌面端管理的 Node 二进制（Node 系框架用）；None 则只看实例标记 / PATH
    pub node_bin: Option<HostPath>,
    /// 桌面端管理的 uv 二进制（Python 系框架用）；None 则只看实例标记 / PATH
    pub uv_bin: Option<HostPath>,
    /// 桌面端管理的 git（只有 Windows 有托管落点，Linux 走系统包）；可能还没装，用前核对
    pub git_bin: Option<HostPath>,
    /// 桌面端管理的 redis-server（云崽写进 redis.yaml 的 path）；可能还没装，用前核对
    pub redis_bin: Option<HostPath>,
    /// npm registry 镜像；None 用默认源
    pub npm_registry: Option<String>,
    /// PyPI 索引镜像（`uv pip install --default-index`）；None 用默认源。
    /// 各框架按自己的包管理器解释：Python 系走 pip index，Node 系忽略。
    pub pypi_index: Option<String>,
    /// 指定安装的版本；None = 装该包的最新正式版。
    /// 各框架按自己的生态解释（Python 系 `pkg==ver`、Node 系 `pkg@ver`）；
    /// 不支持指定版本的框架忽略它。装完的真实版本由 `detect` 回读，不在这里记账。
    pub install_version: Option<String>,
    /// Karin：provision 时一并 `pnpm add @karinjs/plugin-puppeteer`。NoneBot2 忽略。
    pub install_renderer: bool,
    /// 领养已有项目：同步依赖，不写脚手架、不改端口。
    pub adopt_existing: bool,
    /// 实例 id；AstrBot 预置 `ncd-app:<id>` 行时用。图占位可填 `"x"`。
    pub instance_id: String,
    /// 用户名密码类 WebUI 首启前要种进配置的账号（明文只在内存里过一次）；None = 不碰
    pub webui_username: Option<String>,
    pub webui_password: Option<String>,
}

/// 用户名密码类 WebUI 落盘账号 + 与桌面端记住的密码是否一致
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebUiAccountProbe {
    pub username: String,
    /// None = 落盘还没有哈希（首启才生成）或没给要核对的密码
    pub password_matches: Option<bool>,
    /// 落盘存储的密码是否为哈希格式（用于判断能否代登录）
    pub stored_is_hash: bool,
}

#[async_trait]
pub trait AppFrameworkAdapter: Send + Sync {
    fn manifest(&self) -> &AppFrameworkManifest;

    /// 纯计划侧
    fn integration(&self) -> &dyn AppIntegration;

    /// 安装 / 探测 / 启动命令（走既有 Component × Host × Action，不另起一套安装流程）
    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component>;

    /// 该框架支持的**可安装版本清单**（供「装任意版本」的选择器）。
    ///
    /// 默认 `Ok(None)`：不支持按版本安装的框架（Node 系、整包发行）不必实现，
    /// UI 拿到 None 就不显示版本选择器。Python 系框架实现它去查 PyPI。
    async fn available_versions(
        &self,
    ) -> Result<Option<crate::neobot::versions::PackageVersions>, AppFrameworkError> {
        Ok(None)
    }

    /// 丢掉为这个实例缓存的面板会话。面板密码换了或清了之后调用：
    /// 不丢的话旧 token 还能用，「保存并验证」会对一个错密码报成功
    fn forget_panel_session(&self, _instance_id: &str) {}

    /// 探测已有目录能不能当这个框架的实例领养
    async fn probe_project(
        &self,
        _host: &dyn Host,
        _path: &HostPath,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        Err(AppFrameworkError::Validation(format!(
            "{} 暂不支持导入已有项目",
            self.manifest().display_name
        )))
    }

    /// 导入时写入 `AppInstance.port` 的口。默认：探测口，否则框架默认口。
    /// 探测不到真实口时不要猜默认口——多监听口会把默认口误认成认领。
    fn suggested_import_port(&self, probe: &AppProjectProbe) -> Option<u16> {
        probe
            .port
            .filter(|p| *p > 0)
            .or(Some(self.manifest().default_port))
    }

    /// `read_config` 失败时打开 WebUI 用的回落口。默认等于 `instance.port`。
    fn webui_fallback_port(&self, instance: &AppInstance) -> u16 {
        instance.port
    }

    /// 只读取打开 WebUI 必需的端口和登录密钥，不解析框架业务配置。
    async fn read_webui_endpoint(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(u16, String), AppFrameworkError> {
        Ok((self.webui_fallback_port(instance), String::new()))
    }

    /// 用户名密码类 WebUI：读落盘账号，顺带核对桌面端记住的密码。None = 该框架不是账号密码登录。
    async fn read_webui_account(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
        _remembered_password: Option<&str>,
    ) -> Result<Option<WebUiAccountProbe>, AppFrameworkError> {
        Ok(None)
    }

    /// 落盘读不到账号、桌面端也没记用户名时登录先用的用户名，即框架首启自己写的那个；空 = 没有约定
    fn default_webui_username(&self) -> &str {
        ""
    }

    /// 把新密码按框架哈希格式写进实例配置。调用方保证实例已停止（运行中改会被进程覆盖）。
    async fn write_webui_password(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _password: &str,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::ConfigUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    /// 框架自己的口令策略；新建 / 重置时用户给的密码先过这里
    fn validate_webui_password(&self, _password: &str) -> Result<(), String> {
        Ok(())
    }

    fn generate_webui_password(&self) -> String {
        use rand::Rng;
        rand::thread_rng()
            .sample_iter(rand::distributions::Alphanumeric)
            .take(24)
            .map(char::from)
            .collect()
    }

    /// manifest 声明了上游条款的框架：实例目录里还没同意、或更新后改过的条款（带原文）。
    /// 非空时编排层拒绝启动，前端先弹同意框
    async fn pending_terms(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<Vec<AppPendingTerms>, AppFrameworkError> {
        Ok(Vec::new())
    }

    /// 用户在同意框里点了同意：把实例目录里现在这版条款记成已同意
    async fn accept_terms(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// 导入前要快照的相对路径（dotenv / 入口 / 清单）。缺文件也列入，释放时删掉我们后来写的。
    async fn adopt_watch_rels(
        &self,
        host: &dyn Host,
        root: &HostPath,
    ) -> Result<Vec<String>, AppFrameworkError> {
        crate::adopt::list_dotenv_rels(host, root).await
    }

    /// 停实例前先请应用自己退（存盘、收它拉起的子进程）。Ok(true) = 请求已送达，编排层接着等
    /// 进程退出，等不到再收整棵树；Ok(false) = 这个框架没有这种入口，直接收树
    ///
    /// `panel_password` 是用户在桌面端填过的**面板密码**（密钥库里的那个，可能为 None）。
    /// 走 HTTP 退出入口的框架（NeoBot）需要它：面板接口要登录，回环来源并不能绕过鉴权。
    async fn request_graceful_stop(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
        _panel_password: Option<&str>,
    ) -> Result<bool, AppFrameworkError> {
        Ok(false)
    }

    /// 送达后最多等多久
    fn graceful_stop_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(15)
    }

    /// 启动命令（可做 IO 解析工具链，比 `Component::launch_command` 的同步版准确）
    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError>;

    /// 上游支持「桌面托管会话」免密登录（AstrBot v4.28+ 的 desktop-session）：返回
    /// （托管开关 env 键, 会话密钥 env 键），编排层启动实例时注入，代登录优先走它。
    /// None = 没这套机制，代登录只能用密码
    fn desktop_session_env_keys(&self) -> Option<(&'static str, &'static str)> {
        None
    }

    /// 终端开在实例目录时接上的环境（PATH 前缀、变量、提示、常用命令）。默认只进目录
    async fn terminal_profile(
        &self,
        _host: &dyn Host,
        _spec: &AppComponentSpec,
    ) -> crate::terminal::AppTerminalProfile {
        crate::terminal::AppTerminalProfile::default()
    }

    /// 应用端已配置的 OneBot 反向 WS token；没有或为空返回 None
    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError>;

    /// 应用作为客户端连协议 Bot 的 WS 地址 (NoneBot ONEBOT_WS_URLS / Karin onebot.ws_client)
    async fn read_outbound_ws_urls(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        Ok(Vec::new())
    }

    /// 把计划写进应用端：先备份 `<file>.ncd.bak`，任一步失败自动还原
    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError>;

    /// 解绑：应用端侧默认不动监听口（Bot 侧删连接即可）；有需要的框架覆写
    async fn unlink(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// `apply_link` 成功但 Bot 侧写失败时，把应用端文件从 `.ncd.bak` 还原
    async fn rollback_link(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// 实例运行日志文件（远端 tail 用）；None 表示只有进程 stdout
    fn log_file(&self, instance: &AppInstance) -> Option<HostPath>;

    /// 可编辑、随实例迁移和备份的配置文档。
    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        Vec::new()
    }

    /// 能碰 Host 时按真实文件列出（NoneBot 的 `.env.{ENVIRONMENT}`）
    async fn list_config_documents(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        Ok(self.config_documents(instance))
    }

    /// 框架会回写原文时，拒绝与运行中进程争写。
    fn validate_config_file_write(
        &self,
        _instance: &AppInstance,
        _doc_id: &str,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// 跑着的实例要经 WebUI 口（控制台）写对接配置的框架：编排层在对接之前
    /// 先把桌面端这边的回环口（远端是隧道口）告诉适配器
    fn wants_live_port(&self) -> bool {
        false
    }

    fn note_live_port(&self, _instance_id: &str, _port: u16) {}
}

/// 「备份 → 写 → 失败还原」骨架：`write` 返回 Err 时把 `files` 全部还原到备份内容。
/// 脚手架实例还会落 `<file>.ncd.bak`；导入项目只做内存回滚，避免弄脏用户仓库。
pub async fn apply_with_backup<F, Fut>(
    host: &dyn Host,
    files: &[HostPath],
    write: F,
) -> Result<(), AppFrameworkError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppFrameworkError>>,
{
    apply_with_backup_ex(host, files, true, write).await
}

pub async fn apply_with_backup_ex<F, Fut>(
    host: &dyn Host,
    files: &[HostPath],
    write_sidecar: bool,
    write: F,
) -> Result<(), AppFrameworkError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppFrameworkError>>,
{
    let mut originals: Vec<(HostPath, Option<Vec<u8>>)> = Vec::with_capacity(files.len());
    for file in files {
        let existed = host
            .exists(file)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        let content = if existed {
            let bytes = host
                .read_file(file)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
            Some(bytes.to_vec())
        } else {
            None
        };
        originals.push((file.clone(), content));
    }

    apply_with_backup_snapshots(host, &originals, write_sidecar, write).await
}

/// 复用校验过的快照，让备份与回滚保留同一份原文。
pub(crate) async fn apply_with_backup_snapshots<F, Fut>(
    host: &dyn Host,
    originals: &[(HostPath, Option<Vec<u8>>)],
    write_sidecar: bool,
    write: F,
) -> Result<(), AppFrameworkError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppFrameworkError>>,
{
    if write_sidecar {
        for (file, content) in originals {
            if let Some(bytes) = content {
                host.write_file(&backup_path(file), bytes)
                    .await
                    .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
            }
        }
    }

    match write().await {
        Ok(()) => Ok(()),
        Err(err) => {
            for (file, original) in originals {
                let restore = match original {
                    Some(bytes) => host.write_file(file, bytes).await,
                    None => match host.exists(file).await {
                        Ok(true) => host.remove_file(file).await,
                        _ => Ok(()),
                    },
                };
                if let Err(e) = restore {
                    tracing::error!(
                        file = %file.as_posix(),
                        error = %e,
                        "restore app-side config after failed link"
                    );
                }
            }
            Err(err)
        }
    }
}

pub fn backup_path(file: &HostPath) -> HostPath {
    HostPath::from_posix(format!("{}.ncd.bak", file.as_posix()))
}

/// 把 `files` 各自从 `<file>.ncd.bak` 还原（没有备份的跳过）
pub async fn restore_from_backup(
    host: &dyn Host,
    files: &[HostPath],
) -> Result<(), AppFrameworkError> {
    for file in files {
        let backup = backup_path(file);
        let has_backup = host
            .exists(&backup)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if !has_backup {
            continue;
        }
        let bytes = host
            .read_file(&backup)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        host.write_file(file, &bytes)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    }
    Ok(())
}

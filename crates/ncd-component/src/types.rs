//! Component / Action 共享数据类型

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Component 标识
///
/// 跨边界时各 variant 的字面量(serde / ts-rs)锁定为:
/// - NapCat → napcat
/// - SnowLuma → snowluma
/// - Qq → qq
/// - NodeJs → nodejs
/// - NoVnc → novnc
/// - DesktopSelf → desktop_self
/// - NcdWatch → ncd_watch
/// - Karin → karin（应用端框架；按实例目录安装，不进组件页 catalog）
/// - Uv → uv（Python 工具链，单二进制，可自带装 Python；NoneBot2 的运行时依赖）
/// - NoneBot2 → nonebot2（应用端框架；同 Karin 按实例目录安装）
/// - AstrBot → astrbot（应用端框架；按实例目录安装，不进组件页 catalog）
///
/// 与项目内 napcat_* / snowluma_* 事件名风格保持一致;不直接走 serde
/// 的 rename_all = "snake_case",因为它会把 NapCat 切成 nap_cat,
/// Qq 切成 qq 也算巧合,但 NapCat 不行,所以统一都用显式 rename
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ComponentId {
    #[serde(rename = "napcat")]
    NapCat,
    #[serde(rename = "snowluma")]
    SnowLuma,
    #[serde(rename = "qq")]
    Qq,
    #[serde(rename = "nodejs")]
    NodeJs,
    #[serde(rename = "novnc")]
    NoVnc,
    #[serde(rename = "desktop_self")]
    DesktopSelf,
    #[serde(rename = "ncd_watch")]
    NcdWatch,
    #[serde(rename = "karin")]
    Karin,
    #[serde(rename = "uv")]
    Uv,
    #[serde(rename = "nonebot2")]
    NoneBot2,
    #[serde(rename = "astrbot")]
    AstrBot,
}

impl ComponentId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NapCat => "napcat",
            Self::SnowLuma => "snowluma",
            Self::Qq => "qq",
            Self::NodeJs => "nodejs",
            Self::NoVnc => "novnc",
            Self::DesktopSelf => "desktop_self",
            Self::NcdWatch => "ncd_watch",
            Self::Karin => "karin",
            Self::Uv => "uv",
            Self::NoneBot2 => "nonebot2",
            Self::AstrBot => "astrbot",
        }
    }

    /// 应用端框架：按实例目录装，不进组件页 catalog。
    /// 新框架加变体时必须写进这里，factory / 依赖图靠它分流，不再点名。
    pub const fn is_app_framework(&self) -> bool {
        matches!(self, Self::Karin | Self::NoneBot2 | Self::AstrBot)
    }

    /// 从跨边界字面量还原（与 serde rename 同源）；未知返回 None
    pub fn parse(value: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
    }
}

/// 探测结果(Component::detect 返回值)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DetectedVersion {
    /// 探测到的版本号(如 "v20.10.0" / "3.2.25-45758")
    pub version: String,
    /// 探测来源(如 "package.json" / "qq --version" / "node -v")
    pub source: String,
}

/// 找到了安装但不能用:版本不满足要求,或二进制在却跑不起来
///
/// 与「未安装」分开表达,否则用户装过 Node 也会看到同一个「未安装」徽章;
/// 但 detect() 对此仍返回 None,依赖判断 / ensure_installed 不把它当已装
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct UnusableInstall {
    /// 定位到的二进制 / 目录(与 DetectedVersion.source 同口径)
    pub source: String,
    /// 探测到的版本;跑不起来时为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 为什么不能用,直接给 UI 显示
    pub reason: String,
}

/// Component::detect_outcome 返回值:比 detect 多一档「找到但不可用」
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectOutcome {
    NotInstalled,
    Installed(DetectedVersion),
    Unusable(UnusableInstall),
}

impl DetectOutcome {
    /// 折回 detect() 的二值语义:只有 Installed 算装上
    pub fn into_installed(self) -> Option<DetectedVersion> {
        match self {
            Self::Installed(v) => Some(v),
            Self::NotInstalled | Self::Unusable(_) => None,
        }
    }

    pub fn from_detected(detected: Option<DetectedVersion>) -> Self {
        detected.map_or(Self::NotInstalled, Self::Installed)
    }
}

/// 校验报告(Component::verify 返回值)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub ok: bool,
    /// 已验证项(如 "binary exists" / "sha256 matches" / "manifest version")
    pub checks: Vec<VerifyCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyCheck {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
}

impl VerifyReport {
    pub fn ok() -> Self {
        Self {
            ok: true,
            checks: Vec::new(),
        }
    }

    pub fn with_check(
        mut self,
        name: impl Into<String>,
        passed: bool,
        detail: Option<String>,
    ) -> Self {
        self.checks.push(VerifyCheck {
            name: name.into(),
            passed,
            detail,
        });
        if !passed {
            self.ok = false;
        }
        self
    }
}

/// 启动参数(Component::launch_command 入参)
#[derive(Debug, Clone, Default)]
pub struct LaunchArgs {
    /// 额外环境变量
    pub extra_env: Vec<(String, String)>,
    /// 额外命令行参数
    pub extra_args: Vec<String>,
    /// 工作目录(若 None,由 Component 决定默认值)
    pub working_dir: Option<ncd_host::HostPath>,
}

impl LaunchArgs {
    // 把 extra_args / extra_env / working_dir 追加到 cmd,各 component 的
    // launch_command 复用,避免 6 处抄同一片 for 循环
    pub fn apply_to(&self, mut cmd: ncd_host::HostCommand) -> ncd_host::HostCommand {
        for a in &self.extra_args {
            cmd = cmd.arg(a);
        }
        for (k, v) in &self.extra_env {
            cmd = cmd.env(k, v);
        }
        if let Some(wd) = &self.working_dir {
            cmd = cmd.working_dir(wd.clone());
        }
        cmd
    }
}

/// 组件分类
///
/// - Framework:用户主动选择安装的协议框架(NapCat / SnowLuma)
/// - RuntimeDep:Framework 依赖的运行时(QQ / NodeJs / NoVnc)
/// - SelfApp:Desktop 产品侧(本机 Desktop 自更新;远端 ncd-watch 脱管监控)
/// - AppFramework:应用端框架(Karin / NoneBot2 / AstrBot),消费 OneBot,按实例安装
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ComponentCategory {
    Framework,
    RuntimeDep,
    SelfApp,
    AppFramework,
}

/// (Os, Locality) 组合的强类型表达
///
/// Component::supported_targets 暴露的是 &'static [(Os, Locality)],跨边界
/// 时拍扁成本结构以保留字段名(前端按 os / locality 字段访问)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct SupportedTarget {
    pub os: ncd_host::Os,
    pub locality: ncd_host::Locality,
}

impl SupportedTarget {
    pub const fn new(os: ncd_host::Os, locality: ncd_host::Locality) -> Self {
        Self { os, locality }
    }
}

impl From<(ncd_host::Os, ncd_host::Locality)> for SupportedTarget {
    fn from((os, locality): (ncd_host::Os, ncd_host::Locality)) -> Self {
        Self { os, locality }
    }
}

/// 组件元数据,Components 页直接消费的清单数据
///
/// 字段都由各 Component 实装的 info() 静态方法写死;前端不做任何派生
/// (比如 i18n 文案就由后端写死中文 + 简短描述)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ComponentInfo {
    pub id: ComponentId,
    /// UI 显示名("NapCat" / "SnowLuma" / "Node.js" / "QQ" 等)
    pub display_name: String,
    /// 一行简介,2-30 字
    pub description: String,
    /// GitHub / 官网链接(None 表示无对应外链)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    /// 支持的 (Os, Locality) 组合,前端用来判断"在某主机上能不能装"
    pub supported_targets: Vec<SupportedTarget>,
    /// 分类
    pub category: ComponentCategory,
}

/// 1 个 component 在 1 台 host 上的探测结果
///
/// detect_component Tauri command 出参;前端按字段渲染"是否已装 / 哪个
/// 版本 / 该 host 是否支持本 component"任一字段缺失都不影响其它字段
/// 的解释(比如 supported=false 时 detected 必为 None,但前端仍可
/// 显示 host_id)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ComponentDetectResult {
    pub component_id: ComponentId,
    pub host_id: String,
    /// None 表示未安装;Some 表示已装
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected: Option<DetectedVersion>,
    /// detected 为 None 时可能有值:找到了但不可用(版本不符 / 无法执行)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unusable: Option<UnusableInstall>,
    /// 当前 host 是否在 component 的 supported_targets 中;不支持时
    /// detected 始终为 None
    pub supported: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_id_as_str_matches_snake_case() {
        assert_eq!(ComponentId::NapCat.as_str(), "napcat");
        assert_eq!(ComponentId::SnowLuma.as_str(), "snowluma");
        assert_eq!(ComponentId::Qq.as_str(), "qq");
    }

    #[test]
    fn component_id_serializes_snake_case() {
        let s = serde_json::to_string(&ComponentId::DesktopSelf).unwrap();
        assert_eq!(s, "\"desktop_self\"");
    }

    /// 锁定每个 ComponentId variant 的 wire 字面量与 as_str() 一致;
    /// 同时锁定 round-trip 等价任何 typo(包括误用 serde 默认 snake_case
    /// 把 NapCat 切成 nap_cat)都会让此测试失败
    #[test]
    fn component_id_parse_round_trips_as_str() {
        for id in [
            ComponentId::NapCat,
            ComponentId::SnowLuma,
            ComponentId::Qq,
            ComponentId::NodeJs,
            ComponentId::NoVnc,
            ComponentId::DesktopSelf,
            ComponentId::NcdWatch,
            ComponentId::Karin,
            ComponentId::Uv,
            ComponentId::NoneBot2,
            ComponentId::AstrBot,
        ] {
            assert_eq!(ComponentId::parse(id.as_str()), Some(id));
        }
        assert_eq!(ComponentId::parse("nope"), None);
    }

    #[test]
    fn app_framework_flag_marks_only_frameworks() {
        assert!(ComponentId::Karin.is_app_framework());
        assert!(ComponentId::NoneBot2.is_app_framework());
        assert!(ComponentId::AstrBot.is_app_framework());
        assert!(!ComponentId::Uv.is_app_framework());
        assert!(!ComponentId::NodeJs.is_app_framework());
    }

    #[test]
    fn component_id_serde_aligns_with_as_str() {
        for id in [
            ComponentId::NapCat,
            ComponentId::SnowLuma,
            ComponentId::Qq,
            ComponentId::NodeJs,
            ComponentId::NoVnc,
            ComponentId::DesktopSelf,
            ComponentId::NcdWatch,
            ComponentId::Karin,
            ComponentId::Uv,
            ComponentId::NoneBot2,
            ComponentId::AstrBot,
        ] {
            let s = serde_json::to_string(&id).unwrap();
            let expected = format!("\"{}\"", id.as_str());
            assert_eq!(s, expected);
            let decoded: ComponentId = serde_json::from_str(&s).unwrap();
            assert_eq!(decoded, id);
        }
    }

    #[test]
    fn verify_report_ok_starts_empty() {
        let r = VerifyReport::ok();
        assert!(r.ok);
        assert!(r.checks.is_empty());
    }

    #[test]
    fn verify_report_failed_check_flips_ok() {
        let r = VerifyReport::ok().with_check("a", true, None).with_check(
            "b",
            false,
            Some("missing".into()),
        );
        assert!(!r.ok);
        assert_eq!(r.checks.len(), 2);
    }

    /// ComponentInfo 字面量字节级 round-trip:锁定前后端契约
    /// 任何字段重命名 / 顺序变更都会让此测试失败
    #[test]
    fn component_info_round_trips() {
        let info = ComponentInfo {
            id: ComponentId::NapCat,
            display_name: "NapCat".to_string(),
            description: "无 QQ 窗口的协议端".to_string(),
            repo_url: Some("https://github.com/NapNeko/NapCatQQ".to_string()),
            supported_targets: vec![
                SupportedTarget::new(ncd_host::Os::Windows, ncd_host::Locality::Local),
                SupportedTarget::new(ncd_host::Os::Linux, ncd_host::Locality::Remote),
            ],
            category: ComponentCategory::Framework,
        };
        let json = serde_json::to_string(&info).expect("serialize ComponentInfo");
        let decoded: ComponentInfo =
            serde_json::from_str(&json).expect("deserialize ComponentInfo");
        assert_eq!(decoded, info);
    }

    #[test]
    fn component_category_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ComponentCategory::RuntimeDep).unwrap(),
            "\"runtime_dep\""
        );
        assert_eq!(
            serde_json::to_string(&ComponentCategory::SelfApp).unwrap(),
            "\"self_app\""
        );
        assert_eq!(
            serde_json::to_string(&ComponentCategory::Framework).unwrap(),
            "\"framework\""
        );
    }

    #[test]
    fn detect_outcome_only_installed_counts_as_detected() {
        let v = DetectedVersion {
            version: "22.13.0".into(),
            source: "/opt/node/bin/node".into(),
        };
        assert_eq!(
            DetectOutcome::Installed(v.clone()).into_installed(),
            Some(v.clone())
        );
        assert_eq!(DetectOutcome::NotInstalled.into_installed(), None);
        assert_eq!(
            DetectOutcome::Unusable(UnusableInstall {
                source: "$PATH/node".into(),
                version: Some("18.19.1".into()),
                reason: "too old".into(),
            })
            .into_installed(),
            None
        );
        assert_eq!(
            DetectOutcome::from_detected(Some(v.clone())),
            DetectOutcome::Installed(v)
        );
        assert_eq!(DetectOutcome::from_detected(None), DetectOutcome::NotInstalled);
    }

    /// unusable 缺省不落盘,老前端 / 旧 JSON 照常解析
    #[test]
    fn detect_result_unusable_is_optional_on_wire() {
        let json = r#"{"component_id":"nodejs","host_id":"local","supported":true}"#;
        let decoded: ComponentDetectResult = serde_json::from_str(json).unwrap();
        assert_eq!(decoded.detected, None);
        assert_eq!(decoded.unusable, None);
        let encoded = serde_json::to_string(&decoded).unwrap();
        assert!(!encoded.contains("unusable"));
    }

    #[test]
    fn supported_target_from_tuple() {
        let st: SupportedTarget = (ncd_host::Os::Linux, ncd_host::Locality::Remote).into();
        assert_eq!(st.os, ncd_host::Os::Linux);
        assert_eq!(st.locality, ncd_host::Locality::Remote);
    }
}

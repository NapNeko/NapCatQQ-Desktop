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
        }
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
/// - Framework:用户主动选择安装的 Bot 框架(NapCat / SnowLuma)
/// - RuntimeDep:Framework 依赖的运行时(QQ / NodeJs / NoVnc)
/// - SelfApp:Desktop 产品侧(本机 Desktop 自更新;远端 ncd-watch 脱管监控)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ComponentCategory {
    Framework,
    RuntimeDep,
    SelfApp,
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
    fn component_id_serde_aligns_with_as_str() {
        for id in [
            ComponentId::NapCat,
            ComponentId::SnowLuma,
            ComponentId::Qq,
            ComponentId::NodeJs,
            ComponentId::NoVnc,
            ComponentId::DesktopSelf,
            ComponentId::NcdWatch,
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
            description: "NapCat 框架（注入 QQ 进程）".to_string(),
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
    fn supported_target_from_tuple() {
        let st: SupportedTarget = (ncd_host::Os::Linux, ncd_host::Locality::Remote).into();
        assert_eq!(st.os, ncd_host::Os::Linux);
        assert_eq!(st.locality, ncd_host::Locality::Remote);
    }
}

//! framework_id → 适配器。注册即可被编排层与 UI 看到。

use std::collections::BTreeMap;
use std::sync::Arc;

use ncd_domain::{AppFrameworkId, AppFrameworkManifest};
use ncd_traits::AppFrameworkError;

use crate::adapter::AppFrameworkAdapter;
use crate::astrbot::AstrBotAdapter;
use crate::karin::KarinAdapter;
use crate::nonebot2::NoneBot2Adapter;

#[derive(Default)]
pub struct AppFrameworkRegistry {
    adapters: BTreeMap<String, Arc<dyn AppFrameworkAdapter>>,
}

impl AppFrameworkRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 内置框架。接新框架：本函数加一行 + `ComponentId` 变体 + `is_app_framework` +（可选）前端 UI 模块。
    pub fn with_builtin() -> Self {
        let mut reg = Self::new();
        reg.register(Arc::new(KarinAdapter::new()));
        reg.register(Arc::new(NoneBot2Adapter::new()));
        reg.register(Arc::new(AstrBotAdapter::new()));
        reg
    }

    pub fn register(&mut self, adapter: Arc<dyn AppFrameworkAdapter>) {
        let id = adapter.manifest().id.as_str().to_string();
        self.adapters.insert(id, adapter);
    }

    pub fn get(&self, id: &AppFrameworkId) -> Result<Arc<dyn AppFrameworkAdapter>, AppFrameworkError> {
        self.adapters
            .get(id.as_str())
            .cloned()
            .ok_or_else(|| AppFrameworkError::NotRegistered(id.as_str().to_string()))
    }

    /// 按 component wire 字面量反查（factory 用）
    pub fn by_component_id(&self, component_id: &str) -> Option<Arc<dyn AppFrameworkAdapter>> {
        self.adapters
            .values()
            .find(|a| a.manifest().component_id == component_id)
            .cloned()
    }

    pub fn manifests(&self) -> Vec<AppFrameworkManifest> {
        self.adapters.values().map(|a| a.manifest().clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_has_karin_and_nonebot2() {
        let reg = AppFrameworkRegistry::with_builtin();
        let ids: Vec<String> = reg
            .manifests()
            .iter()
            .map(|m| m.id.as_str().to_string())
            .collect();
        assert_eq!(
            ids,
            vec![
                "astrbot".to_string(),
                "karin".to_string(),
                "nonebot2".to_string()
            ]
        );
        assert!(reg.get(&AppFrameworkId::new("karin")).is_ok());
        assert!(reg.get(&AppFrameworkId::new("nonebot2")).is_ok());
        assert!(reg.get(&AppFrameworkId::new("astrbot")).is_ok());
        assert!(matches!(
            reg.get(&AppFrameworkId::new("koishi")),
            Err(AppFrameworkError::NotRegistered(_))
        ));
        assert!(reg.by_component_id("karin").is_some());
        assert!(reg.by_component_id("nonebot2").is_some());
        assert!(reg.by_component_id("astrbot").is_some());
        assert!(reg.by_component_id("uv").is_none(), "运行时依赖不是应用端框架");
    }

    /// 每个框架的 component_id 必须是 `ComponentId` 里标了 is_app_framework 的变体，
    /// factory / 任务队列靠它区分「按实例安装」与 catalog 组件
    #[test]
    fn every_builtin_component_id_is_an_app_framework_component() {
        for m in AppFrameworkRegistry::with_builtin().manifests() {
            let id = ncd_component::ComponentId::parse(&m.component_id)
                .unwrap_or_else(|| panic!("{} 不是合法 ComponentId", m.component_id));
            assert!(id.is_app_framework(), "{} 未标 is_app_framework", m.component_id);
        }
    }

    /// manifest 里给 UI 看的运行时依赖必须与 Component::requirements() 一致（单一事实在 requirements）
    #[test]
    fn manifest_runtime_component_ids_match_component_requirements() {
        use ncd_host::{HostPath, Locality, Os};
        let reg = AppFrameworkRegistry::with_builtin();
        for m in reg.manifests() {
            let adapter = reg.get(&m.id).expect("registered");
            let spec = crate::adapter::AppComponentSpec {
                install_dir: HostPath::from_posix("/x"),
                port: 1,
                node_bin: None,
                uv_bin: None,
                npm_registry: None,
                install_renderer: false,
                adopt_existing: false,
                instance_id: "x".into(),
                webui_username: None,
                webui_password: None,
            };
            let mut from_reqs: Vec<String> = adapter
                .component(&spec)
                .requirements(Os::Linux, Locality::Remote)
                .iter()
                .filter_map(|r| r.component_id())
                .map(|id| id.as_str().to_string())
                .collect();
            from_reqs.sort();
            let mut declared = m.runtime_component_ids.clone();
            declared.sort();
            assert_eq!(declared, from_reqs, "{} 的 runtime_component_ids 与 requirements 不一致", m.id.as_str());
        }
    }

    #[test]
    fn manifest_store_resources_have_market_urls() {
        let reg = AppFrameworkRegistry::with_builtin();
        for m in reg.manifests() {
            let adapter = reg.get(&m.id).expect("registered");
            for resource in &m.store_resources {
                assert!(
                    !adapter.store_market_urls(*resource).is_empty(),
                    "{} 声明了 {:?} 商店但没有目录 URL",
                    m.id.as_str(),
                    resource
                );
            }
        }
    }
}

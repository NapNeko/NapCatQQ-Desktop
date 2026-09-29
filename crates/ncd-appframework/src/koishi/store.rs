//! 插件市场：上游 registry 的 `index.json`（`@koishijs/registry` 的 SearchResult，四五千条、5 MB 上下）。
//! 装更卸都是改 package.json 再跑整包自带的 yarn（上游 market 插件也是这么装的），
//! 插件树由适配器另外改：装完加一条停用的（配好再开），卸之前先把所有条目摘掉。

use ncd_domain::AppStoreResource;
use serde::Deserialize;
use serde_json::Value;

use super::manifest::LOCKED_PACKAGES;
use super::probe::{KoishiPackageInfo, short_name};
use super::yml::KoishiInstanceConfig;
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};

/// 官方源排第一；后两个是社区镜像，内容同源
pub const KOISHI_MARKET_URLS: &[&str] = &[
    "https://registry.koishi.chat/index.json",
    "https://koishi-registry.yumetsuki.moe/index.json",
    "https://kp.itzdrli.cc/index.json",
];

#[derive(Deserialize)]
struct SearchResult {
    #[serde(default)]
    objects: Vec<SearchObject>,
}

#[derive(Deserialize)]
struct SearchObject {
    shortname: String,
    package: RemotePackage,
    #[serde(default)]
    manifest: Option<Manifest>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    insecure: bool,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
}

#[derive(Deserialize)]
struct RemotePackage {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    publisher: Option<Person>,
    #[serde(default)]
    maintainers: Vec<Person>,
    #[serde(default)]
    links: Option<Links>,
}

#[derive(Deserialize)]
struct Person {
    #[serde(default)]
    username: String,
}

#[derive(Deserialize)]
struct Links {
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    npm: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    description: Option<Value>,
    #[serde(default)]
    hidden: bool,
}

/// 上游市场的分类名（`@koishijs/registry` 的 categories）
const CATEGORY_LABELS: &[(&str, &str)] = &[
    ("adapter", "适配器"),
    ("storage", "存储服务"),
    ("extension", "扩展功能"),
    ("console", "控制台"),
    ("manage", "管理工具"),
    ("preset", "预设"),
    ("image", "图片服务"),
    ("media", "资讯服务"),
    ("tool", "实用工具"),
    ("ai", "人工智能"),
    ("meme", "趣味交互"),
    ("game", "娱乐玩法"),
    ("gametool", "游戏工具"),
    ("life", "生活服务"),
    ("webui", "控制台"),
    ("general", "通用"),
    ("other", "其他"),
];

fn category_label(key: &str) -> Option<&'static str> {
    CATEGORY_LABELS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

fn localized(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(m) => ["zh", "zh-CN", "en"]
            .iter()
            .find_map(|k| m.get(*k).and_then(Value::as_str))
            .or_else(|| m.values().find_map(Value::as_str))
            .map(str::to_string),
        _ => None,
    }
}

pub fn parse_koishi_market(text: &str) -> Result<Vec<AppStoreMarketEntry>, String> {
    let result: SearchResult =
        serde_json::from_str(text).map_err(|e| format!("Koishi 插件市场解析失败: {e}"))?;
    Ok(result
        .objects
        .into_iter()
        .filter(|o| !o.manifest.as_ref().is_some_and(|m| m.hidden))
        .map(|o| {
            let description = o
                .manifest
                .as_ref()
                .and_then(|m| m.description.as_ref())
                .and_then(localized)
                .filter(|s| !s.trim().is_empty())
                .or(o.package.description.clone())
                .unwrap_or_default();
            let author = o
                .package
                .publisher
                .as_ref()
                .map(|p| p.username.clone())
                .filter(|s| !s.is_empty())
                .or_else(|| o.package.maintainers.first().map(|p| p.username.clone()))
                .unwrap_or_default();
            let homepage = o
                .package
                .links
                .as_ref()
                .and_then(|l| {
                    l.homepage
                        .clone()
                        .or(l.repository.clone())
                        .or(l.npm.clone())
                })
                .unwrap_or_default();
            let mut tags: Vec<String> = o
                .category
                .as_deref()
                .and_then(category_label)
                .map(|s| vec![s.to_string()])
                .unwrap_or_default();
            if o.insecure {
                tags.push("不安全".to_string());
            }
            tags.extend(
                o.package
                    .keywords
                    .iter()
                    .filter(|k| !matches!(k.as_str(), "koishi" | "plugin" | "chatbot" | "bot"))
                    .take(4)
                    .cloned(),
            );
            AppStoreMarketEntry {
                resource: AppStoreResource::Plugin,
                id: o.package.name.clone(),
                name: o.shortname.clone(),
                description,
                version: o.package.version.clone(),
                author,
                homepage,
                time: o.updated_at.unwrap_or_default(),
                package: o.package.name.clone(),
                module_name: o.shortname,
                flavor: AppStoreFlavor::Npm,
                is_official: o.package.name.starts_with("@koishijs/"),
                valid: !o.insecure,
                tags,
                supported_adapters: Vec::new(),
                authors: Vec::new(),
                repos: Vec::new(),
                files: Vec::new(),
                allow_build: Vec::new(),
            }
        })
        .collect())
}

pub fn is_locked(package: &str) -> bool {
    LOCKED_PACKAGES.contains(&package)
}

/// 已装 = package.json 里的插件依赖；启用看插件树里有没有开着的条目
pub fn installed_rows(
    packages: &[KoishiPackageInfo],
    config: &KoishiInstanceConfig,
) -> Vec<AppStoreInstalled> {
    let effective = config.effective();
    packages
        .iter()
        .map(|p| AppStoreInstalled {
            id: p.package.clone(),
            name: p.name.clone(),
            resource: AppStoreResource::Plugin,
            flavor: AppStoreFlavor::Npm,
            version: p.version.clone(),
            enabled: effective.iter().any(|n| n.name == p.name),
            package: p.package.clone(),
            locked: is_locked(&p.package),
        })
        .collect()
}

/// yarn 的版本参数：市场给了版本就锁大版本范围（上游 market 写的也是 `^x.y.z`）
pub fn yarn_spec(package: &str, version: &str) -> String {
    let v = version.trim().trim_start_matches(['^', '~']);
    if v.is_empty() {
        package.to_string()
    } else {
        format!("{package}@^{v}")
    }
}

/// 商店 id 可能是包名也可能是短名，统一成（包名, 短名）
pub fn resolve_ids(id: &str, packages: &[KoishiPackageInfo]) -> (String, String) {
    if let Some(p) = packages.iter().find(|p| p.package == id || p.name == id) {
        return (p.package.clone(), p.name.clone());
    }
    (id.to_string(), short_name(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{"version":6,"objects":[
      {"shortname":"adapter-onebot","category":"adapter","insecure":false,"updatedAt":"2026-06-05T06:17:20.210Z",
       "package":{"name":"koishi-plugin-adapter-onebot","version":"6.9.4","description":"OneBot Adapter for Koishi",
         "keywords":["bot","koishi","onebot"],"publisher":{"username":"shigma"},
         "links":{"homepage":"https://github.com/koishijs/koishi-plugin-adapter-onebot"}},
       "manifest":{"description":{"en":"OneBot Adapter for Koishi","zh":"OneBot 适配器"}}},
      {"shortname":"hidden-one","package":{"name":"koishi-plugin-hidden-one"},"manifest":{"hidden":true}},
      {"shortname":"evil","insecure":true,"package":{"name":"koishi-plugin-evil","version":"1.0.0","maintainers":[{"username":"m"}]}},
      {"shortname":"help","package":{"name":"@koishijs/plugin-help","version":"2.4.6"}}
    ]}"#;

    #[test]
    fn market_entries_follow_registry_shape() {
        let list = parse_koishi_market(INDEX).unwrap();
        assert_eq!(list.len(), 3, "manifest.hidden 的不列");
        let ob = &list[0];
        assert_eq!(ob.id, "koishi-plugin-adapter-onebot");
        assert_eq!(ob.name, "adapter-onebot");
        assert_eq!(ob.description, "OneBot 适配器");
        assert_eq!(ob.author, "shigma");
        assert_eq!(ob.tags[0], "适配器");
        assert!(ob.tags.contains(&"onebot".to_string()));
        assert!(!ob.tags.contains(&"koishi".to_string()));
        assert!(ob.valid && !ob.is_official);
        let evil = &list[1];
        assert!(!evil.valid);
        assert_eq!(evil.author, "m");
        assert!(list[2].is_official);
        assert!(parse_koishi_market("{").is_err());
    }

    #[test]
    fn installed_rows_mark_enabled_and_locked() {
        let cfg = KoishiInstanceConfig::parse(
            "plugins:\n  group:a:\n    help:x: {}\n  ~group:b:\n    foo:y: {}\n  adapter-onebot:ncd-link: {}\n",
        )
        .unwrap();
        let pk = |p: &str, n: &str| KoishiPackageInfo {
            package: p.into(),
            name: n.into(),
            request: "1".into(),
            version: Some("1.0.0".into()),
            description: String::new(),
        };
        let rows = installed_rows(
            &[
                pk("@koishijs/plugin-help", "help"),
                pk("koishi-plugin-foo", "foo"),
                pk("koishi-plugin-adapter-onebot", "adapter-onebot"),
            ],
            &cfg,
        );
        assert!(rows[0].enabled);
        assert!(!rows[1].enabled, "在停用的分组里不算启用");
        assert!(rows[2].locked && rows[2].enabled);
    }

    #[test]
    fn yarn_spec_and_id_resolution() {
        assert_eq!(
            yarn_spec("koishi-plugin-foo", "1.2.3"),
            "koishi-plugin-foo@^1.2.3"
        );
        assert_eq!(
            yarn_spec("koishi-plugin-foo", "^1.2.3"),
            "koishi-plugin-foo@^1.2.3"
        );
        assert_eq!(yarn_spec("koishi-plugin-foo", ""), "koishi-plugin-foo");
        assert_eq!(
            resolve_ids("koishi-plugin-bar", &[]),
            ("koishi-plugin-bar".to_string(), "bar".to_string())
        );
    }
}

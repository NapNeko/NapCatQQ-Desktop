//! `koishi.yml` ↔ 类型化插件树。
//!
//! 树的形状是桌面端的（节点：名字 / 标识 / 启停 / 元信息 / 配置 / 子节点），插件自己的配置值不是：
//! 每个插件的字段由它自己的 schema 定，和 AstrBot 插件配置一样按 JSON 值原样搬运、原序保留。
//! 上游自己写这份文件也是 js-yaml 整份 dump，本来就不留注释，所以这里整份重写不丢东西。
//!
//! 键的规矩（`@koishijs/loader` 的 `shared.ts`）：`名字:标识`，`~` 前缀 = 停用，`$` 开头 = 元信息，
//! 名字为 `group` 的是分组，它的值里除了元信息都是子插件。

use ncd_domain::AppConfigIssue;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use super::manifest::KOISHI_DEFAULT_PORT;
use crate::config_doc::IssueSink;

pub const GROUP_NAME: &str = "group";
pub const SERVER_NAME: &str = "server";
pub const CONSOLE_NAME: &str = "console";

/// 插件树里的一个节点（插件或分组）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiPluginNode {
    /// 短名（`server` / `adapter-onebot` / `@scope/foo`）；分组为 `group`
    pub name: String,
    /// 冒号后的标识；首启前的模板条目可能没有
    pub ident: String,
    pub enabled: bool,
    /// `$if` / `$filter` / `$label` 之类的元信息，原样保留
    #[ts(type = "Record<string, unknown>")]
    pub meta: Map<String, Value>,
    /// 插件配置；分组恒为空对象
    #[ts(type = "Record<string, unknown>")]
    pub config: Map<String, Value>,
    /// 分组的子节点；插件恒为空
    pub children: Vec<KoishiPluginNode>,
}

impl KoishiPluginNode {
    pub fn plugin(name: &str, ident: &str, enabled: bool, config: Map<String, Value>) -> Self {
        Self {
            name: name.to_string(),
            ident: ident.to_string(),
            enabled,
            meta: Map::new(),
            config,
            children: Vec::new(),
        }
    }

    pub fn is_group(&self) -> bool {
        self.name == GROUP_NAME
    }

    /// 不带 `~` 的键
    pub fn key(&self) -> String {
        if self.ident.is_empty() {
            self.name.clone()
        } else {
            format!("{}:{}", self.name, self.ident)
        }
    }

    /// 写进文件的键
    pub fn file_key(&self) -> String {
        if self.enabled {
            self.key()
        } else {
            format!("~{}", self.key())
        }
    }

    /// 写进文件的值：元信息在前、配置在后（上游 `before-update` 也是这个顺序）；分组是子节点表
    pub fn file_value(&self) -> Value {
        let mut out = self.meta.clone();
        if self.is_group() {
            for child in &self.children {
                out.insert(child.file_key(), child.file_value());
            }
        } else {
            for (k, v) in &self.config {
                out.insert(k.clone(), v.clone());
            }
        }
        Value::Object(out)
    }
}

/// 类型化的 koishi.yml
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiInstanceConfig {
    /// 根上 `plugins` 以外的键（全局设置：前缀、昵称、国际化、延迟…），按上游 `Context.Config` 的 schema 画
    #[ts(type = "Record<string, unknown>")]
    pub global: Map<String, Value>,
    /// 入口分组自己的元信息（极少有）
    #[ts(type = "Record<string, unknown>")]
    pub entry_meta: Map<String, Value>,
    pub plugins: Vec<KoishiPluginNode>,
}

/// server 插件里桌面端关心的几项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KoishiServerView {
    pub port: u16,
    pub host: String,
    pub max_port: Option<u16>,
}

impl KoishiInstanceConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        let root: Value = if text.trim().is_empty() {
            Value::Object(Map::new())
        } else {
            serde_yaml::from_str(text).map_err(|e| format!("koishi.yml 解析失败: {e}"))?
        };
        let Value::Object(mut root) = root else {
            return Err("koishi.yml 根上应该是一张表".to_string());
        };
        let plugins = match root.remove("plugins") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(m)) => m,
            Some(_) => return Err("koishi.yml 的 plugins 应该是一张表".to_string()),
        };
        let (entry_meta, nodes) = parse_group(plugins, "plugins")?;
        Ok(Self {
            global: root,
            entry_meta,
            plugins: nodes,
        })
    }

    pub fn to_value(&self) -> Value {
        let mut root = Map::new();
        let mut plugins = self.entry_meta.clone();
        for node in &self.plugins {
            plugins.insert(node.file_key(), node.file_value());
        }
        root.insert("plugins".to_string(), Value::Object(plugins));
        for (k, v) in &self.global {
            root.insert(k.clone(), v.clone());
        }
        Value::Object(root)
    }

    pub fn render(&self) -> Result<String, String> {
        serde_yaml::to_string(&self.to_value()).map_err(|e| format!("生成 koishi.yml 失败: {e}"))
    }

    /// 深度优先的全部节点
    pub fn walk(&self) -> Vec<&KoishiPluginNode> {
        fn go<'a>(nodes: &'a [KoishiPluginNode], out: &mut Vec<&'a KoishiPluginNode>) {
            for n in nodes {
                out.push(n);
                go(&n.children, out);
            }
        }
        let mut out = Vec::new();
        go(&self.plugins, &mut out);
        out
    }

    /// 真正生效的节点（自己和所有上级分组都开着）
    pub fn effective(&self) -> Vec<&KoishiPluginNode> {
        fn go<'a>(nodes: &'a [KoishiPluginNode], out: &mut Vec<&'a KoishiPluginNode>) {
            for n in nodes.iter().filter(|n| n.enabled) {
                out.push(n);
                go(&n.children, out);
            }
        }
        let mut out = Vec::new();
        go(&self.plugins, &mut out);
        out
    }

    fn server_node(&self) -> Option<&KoishiPluginNode> {
        self.effective()
            .into_iter()
            .find(|n| n.name == SERVER_NAME)
            .or_else(|| self.walk().into_iter().find(|n| n.name == SERVER_NAME))
    }

    pub fn server(&self) -> KoishiServerView {
        let node = self.server_node();
        let cfg = node.map(|n| &n.config);
        let port = cfg
            .and_then(|c| c.get("port"))
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(KOISHI_DEFAULT_PORT);
        let host = cfg
            .and_then(|c| c.get("host"))
            .and_then(Value::as_str)
            .unwrap_or("127.0.0.1")
            .to_string();
        let max_port = cfg
            .and_then(|c| c.get("maxPort"))
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok());
        KoishiServerView {
            port,
            host,
            max_port,
        }
    }

    pub fn listen_port(&self) -> u16 {
        self.server().port
    }

    /// 首装 / 桌面端改口：写口、只绑回环、去掉 maxPort（口被占时上游会悄悄顺延，桌面端就找不到它了）。
    /// 没有 server 插件就在根上补一个
    pub fn pin_server(&mut self, port: u16) {
        let slot = find_mut(&mut self.plugins, &|n| n.name == SERVER_NAME);
        let node = match slot {
            Some(n) => n,
            None => {
                self.plugins.insert(
                    0,
                    KoishiPluginNode::plugin(SERVER_NAME, "", true, Map::new()),
                );
                &mut self.plugins[0]
            }
        };
        node.enabled = true;
        node.config.insert("port".into(), Value::from(port));
        node.config.remove("maxPort");
        node.config
            .entry("host")
            .or_insert_with(|| Value::from("127.0.0.1"));
    }

    /// 首装：后台起的，别让它去开浏览器
    pub fn disable_console_autoopen(&mut self) {
        if let Some(node) = find_mut(&mut self.plugins, &|n| n.name == CONSOLE_NAME) {
            node.config.insert("open".into(), Value::Bool(false));
        }
    }

    pub fn find(&self, name: &str, ident: &str) -> Option<&KoishiPluginNode> {
        self.walk()
            .into_iter()
            .find(|n| n.name == name && n.ident == ident)
    }

    pub fn find_mut(&mut self, name: &str, ident: &str) -> Option<&mut KoishiPluginNode> {
        find_mut(&mut self.plugins, &|n| n.name == name && n.ident == ident)
    }

    /// 分组（按标识找）的子节点表；没有就是入口
    pub fn group_children_mut(&mut self, group_ident: &str) -> &mut Vec<KoishiPluginNode> {
        fn path_to(nodes: &[KoishiPluginNode], ident: &str) -> Option<Vec<usize>> {
            for (i, n) in nodes.iter().enumerate() {
                if !n.is_group() {
                    continue;
                }
                if n.ident == ident {
                    return Some(vec![i]);
                }
                if let Some(mut rest) = path_to(&n.children, ident) {
                    rest.insert(0, i);
                    return Some(rest);
                }
            }
            None
        }
        let path = if group_ident.is_empty() {
            Vec::new()
        } else {
            path_to(&self.plugins, group_ident).unwrap_or_default()
        };
        let mut nodes = &mut self.plugins;
        for i in path {
            nodes = &mut nodes[i].children;
        }
        nodes
    }

    /// 节点所在分组的标识（入口为空串）；找不到返回 None
    pub fn parent_ident_of(&self, name: &str, ident: &str) -> Option<String> {
        fn go(nodes: &[KoishiPluginNode], parent: &str, name: &str, ident: &str) -> Option<String> {
            for n in nodes {
                if n.name == name && n.ident == ident {
                    return Some(parent.to_string());
                }
                if n.is_group()
                    && let Some(p) = go(&n.children, &n.ident, name, ident)
                {
                    return Some(p);
                }
            }
            None
        }
        go(&self.plugins, "", name, ident)
    }

    /// 摘掉节点，返回摘下来的那个
    pub fn remove(&mut self, name: &str, ident: &str) -> Option<KoishiPluginNode> {
        fn go(
            nodes: &mut Vec<KoishiPluginNode>,
            name: &str,
            ident: &str,
        ) -> Option<KoishiPluginNode> {
            if let Some(i) = nodes
                .iter()
                .position(|n| n.name == name && n.ident == ident)
            {
                return Some(nodes.remove(i));
            }
            nodes
                .iter_mut()
                .find_map(|n| go(&mut n.children, name, ident))
        }
        go(&mut self.plugins, name, ident)
    }

    /// 全树已用的标识（上游要求全局唯一）
    pub fn used_idents(&self) -> Vec<String> {
        self.walk()
            .into_iter()
            .filter(|n| !n.ident.is_empty())
            .map(|n| n.ident.clone())
            .collect()
    }

    /// 生成一个没被占用的标识（上游是 `Math.random().toString(36).slice(2, 8)`）
    pub fn fresh_ident(&self) -> String {
        let used = self.used_idents();
        loop {
            let id = random_ident();
            if !used.contains(&id) {
                return id;
            }
        }
    }

    pub fn validate(&self) -> Vec<AppConfigIssue> {
        let mut sink = IssueSink::default();
        let port = self.server();
        if port.port == 0 {
            sink.push("server/port", "端口不能为 0");
        }
        let mut seen: Vec<&str> = Vec::new();
        fn check<'a>(
            nodes: &'a [KoishiPluginNode],
            path: &str,
            sink: &mut IssueSink,
            seen: &mut Vec<&'a str>,
        ) {
            let mut keys: Vec<String> = Vec::new();
            for (i, n) in nodes.iter().enumerate() {
                let here = format!("{path}/{i}");
                let name = n.name.trim();
                if name.is_empty() {
                    sink.push(format!("{here}/name"), "插件名不能为空");
                } else if name.contains(':') || name.starts_with('~') || name.starts_with('$') {
                    sink.push(
                        format!("{here}/name"),
                        "插件名不能含冒号，也不能以 ~ / $ 开头",
                    );
                }
                if n.ident.contains(':') || n.ident.contains('~') {
                    sink.push(format!("{here}/ident"), "标识不能含冒号或 ~");
                }
                if !n.ident.is_empty() {
                    if seen.contains(&n.ident.as_str()) {
                        sink.push(format!("{here}/ident"), format!("标识 {} 重复了", n.ident));
                    }
                    seen.push(n.ident.as_str());
                }
                let key = n.key();
                if keys.contains(&key) {
                    sink.push(
                        format!("{here}/name"),
                        format!("同一分组里 {key} 出现了两次"),
                    );
                }
                keys.push(key);
                if n.is_group() {
                    check(&n.children, &format!("{here}/children"), sink, seen);
                }
            }
        }
        check(&self.plugins, "plugins", &mut sink, &mut seen);
        sink.into_vec()
    }
}

fn find_mut<'a>(
    nodes: &'a mut [KoishiPluginNode],
    pred: &dyn Fn(&KoishiPluginNode) -> bool,
) -> Option<&'a mut KoishiPluginNode> {
    for n in nodes.iter_mut() {
        if pred(n) {
            return Some(n);
        }
        if let Some(hit) = find_mut(&mut n.children, pred) {
            return Some(hit);
        }
    }
    None
}

pub fn random_ident() -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// `~name:ident` → (enabled, name, ident)
pub fn split_key(raw: &str) -> (bool, String, String) {
    let (enabled, rest) = match raw.strip_prefix('~') {
        Some(r) => (false, r),
        None => (true, raw),
    };
    match rest.split_once(':') {
        Some((name, ident)) => (enabled, name.to_string(), ident.to_string()),
        None => (enabled, rest.to_string(), String::new()),
    }
}

fn split_meta(value: Value) -> (Map<String, Value>, Map<String, Value>) {
    let mut meta = Map::new();
    let mut rest = Map::new();
    if let Value::Object(m) = value {
        for (k, v) in m {
            if k.starts_with('$') {
                meta.insert(k, v);
            } else {
                rest.insert(k, v);
            }
        }
    }
    (meta, rest)
}

fn parse_group(
    map: Map<String, Value>,
    path: &str,
) -> Result<(Map<String, Value>, Vec<KoishiPluginNode>), String> {
    let mut meta = Map::new();
    let mut nodes = Vec::new();
    for (key, value) in map {
        if key.starts_with('$') {
            meta.insert(key, value);
            continue;
        }
        let (enabled, name, ident) = split_key(&key);
        if name == GROUP_NAME {
            let inner = match value {
                Value::Object(m) => m,
                Value::Null => Map::new(),
                _ => return Err(format!("{path} 里的 {key} 是分组，值应该是一张表")),
            };
            let (gmeta, children) = parse_group(inner, &format!("{path}.{key}"))?;
            nodes.push(KoishiPluginNode {
                name,
                ident,
                enabled,
                meta: gmeta,
                config: Map::new(),
                children,
            });
        } else {
            let (pmeta, config) = match value {
                Value::Null => (Map::new(), Map::new()),
                v @ Value::Object(_) => split_meta(v),
                _ => return Err(format!("{path} 里的 {key} 的配置应该是一张表")),
            };
            nodes.push(KoishiPluginNode {
                name,
                ident,
                enabled,
                meta: pmeta,
                config,
                children: Vec::new(),
            });
        }
    }
    Ok((meta, nodes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = include_str!("testdata/koishi.template.yml");
    const MIGRATED: &str = include_str!("testdata/koishi.migrated.yml");

    #[test]
    fn parses_upstream_template_without_idents() {
        let cfg = KoishiInstanceConfig::parse(TEMPLATE).unwrap();
        let server_group = &cfg.plugins[0];
        assert!(server_group.is_group());
        assert_eq!(server_group.ident, "server");
        let server = &server_group.children[0];
        assert_eq!(
            (server.name.as_str(), server.ident.as_str()),
            ("server", "")
        );
        assert_eq!(cfg.server().port, 5140);
        assert_eq!(cfg.server().max_port, Some(5149));
        let satori = &server_group.children[1];
        assert!(!satori.enabled);
        let android = cfg.find("android", "").unwrap();
        assert_eq!(
            android.meta.get("$if").and_then(Value::as_str),
            Some("env.KOISHI_AGENT?.includes('Android')")
        );
        assert!(android.config.is_empty());
        let develop = cfg.find("group", "develop").unwrap();
        assert!(develop.meta.contains_key("$if"));
        assert_eq!(cfg.global.get("enableTips"), None);
    }

    #[test]
    fn round_trip_keeps_order_meta_and_disabled_marks() {
        let cfg = KoishiInstanceConfig::parse(MIGRATED).unwrap();
        let again = KoishiInstanceConfig::parse(&cfg.render().unwrap()).unwrap();
        assert_eq!(again, cfg);
        let original: Value = serde_yaml::from_str(MIGRATED).unwrap();
        assert_eq!(cfg.to_value(), original, "值和键序与上游写出的一致");
    }

    #[test]
    fn pin_server_writes_port_host_and_drops_max_port() {
        let mut cfg = KoishiInstanceConfig::parse(MIGRATED).unwrap();
        cfg.pin_server(23140);
        cfg.disable_console_autoopen();
        let s = cfg.server();
        assert_eq!(s.port, 23140);
        assert_eq!(s.host, "127.0.0.1");
        assert_eq!(s.max_port, None);
        let console = cfg
            .walk()
            .into_iter()
            .find(|n| n.name == "console")
            .unwrap();
        assert_eq!(console.config.get("open"), Some(&Value::Bool(false)));

        let mut empty = KoishiInstanceConfig::parse("").unwrap();
        empty.pin_server(1);
        assert_eq!(empty.plugins[0].name, "server");
        assert_eq!(empty.listen_port(), 1);
    }

    #[test]
    fn server_port_comes_from_the_effective_server_plugin() {
        let text = "plugins:\n  ~server:a:\n    port: 1\n  server:b:\n    port: 2\n";
        assert_eq!(KoishiInstanceConfig::parse(text).unwrap().listen_port(), 2);
        let only_off = "plugins:\n  ~server:a:\n    port: 3\n";
        assert_eq!(
            KoishiInstanceConfig::parse(only_off).unwrap().listen_port(),
            3
        );
    }

    #[test]
    fn validate_flags_duplicate_idents_and_bad_names() {
        let text = "plugins:\n  help:abc: {}\n  group:g:\n    commands:abc: {}\n";
        let issues = KoishiInstanceConfig::parse(text).unwrap().validate();
        assert!(
            issues.iter().any(|i| i.message.contains("abc 重复")),
            "{issues:?}"
        );
        let mut cfg = KoishiInstanceConfig::parse("plugins: {}").unwrap();
        cfg.plugins
            .push(KoishiPluginNode::plugin("a:b", "x", true, Map::new()));
        assert_eq!(cfg.validate()[0].path, "plugins/0/name");
        assert!(
            KoishiInstanceConfig::parse(MIGRATED)
                .unwrap()
                .validate()
                .is_empty()
        );
    }

    #[test]
    fn tree_edits_find_parent_remove_and_fresh_idents() {
        let mut cfg = KoishiInstanceConfig::parse(MIGRATED).unwrap();
        assert_eq!(
            cfg.parent_ident_of("help", "ejb1rf").as_deref(),
            Some("basic")
        );
        assert_eq!(cfg.parent_ident_of("group", "server").as_deref(), Some(""));
        let id = cfg.fresh_ident();
        assert_eq!(id.len(), 6);
        assert!(!cfg.used_idents().contains(&id));
        cfg.group_children_mut("adapter")
            .push(KoishiPluginNode::plugin(
                "adapter-onebot",
                "ncd-link",
                true,
                Map::new(),
            ));
        assert_eq!(
            cfg.parent_ident_of("adapter-onebot", "ncd-link").as_deref(),
            Some("adapter")
        );
        assert!(cfg.remove("adapter-onebot", "ncd-link").is_some());
        assert!(cfg.find("adapter-onebot", "ncd-link").is_none());
        cfg.group_children_mut("nope")
            .push(KoishiPluginNode::plugin("x", "y", true, Map::new()));
        assert_eq!(cfg.plugins.last().unwrap().name, "x", "分组不存在就放根上");
    }

    #[test]
    fn rejects_non_table_shapes() {
        assert!(KoishiInstanceConfig::parse("- a").is_err());
        assert!(KoishiInstanceConfig::parse("plugins: 3").is_err());
        assert!(KoishiInstanceConfig::parse("plugins:\n  help: 3\n").is_err());
        let null_value = KoishiInstanceConfig::parse("plugins:\n  help:\n").unwrap();
        assert!(null_value.plugins[0].config.is_empty());
    }
}

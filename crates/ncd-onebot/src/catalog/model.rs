//! 一个后端的动作目录：按名字 / 别名查询、生成给前端的摘要，
//! 以及「上游现取的目录 + 随包快照」的合并、「两个后端之间」的对照标注。

use std::collections::{HashMap, HashSet};

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{
    DebugActionSpec, DebugActionSummary, DebugCatalog, DebugCatalogSource, DebugOtherBackend,
};

use super::diff::{param_diff, params_incompatible};

/// 目录数据长得不对
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("目录格式不对：{0}")]
    Shape(String),
}

/// 一个后端的动作目录
#[derive(Debug, Clone)]
pub struct Catalog {
    backend: BackendType,
    source: DebugCatalogSource,
    version: String,
    actions: Vec<DebugActionSpec>,
    /// 动作名和别名 → `actions` 下标。名字优先于别名：别名撞上别的动作的名字时，查到的仍是那个动作
    index: HashMap<String, usize>,
}

impl Catalog {
    /// 由动作列表建目录。同名动作只留第一个，免得列表里出现两行一样的
    pub fn new(
        backend: BackendType,
        source: DebugCatalogSource,
        version: impl Into<String>,
        actions: Vec<DebugActionSpec>,
    ) -> Self {
        let mut seen = HashSet::new();
        let actions: Vec<DebugActionSpec> = actions
            .into_iter()
            .filter(|spec| seen.insert(spec.name.clone()))
            .collect();

        let mut index = HashMap::with_capacity(actions.len());
        for (i, spec) in actions.iter().enumerate() {
            index.insert(spec.name.clone(), i);
        }
        for (i, spec) in actions.iter().enumerate() {
            for alias in &spec.aliases {
                index.entry(alias.clone()).or_insert(i);
            }
        }

        Self {
            backend,
            source,
            version: version.into(),
            actions,
            index,
        }
    }

    /// 没有任何动作的目录。快照文件坏了时拿它兜底，调用方不必为此 panic
    pub fn empty(backend: BackendType, version: impl Into<String>) -> Self {
        Self::new(backend, DebugCatalogSource::Snapshot, version, Vec::new())
    }

    pub fn backend(&self) -> BackendType {
        self.backend
    }

    pub fn source(&self) -> DebugCatalogSource {
        self.source.clone()
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn actions(&self) -> &[DebugActionSpec] {
        &self.actions
    }

    /// 按动作名或别名查
    pub fn get(&self, name_or_alias: &str) -> Option<&DebugActionSpec> {
        self.index
            .get(name_or_alias)
            .and_then(|&i| self.actions.get(i))
    }

    /// 目录列表里的每一行
    pub fn summaries(&self) -> Vec<DebugActionSummary> {
        self.actions
            .iter()
            .map(|spec| DebugActionSummary {
                name: spec.name.clone(),
                aliases: spec.aliases.clone(),
                summary: spec.summary.clone(),
                category: spec.category.clone(),
                safety: spec.safety.clone(),
                stream: spec.stream,
                supported: spec.supported,
                other_backend_present: spec.other_backend.as_ref().map(|o| o.present),
                param_diff: spec.other_backend.as_ref().is_some_and(|o| o.breaking),
            })
            .collect()
    }

    pub fn to_debug_catalog(&self) -> DebugCatalog {
        DebugCatalog {
            backend: self.backend,
            source: self.source.clone(),
            snapshot_version: self.version.clone(),
            actions: self.summaries(),
        }
    }

    /// 合并上游现取的目录和随包快照。
    ///
    /// - `live` 为 `None`：整份用快照，来源标为快照。
    /// - 否则以 `live` 为「有没有这个动作」的准；快照只补 live 里缺的文档性内容
    ///   （长说明、示例、错误码、返回说明），live 已经给了的不覆盖。
    /// - 快照里有、live 里没有的动作保留下来，标 `supported = false`：
    ///   用户至少能查到文档，也能看出「这个版本的上游没实现它」。
    ///
    /// 合并结果的 `version` 取快照的版本：`DebugCatalog` 里这个字段叫
    /// `snapshot_version`，补进来的文档都出自这份快照；live 那边的版本号没有地方展示。
    pub fn merge(live: Option<Catalog>, snapshot: &Catalog) -> Catalog {
        let Some(live) = live else {
            let actions = snapshot
                .actions
                .iter()
                .map(|spec| DebugActionSpec {
                    supported: true,
                    source: DebugCatalogSource::Snapshot,
                    other_backend: None,
                    ..spec.clone()
                })
                .collect();
            return Catalog::new(
                snapshot.backend,
                DebugCatalogSource::Snapshot,
                snapshot.version.clone(),
                actions,
            );
        };

        let mut merged: Vec<DebugActionSpec> = Vec::with_capacity(live.actions.len());
        for spec in live.actions {
            let mut spec = spec;
            if let Some(doc) = snapshot.get(&spec.name) {
                supplement_from_snapshot(&mut spec, doc);
            }
            spec.supported = true;
            spec.source = DebugCatalogSource::Live;
            spec.other_backend = None;
            merged.push(spec);
        }

        // 快照里有而 live 没有的：按名字和别名都对不上才算缺
        let live_names: HashSet<String> = merged
            .iter()
            .flat_map(|s| std::iter::once(&s.name).chain(s.aliases.iter()))
            .cloned()
            .collect();
        for doc in &snapshot.actions {
            let covered = std::iter::once(&doc.name)
                .chain(doc.aliases.iter())
                .any(|n| live_names.contains(n));
            if !covered {
                merged.push(DebugActionSpec {
                    supported: false,
                    source: DebugCatalogSource::Snapshot,
                    other_backend: None,
                    ..doc.clone()
                });
            }
        }

        Catalog::new(
            live.backend,
            DebugCatalogSource::Live,
            snapshot.version.clone(),
            merged,
        )
    }

    /// 标注另一个后端有没有同名动作，以及两边参数的出入。
    ///
    /// 对方目录里那个动作自己标了 `supported = false`（只在快照里、上游实际没实现）
    /// 时按「没有」算：这里问的是另一个后端现在能不能调它
    pub fn annotate_other(&mut self, other: &Catalog) {
        for spec in &mut self.actions {
            let counterpart = std::iter::once(&spec.name)
                .chain(spec.aliases.iter())
                .find_map(|n| other.get(n))
                .filter(|o| o.supported);
            spec.other_backend = Some(match counterpart {
                Some(o) => DebugOtherBackend {
                    backend: other.backend,
                    present: true,
                    diffs: param_diff(&spec.params_schema, &o.params_schema),
                    breaking: params_incompatible(&spec.params_schema, &o.params_schema),
                },
                None => DebugOtherBackend {
                    backend: other.backend,
                    present: false,
                    diffs: Vec::new(),
                    breaking: false,
                },
            });
        }
    }
}

/// 把快照里的文档性内容补进 live 的动作。只补 live 没有的，不覆盖
fn supplement_from_snapshot(spec: &mut DebugActionSpec, doc: &DebugActionSpec) {
    if spec.summary.is_empty() {
        spec.summary.clone_from(&doc.summary);
    }
    if spec.description.is_none() {
        spec.description.clone_from(&doc.description);
    }
    if spec.returns_schema.is_none() {
        spec.returns_schema.clone_from(&doc.returns_schema);
    }
    if spec.returns_text.is_none() {
        spec.returns_text.clone_from(&doc.returns_text);
    }
    if spec.return_example.is_none() {
        spec.return_example.clone_from(&doc.return_example);
    }
    if spec.examples.is_empty() {
        spec.examples.clone_from(&doc.examples);
    }
    if spec.error_examples.is_empty() {
        spec.error_examples.clone_from(&doc.error_examples);
    }
    if spec.invariants.is_empty() {
        spec.invariants.clone_from(&doc.invariants);
    }
}

#[cfg(test)]
mod tests {
    use ncd_domain::onebot_debug::{DebugActionCategory, DebugActionSafety, DebugErrorExample};
    use serde_json::json;

    use super::*;

    fn spec(name: &str) -> DebugActionSpec {
        DebugActionSpec {
            name: name.to_owned(),
            aliases: Vec::new(),
            summary: String::new(),
            description: None,
            category: DebugActionCategory::Extension,
            safety: DebugActionSafety::ReadOnly,
            stream: false,
            supported: true,
            params_schema: json!({"type": "object", "properties": {}}),
            returns_schema: None,
            returns_text: None,
            return_example: None,
            examples: Vec::new(),
            error_examples: Vec::new(),
            invariants: Vec::new(),
            other_backend: None,
            source: DebugCatalogSource::Live,
        }
    }

    fn catalog(source: DebugCatalogSource, actions: Vec<DebugActionSpec>) -> Catalog {
        Catalog::new(BackendType::NapCat, source, "v1", actions)
    }

    #[test]
    fn get_resolves_name_and_alias_with_name_first() {
        let mut a = spec("get_rkey");
        a.aliases = vec!["nc_get_rkey".to_owned(), "other".to_owned()];
        let mut b = spec("other");
        b.aliases = vec!["get_rkey".to_owned()];
        let cat = catalog(DebugCatalogSource::Live, vec![a, b]);

        assert_eq!(
            cat.get("nc_get_rkey").map(|s| s.name.as_str()),
            Some("get_rkey")
        );
        // `other` 既是 b 的名字又是 a 的别名：名字优先
        assert_eq!(cat.get("other").map(|s| s.name.as_str()), Some("other"));
        assert_eq!(
            cat.get("get_rkey").map(|s| s.name.as_str()),
            Some("get_rkey")
        );
        assert!(cat.get("missing").is_none());
    }

    #[test]
    fn new_drops_duplicate_names() {
        let mut first = spec("dup");
        first.summary = "first".to_owned();
        let mut second = spec("dup");
        second.summary = "second".to_owned();
        let cat = catalog(DebugCatalogSource::Live, vec![first, second]);
        assert_eq!(cat.actions().len(), 1);
        assert_eq!(cat.get("dup").map(|s| s.summary.as_str()), Some("first"));
    }

    #[test]
    fn merge_uses_live_for_presence_and_snapshot_for_docs() {
        let a = spec("a");
        let mut b_live = spec("b");
        b_live.summary = "live summary".to_owned();
        let live = catalog(DebugCatalogSource::Live, vec![a, b_live]);

        let mut b_doc = spec("b");
        b_doc.summary = "snapshot summary".to_owned();
        b_doc.description = Some("long".to_owned());
        b_doc.examples = vec![json!({"x": 1})];
        b_doc.return_example = Some(json!({"ok": true}));
        b_doc.error_examples = vec![DebugErrorExample {
            retcode: 1400,
            message: "bad".to_owned(),
        }];
        let c_doc = spec("c");
        let snap = Catalog::new(
            BackendType::NapCat,
            DebugCatalogSource::Snapshot,
            "snap-1",
            vec![b_doc, c_doc],
        );

        let merged = Catalog::merge(Some(live), &snap);
        assert_eq!(merged.source(), DebugCatalogSource::Live);
        assert_eq!(merged.version(), "snap-1");
        let names: Vec<&str> = merged.actions().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);

        let a = merged.get("a").unwrap();
        assert!(a.supported);
        assert_eq!(a.source, DebugCatalogSource::Live);

        let b = merged.get("b").unwrap();
        assert!(b.supported);
        // live 已有的说明不被覆盖，空着的才补
        assert_eq!(b.summary, "live summary");
        assert_eq!(b.description.as_deref(), Some("long"));
        assert_eq!(b.examples, vec![json!({"x": 1})]);
        assert_eq!(b.return_example, Some(json!({"ok": true})));
        assert_eq!(b.error_examples.len(), 1);

        let c = merged.get("c").unwrap();
        assert!(!c.supported);
        assert_eq!(c.source, DebugCatalogSource::Snapshot);
    }

    #[test]
    fn merge_without_live_marks_everything_supported_snapshot() {
        let mut unsupported = spec("a");
        unsupported.supported = false;
        let snap = catalog(DebugCatalogSource::Live, vec![unsupported, spec("b")]);
        let merged = Catalog::merge(None, &snap);
        assert_eq!(merged.source(), DebugCatalogSource::Snapshot);
        assert!(merged.actions().iter().all(|s| s.supported));
        assert!(
            merged
                .actions()
                .iter()
                .all(|s| s.source == DebugCatalogSource::Snapshot)
        );
    }

    #[test]
    fn merge_matches_snapshot_alias_against_live_name() {
        // live 用的是别名 `get_ptt_text`，快照里那个动作的正式名叫 `fetch_ptt_text`
        let mut doc = spec("fetch_ptt_text");
        doc.aliases = vec!["get_ptt_text".to_owned()];
        doc.examples = vec![json!({"message_id": 1})];
        let snap = catalog(DebugCatalogSource::Snapshot, vec![doc]);
        let live = catalog(DebugCatalogSource::Live, vec![spec("get_ptt_text")]);

        let merged = Catalog::merge(Some(live), &snap);
        assert_eq!(
            merged.actions().len(),
            1,
            "不该把同一个动作当成快照独有的再加一遍"
        );
        let got = merged.get("get_ptt_text").unwrap();
        assert!(got.supported);
        assert_eq!(got.examples.len(), 1);
    }

    #[test]
    fn summaries_reflect_other_backend_annotation() {
        let mut here = spec("only_here");
        here.params_schema =
            json!({"type":"object","properties":{"a":{"type":"string"}},"required":["a"]});
        let cat_here = catalog(DebugCatalogSource::Live, vec![here, spec("nowhere")]);
        let mut cat_here = cat_here;
        assert!(
            cat_here
                .summaries()
                .iter()
                .all(|s| s.other_backend_present.is_none())
        );

        let mut other_spec = spec("only_here");
        other_spec.params_schema = json!({"type":"object","properties":{}});
        let other = Catalog::new(
            BackendType::SnowLuma,
            DebugCatalogSource::Snapshot,
            "sl",
            vec![other_spec],
        );
        cat_here.annotate_other(&other);
        let rows = cat_here.summaries();
        assert_eq!(rows[0].other_backend_present, Some(true));
        assert!(rows[0].param_diff);
        assert_eq!(rows[1].other_backend_present, Some(false));
        assert!(!rows[1].param_diff);
        let dc = cat_here.to_debug_catalog();
        assert_eq!(dc.snapshot_version, "v1");
        assert_eq!(dc.actions.len(), 2);
    }

    #[test]
    fn annotate_other_treats_unsupported_counterpart_as_absent() {
        let mut here = catalog(DebugCatalogSource::Live, vec![spec("x")]);
        let mut unsupported = spec("x");
        unsupported.supported = false;
        let other = Catalog::new(
            BackendType::SnowLuma,
            DebugCatalogSource::Live,
            "sl",
            vec![unsupported],
        );
        here.annotate_other(&other);
        let ob = here.get("x").unwrap().other_backend.as_ref().unwrap();
        assert!(!ob.present);
        assert_eq!(ob.backend, BackendType::SnowLuma);
    }
}

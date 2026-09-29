//! 运行中改配置：Koishi 把 koishi.yml 读进内存自己管，之后每次在控制台改都整份写回，
//! 这时直接改盘会被它下一次写覆盖，也不会生效。所以跑着的时候把「盘上那份 → 要的那份」
//! 拆成控制台插件配置页用的那几个操作（`@koishijs/plugin-config` 的 `writer.ts`）：
//!
//! - `manager/reload(parent, key, config)`：载入或更新插件，并把配置写回（启用）
//! - `manager/unload(parent, key, config)`：卸下插件，键改成 `~key`（停用，配置照写）
//! - `manager/remove(parent, key)`：连配置一起删
//! - `manager/meta(ident, meta)`：只改分组的元信息
//! - `manager/app-reload(config)`：换全局设置，写完 worker 退出码 51 重拉
//!
//! `parent` 是分组标识，入口为空串。分组整体启停时把整棵子树当配置交过去（上游分组插件
//! 按表里的键逐个载入）；分组保持开着就逐个子节点比。顺序调整不追：控制台自己也不靠顺序。

use serde_json::{Map, Value};

use super::yml::{KoishiInstanceConfig, KoishiPluginNode};

#[derive(Debug, Clone, PartialEq)]
pub enum LiveOp {
    Reload {
        parent: String,
        key: String,
        config: Value,
    },
    Unload {
        parent: String,
        key: String,
        config: Value,
    },
    Remove {
        parent: String,
        key: String,
    },
    Meta {
        ident: String,
        meta: Value,
    },
    AppReload {
        config: Value,
    },
}

impl LiveOp {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Reload { .. } => "manager/reload",
            Self::Unload { .. } => "manager/unload",
            Self::Remove { .. } => "manager/remove",
            Self::Meta { .. } => "manager/meta",
            Self::AppReload { .. } => "manager/app-reload",
        }
    }

    pub fn args(&self) -> Vec<Value> {
        match self {
            Self::Reload {
                parent,
                key,
                config,
            }
            | Self::Unload {
                parent,
                key,
                config,
            } => vec![
                Value::from(parent.as_str()),
                Value::from(key.as_str()),
                config.clone(),
            ],
            Self::Remove { parent, key } => {
                vec![Value::from(parent.as_str()), Value::from(key.as_str())]
            }
            Self::Meta { ident, meta } => vec![Value::from(ident.as_str()), meta.clone()],
            Self::AppReload { config } => vec![config.clone()],
        }
    }
}

fn toggle(parent: &str, node: &KoishiPluginNode) -> LiveOp {
    let (parent, key, config) = (parent.to_string(), node.key(), node.file_value());
    if node.enabled {
        LiveOp::Reload {
            parent,
            key,
            config,
        }
    } else {
        LiveOp::Unload {
            parent,
            key,
            config,
        }
    }
}

/// 分组的元信息改动：上游 `meta` 把给的键挪到最前，`null` 表示删掉
fn meta_patch(before: &Map<String, Value>, after: &Map<String, Value>) -> Option<Value> {
    if before == after {
        return None;
    }
    let mut patch = after.clone();
    for k in before.keys() {
        if !after.contains_key(k) {
            patch.insert(k.clone(), Value::Null);
        }
    }
    Some(Value::Object(patch))
}

fn diff_group(
    parent: &str,
    before: &[KoishiPluginNode],
    after: &[KoishiPluginNode],
    ops: &mut Vec<LiveOp>,
) {
    for old in before {
        if !after.iter().any(|n| n.key() == old.key()) {
            ops.push(LiveOp::Remove {
                parent: parent.to_string(),
                key: old.key(),
            });
        }
    }
    for new in after {
        let Some(old) = before.iter().find(|n| n.key() == new.key()) else {
            ops.push(toggle(parent, new));
            continue;
        };
        if old == new {
            continue;
        }
        if !new.is_group() || old.enabled != new.enabled || !new.enabled {
            ops.push(toggle(parent, new));
            continue;
        }
        if let Some(meta) = meta_patch(&old.meta, &new.meta) {
            ops.push(LiveOp::Meta {
                ident: new.ident.clone(),
                meta,
            });
        }
        diff_group(&new.ident, &old.children, &new.children, ops);
    }
}

/// 盘上那份 → 要的那份。全局设置排最后：它会让 worker 重拉，前面的改动先落盘
pub fn plan_live_ops(before: &KoishiInstanceConfig, after: &KoishiInstanceConfig) -> Vec<LiveOp> {
    let mut ops = Vec::new();
    diff_group("", &before.plugins, &after.plugins, &mut ops);
    if before.global != after.global {
        let mut config = after.global.clone();
        // 上游 reloadApp 会把 plugins 换回它内存里那份，这里给不给都一样
        config.remove("plugins");
        ops.push(LiveOp::AppReload {
            config: Value::Object(config),
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(text: &str) -> KoishiInstanceConfig {
        KoishiInstanceConfig::parse(text).unwrap()
    }

    const BASE: &str = "plugins:\n  help:a1: {}\n  group:g:\n    commands:c1: {}\n    ~inspect:i1: {}\n  ~group:off:\n    bind:b1: {}\nprefix: ['/']\n";

    #[test]
    fn no_change_no_ops() {
        assert!(plan_live_ops(&parse(BASE), &parse(BASE)).is_empty());
    }

    #[test]
    fn plugin_config_enable_disable_and_remove() {
        let after = parse(
            "plugins:\n  ~help:a1:\n    x: 1\n  group:g:\n    commands:c1:\n      y: 2\n    inspect:i1: {}\n  ~group:off:\n    bind:b1: {}\n  echo:e1: {}\nprefix: ['/']\n",
        );
        let ops = plan_live_ops(&parse(BASE), &after);
        assert_eq!(
            ops,
            vec![
                LiveOp::Unload {
                    parent: "".into(),
                    key: "help:a1".into(),
                    config: json!({"x": 1})
                },
                LiveOp::Reload {
                    parent: "g".into(),
                    key: "commands:c1".into(),
                    config: json!({"y": 2})
                },
                LiveOp::Reload {
                    parent: "g".into(),
                    key: "inspect:i1".into(),
                    config: json!({})
                },
                LiveOp::Reload {
                    parent: "".into(),
                    key: "echo:e1".into(),
                    config: json!({})
                },
            ]
        );
        let removed = parse(
            "plugins:\n  group:g:\n    commands:c1: {}\n    ~inspect:i1: {}\n  ~group:off:\n    bind:b1: {}\nprefix: ['/']\n",
        );
        assert_eq!(
            plan_live_ops(&parse(BASE), &removed),
            vec![LiveOp::Remove {
                parent: "".into(),
                key: "help:a1".into()
            }]
        );
    }

    #[test]
    fn group_toggle_hands_over_whole_subtree() {
        let after = parse(
            "plugins:\n  help:a1: {}\n  group:g:\n    commands:c1: {}\n    ~inspect:i1: {}\n  group:off:\n    bind:b1: {}\nprefix: ['/']\n",
        );
        assert_eq!(
            plan_live_ops(&parse(BASE), &after),
            vec![LiveOp::Reload {
                parent: "".into(),
                key: "group:off".into(),
                config: json!({"bind:b1": {}})
            }]
        );
        let edited_while_off = parse(
            "plugins:\n  help:a1: {}\n  group:g:\n    commands:c1: {}\n    ~inspect:i1: {}\n  ~group:off:\n    bind:b1:\n      z: 1\nprefix: ['/']\n",
        );
        assert!(matches!(
            &plan_live_ops(&parse(BASE), &edited_while_off)[0],
            LiveOp::Unload { key, .. } if key == "group:off"
        ));
    }

    #[test]
    fn group_meta_and_global_changes() {
        let after = parse(
            "plugins:\n  help:a1: {}\n  group:g:\n    $label: 基础\n    commands:c1: {}\n    ~inspect:i1: {}\n  ~group:off:\n    bind:b1: {}\nprefix: ['#']\nnickname: [bot]\n",
        );
        let ops = plan_live_ops(&parse(BASE), &after);
        assert_eq!(
            ops[0],
            LiveOp::Meta {
                ident: "g".into(),
                meta: json!({"$label": "基础"})
            }
        );
        assert_eq!(
            ops[1],
            LiveOp::AppReload {
                config: json!({"prefix": ["#"], "nickname": ["bot"]})
            }
        );
        assert_eq!(
            ops[1].args(),
            vec![json!({"prefix": ["#"], "nickname": ["bot"]})]
        );
        assert_eq!(
            meta_patch(&json!({"$a": 1}).as_object().unwrap().clone(), &Map::new()),
            Some(json!({"$a": null}))
        );
    }
}

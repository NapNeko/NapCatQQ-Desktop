//! 把类型化配置的改动落回 TOML 文档：只动变了的地方，保住注释、键序和 Desktop 不认识的键。
//!
//! 改前、改后两份配置各自序列化成 `toml::Table` 逐键比较：
//! - 两边都是表就递归；值一样就不碰文件，所以文件里没写的默认值不会被写出来
//! - 标量、数组整值替换，键上原有的注释和行尾注释留着
//! - 对象数组逐项回填：新条目在旧数组里找「同一个」（有身份键按身份键，没有按下标），
//!   把旧条目里 Desktop 不认识的键带过去；数组写法跟着文件现状，文件里没有时顶层用 `[[x]]`、
//!   表里用行内数组（和 tomlkit 从 pydantic 写出来的样子一致）
//! - 改前有、改后没有的键（可选值清空、映射删项）从文档里删掉
//!
//! [`changes`] 给出同样口径的「只含改动」的部分表，运行中走应用自己的接口时拼请求体用。

use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, TableLike, Value};

/// 对象数组的身份键：给出数组所在路径（`["models"]`、`["mcp", "servers"]`），返回按哪个键认条目
pub type IdentityOf<'a> = &'a dyn Fn(&[&str]) -> Option<&'static str>;

pub fn no_identity(_: &[&str]) -> Option<&'static str> {
    None
}

/// 把 `before → after` 的改动写进 `doc`，返回改到的键路径（点分）
pub fn apply(
    doc: &mut DocumentMut,
    before: &toml::Table,
    after: &toml::Table,
    identity: IdentityOf<'_>,
) -> Vec<String> {
    let mut changed = Vec::new();
    patch_table(doc.as_table_mut(), before, after, &mut Vec::new(), identity, true, &mut changed);
    changed
}

/// 只含改动的部分表：表逐层保留、叶子和数组整值给出；删掉的键不在里面（调用方要自己判断能不能走部分更新）
pub fn changes(before: &toml::Table, after: &toml::Table) -> toml::Table {
    let mut out = toml::Table::new();
    for (key, a) in after {
        match (before.get(key), a) {
            (Some(b), a) if b == a => {}
            (Some(toml::Value::Table(b)), toml::Value::Table(a)) => {
                let sub = changes(b, a);
                if !sub.is_empty() {
                    out.insert(key.clone(), toml::Value::Table(sub));
                }
            }
            (_, a) => {
                out.insert(key.clone(), a.clone());
            }
        }
    }
    out
}

/// 改后少了的键（含嵌套）；部分更新表达不了删除，有这类改动时调用方要换整份写
pub fn removed_keys(before: &toml::Table, after: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    collect_removed(before, after, &mut Vec::new(), &mut out);
    out
}

fn collect_removed(before: &toml::Table, after: &toml::Table, path: &mut Vec<String>, out: &mut Vec<String>) {
    for (key, b) in before {
        path.push(key.clone());
        match (b, after.get(key)) {
            (_, None) => out.push(path.join(".")),
            (toml::Value::Table(b), Some(toml::Value::Table(a))) => collect_removed(b, a, path, out),
            _ => {}
        }
        path.pop();
    }
}

fn patch_table(
    table: &mut dyn TableLike,
    before: &toml::Table,
    after: &toml::Table,
    path: &mut Vec<String>,
    identity: IdentityOf<'_>,
    at_root: bool,
    changed: &mut Vec<String>,
) {
    for (key, a) in after {
        let b = before.get(key);
        if b == Some(a) {
            continue;
        }
        path.push(key.clone());
        match (b, a) {
            (Some(toml::Value::Table(b)), toml::Value::Table(a)) => {
                if let Some(child) = ensure_table(table, key) {
                    patch_table(child, b, a, path, identity, false, changed);
                }
            }
            (None, toml::Value::Table(a)) if table.get(key).is_none_or(Item::is_table_like) => {
                if let Some(child) = ensure_table(table, key) {
                    patch_table(child, &toml::Table::new(), a, path, identity, false, changed);
                }
            }
            (b, toml::Value::Array(items)) if items.iter().all(toml::Value::is_table) && !items.is_empty() => {
                let old = b.and_then(toml::Value::as_array).cloned().unwrap_or_default();
                let refs: Vec<&str> = path.iter().map(String::as_str).collect();
                let id = identity(&refs);
                replace_table_array(table, key, &old, items, id, at_root);
                changed.push(path.join("."));
            }
            (_, a) => {
                set_value(table, key, to_edit_value(a));
                changed.push(path.join("."));
            }
        }
        path.pop();
    }
    let gone: Vec<String> = before.keys().filter(|k| !after.contains_key(*k)).cloned().collect();
    for key in gone {
        if table.remove(&key).is_some() {
            path.push(key);
            changed.push(path.join("."));
            path.pop();
        }
    }
}

/// 拿到子表（行内表也算）；没有就建一张隐式表，只装子表时不会多出一个空表头
fn ensure_table<'t>(table: &'t mut dyn TableLike, key: &str) -> Option<&'t mut dyn TableLike> {
    let needs_new = !table.get(key).is_some_and(Item::is_table_like);
    if needs_new {
        let mut t = Table::new();
        t.set_implicit(true);
        table.insert(key, Item::Table(t));
    }
    table.get_mut(key).and_then(Item::as_table_like_mut)
}

/// 已有的键原地换值：键上方的注释挂在键的 decor 上，行尾注释挂在值上，整个 insert 会连注释一起丢
fn set_value(table: &mut dyn TableLike, key: &str, mut v: Value) {
    if let Some(old) = table.get_mut(key).and_then(Item::as_value_mut) {
        *v.decor_mut() = old.decor().clone();
        *old = v;
        return;
    }
    table.insert(key, Item::Value(v));
}

fn replace_table_array(
    table: &mut dyn TableLike,
    key: &str,
    old: &[toml::Value],
    items: &[toml::Value],
    identity: Option<&'static str>,
    at_root: bool,
) {
    let matched = |i: usize, new: &toml::Table| -> Option<usize> {
        match identity {
            Some(id) => {
                let want = new.get(id)?;
                old.iter().position(|o| o.as_table().and_then(|t| t.get(id)) == Some(want))
            }
            None => (i < old.len()).then_some(i),
        }
    };
    let existing = table.get(key);
    let as_aot = match existing {
        Some(Item::ArrayOfTables(_)) => true,
        Some(_) => false,
        None => at_root,
    };
    if as_aot {
        let old_tables: Vec<Table> = match existing {
            Some(Item::ArrayOfTables(aot)) => aot.iter().cloned().collect(),
            _ => Vec::new(),
        };
        let mut aot = ArrayOfTables::new();
        for (i, item) in items.iter().enumerate() {
            let Some(new) = item.as_table() else {
                continue;
            };
            let mut t = match matched(i, new).and_then(|j| old_tables.get(j).cloned().map(|t| (j, t))) {
                Some((j, mut t)) => {
                    let before = old[j].as_table().cloned().unwrap_or_default();
                    patch_table(&mut t, &before, new, &mut Vec::new(), &no_identity, false, &mut Vec::new());
                    t
                }
                None => {
                    let mut t = Table::new();
                    patch_table(&mut t, &toml::Table::new(), new, &mut Vec::new(), &no_identity, false, &mut Vec::new());
                    t
                }
            };
            t.set_implicit(false);
            aot.push(t);
        }
        // 文档按每张表记的位置排版，不看数组顺序：克隆来的旧条目带着旧位置，重排后会被排回原样。
        // 全设成同一个起点，稳定排序就按数组顺序出
        if let Some(base) = old_tables.iter().filter_map(Table::position).min() {
            for t in aot.iter_mut() {
                t.set_position(base);
            }
        }
        table.insert(key, Item::ArrayOfTables(aot));
        return;
    }
    let old_inline: Vec<InlineTable> = match existing.and_then(Item::as_array) {
        Some(arr) => arr.iter().filter_map(|v| v.as_inline_table().cloned()).collect(),
        None => Vec::new(),
    };
    let mut arr = Array::new();
    for (i, item) in items.iter().enumerate() {
        let Some(new) = item.as_table() else {
            continue;
        };
        let mut t = match matched(i, new).and_then(|j| old_inline.get(j).cloned().map(|t| (j, t))) {
            Some((j, mut t)) => {
                let before = old[j].as_table().cloned().unwrap_or_default();
                patch_table(&mut t, &before, new, &mut Vec::new(), &no_identity, false, &mut Vec::new());
                t
            }
            None => inline_table(new),
        };
        t.fmt();
        arr.push_formatted(Value::InlineTable(t));
    }
    arr.fmt();
    set_value(table, key, Value::Array(arr));
}

fn inline_table(t: &toml::Table) -> InlineTable {
    let mut out = InlineTable::new();
    for (k, v) in t {
        out.insert(k, to_edit_value(v));
    }
    out
}

fn to_edit_value(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::from(s.as_str()),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => Value::from(*f),
        toml::Value::Boolean(b) => Value::from(*b),
        toml::Value::Datetime(d) => d
            .to_string()
            .parse::<Value>()
            .unwrap_or_else(|_| Value::from(d.to_string())),
        toml::Value::Array(items) => {
            let mut arr: Array = items.iter().map(to_edit_value).collect();
            arr.fmt();
            Value::Array(arr)
        }
        toml::Value::Table(t) => {
            let mut inline = inline_table(t);
            inline.fmt();
            Value::InlineTable(inline)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    fn run(doc_text: &str, before: &str, after: &str) -> (String, Vec<String>) {
        let mut doc: DocumentMut = doc_text.parse().unwrap();
        let changed = apply(&mut doc, &t(before), &t(after), &no_identity);
        (doc.to_string(), changed)
    }

    #[test]
    fn unchanged_config_leaves_the_file_byte_identical() {
        let text = "[inner]\nversion = \"1\"\n\n[a]\nx = 1 # 注释\n";
        let (out, changed) = run(text, "[a]\nx = 1\ny = 2\n", "[a]\nx = 1\ny = 2\n");
        assert_eq!(out, text);
        assert!(changed.is_empty());
    }

    #[test]
    fn changed_scalar_keeps_comments_and_neighbours() {
        let text = "[a]\n# 上方注释\nx = 1 # 行尾\nkeep = \"me\"\n";
        let (out, changed) = run(text, "[a]\nx = 1\n", "[a]\nx = 5\n");
        assert_eq!(out, "[a]\n# 上方注释\nx = 5 # 行尾\nkeep = \"me\"\n");
        assert_eq!(changed, vec!["a.x"]);
    }

    #[test]
    fn default_values_absent_from_the_file_stay_absent_until_changed() {
        let text = "[webui]\nport = 1\n";
        let (out, _) = run(text, "[webui]\nport = 1\n[chat.reply_timing]\ntalk_value = 1.0\n", "[webui]\nport = 1\n[chat.reply_timing]\ntalk_value = 0.5\n");
        let back = t(&out);
        assert_eq!(back["chat"]["reply_timing"]["talk_value"].as_float(), Some(0.5));
        assert!(!out.contains("[chat]\n"), "只装子表的父表不该多出空表头：{out}");
    }

    #[test]
    fn inline_rule_arrays_keep_unknown_keys_per_item() {
        let text = "[chat.reply_timing]\ntalk_value_rules = [{platform = \"\", value = 0.8, secret = 1}] # 规则\n";
        let (out, _) = run(
            text,
            "[chat.reply_timing]\ntalk_value_rules = [{platform = \"\", value = 0.8}]\n",
            "[chat.reply_timing]\ntalk_value_rules = [{platform = \"qq\", value = 0.8}, {platform = \"\", value = 1.0}]\n",
        );
        let rules = t(&out)["chat"]["reply_timing"]["talk_value_rules"].as_array().unwrap().clone();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["platform"].as_str(), Some("qq"));
        assert_eq!(rules[0]["secret"].as_integer(), Some(1), "旧条目里不认识的键要带过去：{out}");
        assert!(rules[1].get("secret").is_none());
        assert!(out.contains("# 规则"), "{out}");
    }

    #[test]
    fn root_table_arrays_match_items_by_identity() {
        let text = "[[models]]\nname = \"a\"\nx = 1 # a 的注释\nunknown = true\n\n[[models]]\nname = \"b\"\nx = 2\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let before = t("[[models]]\nname = \"a\"\nx = 1\n[[models]]\nname = \"b\"\nx = 2\n");
        let after = t("[[models]]\nname = \"b\"\nx = 3\n[[models]]\nname = \"a\"\nx = 1\n[[models]]\nname = \"c\"\nx = 9\n");
        let id = |p: &[&str]| (p == ["models"]).then_some("name");
        apply(&mut doc, &before, &after, &id);
        let out = doc.to_string();
        let models = t(&out)["models"].as_array().unwrap().clone();
        let names: Vec<&str> = models.iter().map(|m| m["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        assert_eq!(models[0]["x"].as_integer(), Some(3));
        assert_eq!(models[1]["unknown"].as_bool(), Some(true), "按名字认回来的条目带着未知键：{out}");
        assert!(out.contains("x = 1 # a 的注释"), "{out}");
        assert!(out.contains("[[models]]"));
    }

    #[test]
    fn cleared_optionals_and_map_entries_are_removed() {
        let text = "[m]\ntemperature = 0.3\nheaders = {a = \"1\", b = \"2\"}\n";
        let (out, changed) = run(
            text,
            "[m]\ntemperature = 0.3\nheaders = {a = \"1\", b = \"2\"}\n",
            "[m]\nheaders = {a = \"1\"}\n",
        );
        let back = t(&out);
        assert!(back["m"].get("temperature").is_none(), "{out}");
        assert!(back["m"]["headers"].get("b").is_none());
        assert_eq!(back["m"]["headers"]["a"].as_str(), Some("1"));
        assert!(changed.contains(&"m.temperature".to_string()));
    }

    #[test]
    fn changes_and_removed_keys_describe_the_same_edit() {
        let before = t("[a]\nx = 1\ny = [1]\n[b]\nz = 1\nw = 2\n");
        let after = t("[a]\nx = 1\ny = [1, 2]\n[b]\nz = 3\n");
        let delta = changes(&before, &after);
        assert_eq!(delta, t("[a]\ny = [1, 2]\n[b]\nz = 3\n"));
        assert_eq!(removed_keys(&before, &after), vec!["b.w"]);
    }
}

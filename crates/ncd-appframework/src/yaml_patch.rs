//! 按键路径改 YAML 文本：只重写那一个键占的几行，其余原样留着（注释、空行、没见过的键、换行风格）。
//!
//! 云崽的配置都是从 `default_config` 抄过去的，每个键上面或行尾都有中文说明；用 serde_yaml
//! 解析再整份写回会把这些全丢掉。这里只认那些文件实际用到的子集：块状映射（缩进固定）、
//! `- x` 列表、单行标量、`[]` / `{}` 这类单行流式写法。改的是标量时行尾注释照留；改成列表 /
//! 映射时整块重写，键那一行的行尾注释也留着，块里原来的逐项注释丢掉。
//!
//! 读不在这里：读直接交给 serde_yaml，这里只管写。

use serde_yaml::Value;

/// 把 `path` 指的键设成 `value`；路径上缺的映射层级会补出来，缺的键追加在所在映射末尾
pub fn set_value(text: &str, path: &[&str], value: &Value) -> Result<String, String> {
    if path.is_empty() {
        return Err("键路径不能为空".into());
    }
    let mut doc = Doc::parse(text);
    doc.set(path, value)?;
    Ok(doc.render())
}

/// 删掉 `path` 指的键（连同它的子块）；本来就没有时原样返回
pub fn remove_key(text: &str, path: &[&str]) -> Result<String, String> {
    if path.is_empty() {
        return Err("键路径不能为空".into());
    }
    let mut doc = Doc::parse(text);
    doc.remove(path);
    Ok(doc.render())
}

struct Doc {
    lines: Vec<String>,
    newline: &'static str,
    trailing_newline: bool,
}

/// 一行键的拆解：缩进、原样的键文本、键名（去掉引号）、冒号后的值文本、行尾注释（含前导空白）
struct KeyLine {
    indent: usize,
    raw_key: String,
    key: String,
    value: String,
    comment: String,
}

impl Doc {
    fn parse(text: &str) -> Self {
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let trailing_newline = text.is_empty() || text.ends_with('\n');
        let body = text.strip_suffix('\n').unwrap_or(text);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let lines = if body.is_empty() {
            Vec::new()
        } else {
            body.split('\n')
                .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
                .collect()
        };
        Self {
            lines,
            newline,
            trailing_newline,
        }
    }

    fn render(&self) -> String {
        let mut out = self.lines.join(self.newline);
        if self.trailing_newline && !out.is_empty() {
            out.push_str(self.newline);
        }
        out
    }

    fn set(&mut self, path: &[&str], value: &Value) -> Result<(), String> {
        let (mut start, mut end, mut indent) = (0usize, self.lines.len(), 0usize);
        for (depth, key) in path.iter().enumerate() {
            let last = depth + 1 == path.len();
            let child_indent = self.child_indent(start, end, indent, depth == 0);
            match self.find_key(start, end, child_indent, key) {
                Some((line_idx, block_end)) => {
                    // find_key 只返回能拆成键的行
                    let Some(kl) = parse_key_line(&self.lines[line_idx]) else {
                        return Err(format!("{key} 那一行看不懂"));
                    };
                    if last {
                        let rendered = render_entry(child_indent, &kl.raw_key, value, &kl.comment);
                        self.lines.splice(line_idx..block_end, rendered);
                        return Ok(());
                    }
                    if !kl.value.trim().is_empty() {
                        // `default: {}` / `auth:` 后面写了东西：要往下加子键，先把这一行改成块头
                        if !is_empty_flow(&kl.value) && !is_null_text(&kl.value) {
                            return Err(format!("{key} 不是映射，不能往里加 {}", path[depth + 1]));
                        }
                        self.lines[line_idx] =
                            format!("{}{}:{}", " ".repeat(child_indent), kl.raw_key, kl.comment);
                    }
                    start = line_idx + 1;
                    end = block_end;
                    indent = child_indent;
                }
                None => {
                    let insert_at = self.insert_point(start, end);
                    let nested = nest_value(&path[depth + 1..], value);
                    let rendered = render_entry(child_indent, &render_key(key), &nested, "");
                    self.lines.splice(insert_at..insert_at, rendered);
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn remove(&mut self, path: &[&str]) {
        let (mut start, mut end, mut indent) = (0usize, self.lines.len(), 0usize);
        for (depth, key) in path.iter().enumerate() {
            let child_indent = self.child_indent(start, end, indent, depth == 0);
            let Some((line_idx, block_end)) = self.find_key(start, end, child_indent, key) else {
                return;
            };
            if depth + 1 == path.len() {
                self.lines.drain(line_idx..block_end);
                return;
            }
            start = line_idx + 1;
            end = block_end;
            indent = child_indent;
        }
    }

    /// 这一层映射里键的缩进：看第一条键行；空映射按上一层 +2
    fn child_indent(&self, start: usize, end: usize, parent: usize, top: bool) -> usize {
        for line in &self.lines[start..end] {
            if is_blank_or_comment(line) {
                continue;
            }
            if let Some(kl) = parse_key_line(line) {
                return kl.indent;
            }
            return leading_spaces(line);
        }
        if top { 0 } else { parent + 2 }
    }

    /// 在 [start, end) 里找缩进为 `indent`、键名为 `key` 的行，返回 (行号, 块尾)。
    /// 块尾不含紧贴在下一个键前面、缩进不深于它的注释 / 空行：那些是下一个键的说明
    fn find_key(&self, start: usize, end: usize, indent: usize, key: &str) -> Option<(usize, usize)> {
        let idx = (start..end).find(|&i| {
            let line = &self.lines[i];
            !is_blank_or_comment(line)
                && parse_key_line(line).is_some_and(|kl| kl.indent == indent && kl.key == key)
        })?;
        let mut next = end;
        for i in idx + 1..end {
            let line = &self.lines[i];
            if is_blank_or_comment(line) {
                continue;
            }
            if leading_spaces(line) <= indent && !line.trim_start().starts_with('-') {
                next = i;
                break;
            }
            // 列表项可以和键同缩进（`key:\n- a`）；再遇到同级的键才算完
            if leading_spaces(line) < indent {
                next = i;
                break;
            }
        }
        let mut block_end = next;
        while block_end > idx + 1 {
            let prev = &self.lines[block_end - 1];
            let trimmed = prev.trim_start();
            if trimmed.is_empty() || (trimmed.starts_with('#') && leading_spaces(prev) <= indent) {
                block_end -= 1;
            } else {
                break;
            }
        }
        Some((idx, block_end))
    }

    /// 新键追加在这一层最后一个有内容的行后面（不插进后面那个键的说明注释里）
    fn insert_point(&self, start: usize, end: usize) -> usize {
        let mut at = end;
        while at > start && self.lines[at - 1].trim().is_empty() {
            at -= 1;
        }
        at
    }
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_blank_or_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.is_empty() || t.starts_with('#')
}

fn is_null_text(v: &str) -> bool {
    matches!(v.trim(), "" | "~" | "null" | "Null" | "NULL")
}

fn is_empty_flow(v: &str) -> bool {
    matches!(v.trim(), "{}" | "[]")
}

fn parse_key_line(line: &str) -> Option<KeyLine> {
    let indent = leading_spaces(line);
    let rest = &line[indent..];
    if rest.is_empty() || rest.starts_with('#') || rest.starts_with("- ") || rest == "-" {
        return None;
    }
    let (raw_key, key, after) = if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let close = rest[1..].find(q)? + 1;
        let raw = &rest[..=close];
        let after = rest[close + 1..].strip_prefix(':')?;
        (raw.to_string(), rest[1..close].to_string(), after)
    } else {
        // 键里可以有冒号（`114514:123456:`），真正的分隔是后面跟空白或行尾的那个
        let bytes = rest.as_bytes();
        let mut split = None;
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'#' && i > 0 && bytes[i - 1] == b' ' {
                return None;
            }
            if *b == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ' || bytes[i + 1] == b'\t') {
                split = Some(i);
                break;
            }
        }
        let i = split?;
        let raw = &rest[..i];
        (raw.to_string(), raw.trim_end().to_string(), &rest[i + 1..])
    };
    let (value, comment) = split_comment(after);
    Some(KeyLine {
        indent,
        raw_key,
        key,
        value: value.to_string(),
        comment: comment.to_string(),
    })
}

/// `  500   # 冷却` → ("  500", "   # 冷却")；引号里的 # 不算
fn split_comment(s: &str) -> (&str, &str) {
    let bytes = s.as_bytes();
    let (mut single, mut double) = (false, false);
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'#' if !single && !double && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') => {
                let mut cut = i;
                while cut > 0 && matches!(bytes[cut - 1], b' ' | b'\t') {
                    cut -= 1;
                }
                return (&s[..cut], &s[cut..]);
            }
            _ => {}
        }
    }
    (s.trim_end(), &s[s.trim_end().len()..])
}

/// 路径上缺的层级：`["a","b"]` + v → `{a: {b: v}}` 里 a 那一层（调用方已经写了 a 的键）
fn nest_value(rest: &[&str], value: &Value) -> Value {
    match rest.split_first() {
        None => value.clone(),
        Some((first, tail)) => {
            let mut map = serde_yaml::Mapping::new();
            map.insert(Value::String((*first).to_string()), nest_value(tail, value));
            Value::Mapping(map)
        }
    }
}

fn render_key(key: &str) -> String {
    let rendered = render_scalar(&Value::String(key.to_string()));
    // 纯数字的键写成数字：云崽的群号键本来就是这么写的，JS 里对象键也都是字符串
    if key.chars().all(|c| c.is_ascii_digit()) && !key.is_empty() {
        return key.to_string();
    }
    rendered
}

/// 单个标量的 YAML 写法；null 写成空（云崽的默认配置就是 `key:` 留空）
pub fn render_scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            let out = serde_yaml::to_string(&Value::String(s.clone())).unwrap_or_default();
            let out = out.trim_end_matches('\n');
            if out.contains('\n') {
                // 多行字符串：serde_yaml 给的是块写法，单行场景改成双引号转义
                format!("{:?}", s)
            } else {
                out.to_string()
            }
        }
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim_end_matches('\n')
            .to_string(),
    }
}

fn is_scalar(v: &Value) -> bool {
    matches!(v, Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_))
}

/// 一个键连同它的值渲染成若干行。原来那行的行尾注释：新值是标量就跟在值后面，是块就留在键那一行
fn render_entry(indent: usize, raw_key: &str, value: &Value, comment: &str) -> Vec<String> {
    let pad = " ".repeat(indent);
    match value {
        v if is_scalar(v) => {
            let scalar = render_scalar(v);
            if scalar.is_empty() {
                vec![format!("{pad}{raw_key}:{comment}")]
            } else {
                vec![format!("{pad}{raw_key}: {scalar}{comment}")]
            }
        }
        Value::Sequence(items) if items.is_empty() => vec![format!("{pad}{raw_key}: []{comment}")],
        Value::Mapping(map) if map.is_empty() => vec![format!("{pad}{raw_key}: {{}}{comment}")],
        Value::Sequence(items) => {
            let mut out = vec![format!("{pad}{raw_key}:{comment}")];
            for item in items {
                if is_scalar(item) {
                    out.push(format!("{pad}  - {}", render_scalar(item)));
                } else {
                    let body = serde_yaml::to_string(item).unwrap_or_default();
                    let mut first = true;
                    for line in body.trim_end_matches('\n').lines() {
                        let lead = if first { "- " } else { "  " };
                        first = false;
                        out.push(format!("{pad}  {lead}{line}"));
                    }
                }
            }
            out
        }
        Value::Mapping(map) => {
            let mut out = vec![format!("{pad}{raw_key}:{comment}")];
            for (k, v) in map {
                let key = match k {
                    Value::String(s) => render_key(s),
                    other => render_scalar(other),
                };
                out.extend(render_entry(indent + 2, &key, v, ""));
            }
            out
        }
        Value::Tagged(t) => render_entry(indent, raw_key, &t.value, comment),
        _ => vec![format!("{pad}{raw_key}:{comment}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP: &str = "\
# 默认设置
default:
  groupCD: 500   # 群聊中所有指令操作冷却时间，单位毫秒,0则无限制
  singleCD: 2000 # 群聊中个人操作冷却时间，单位毫秒

  onlyReplyAt: 0 # 是否只仅关注主动提及Bot的消息 0-否 1-是 2-非主人
  botAlias:      # 开启后则只回复提及Bot的消息及特定前缀的消息
    - 云崽
    - 云宝

  disable:       # 禁用功能
    - 禁用示例

# Bot单独设置
114514:default:
  onlyReplyAt: 1

# 群单独设置
123456:
  groupCD: 500
";

    fn v(s: &str) -> Value {
        serde_yaml::from_str(s).unwrap()
    }

    #[test]
    fn scalar_keeps_trailing_comment_and_neighbours() {
        let out = set_value(GROUP, &["default", "groupCD"], &v("1000")).unwrap();
        assert!(out.contains("  groupCD: 1000   # 群聊中所有指令操作冷却时间"));
        assert!(out.contains("  singleCD: 2000 # 群聊中个人操作冷却时间"));
        assert!(out.starts_with("# 默认设置\n"));
        assert_eq!(out.lines().count(), GROUP.lines().count());
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed["default"]["groupCD"], v("1000"));
    }

    #[test]
    fn list_rewrite_keeps_key_comment_and_following_section_comment() {
        let out = set_value(GROUP, &["default", "botAlias"], &v("[小云, 云宝]")).unwrap();
        assert!(out.contains("  botAlias:      # 开启后则只回复提及Bot的消息及特定前缀的消息\n    - 小云\n    - 云宝\n"));
        // 空行和下一个键原样
        assert!(out.contains("    - 云宝\n\n  disable:"));
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed["default"]["botAlias"], v("[小云, 云宝]"));
        assert_eq!(parsed["default"]["disable"], v("[禁用示例]"));
    }

    #[test]
    fn keys_with_colons_are_matched_whole() {
        let out = set_value(GROUP, &["114514:default", "onlyReplyAt"], &v("2")).unwrap();
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed["114514:default"]["onlyReplyAt"], v("2"));
        assert_eq!(parsed["default"]["onlyReplyAt"], v("0"));
        assert!(out.contains("# 群单独设置\n123456:"), "下一段的说明注释留在原处");
    }

    #[test]
    fn missing_keys_and_levels_are_appended_in_place() {
        let out = set_value(GROUP, &["default", "addRecall"], &v("30")).unwrap();
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed["default"]["addRecall"], v("30"));
        assert!(out.contains("    - 禁用示例\n  addRecall: 30\n\n# Bot单独设置"));

        let out = set_value(GROUP, &["654321", "singleCD"], &v("100")).unwrap();
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed[654321]["singleCD"], v("100"));
        assert!(out.ends_with("654321:\n  singleCD: 100\n"));
    }

    #[test]
    fn remove_drops_whole_block_but_not_next_comment() {
        let out = remove_key(GROUP, &["114514:default"]).unwrap();
        assert!(!out.contains("114514:default"));
        assert!(out.contains("# Bot单独设置\n\n# 群单独设置\n123456:"));
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert!(parsed.get("114514:default").is_none());
        assert_eq!(remove_key(GROUP, &["nope"]).unwrap(), GROUP);
    }

    #[test]
    fn null_value_turns_into_block_when_children_are_set() {
        let server = "# 服务器鉴权\nauth:\n# Authorization: Bearer <access_token>\nhttps:\n";
        let mut map = serde_yaml::Mapping::new();
        map.insert(v("Authorization"), v("Bearer abc"));
        let out = set_value(server, &["auth"], &Value::Mapping(map)).unwrap();
        assert_eq!(
            out,
            "# 服务器鉴权\nauth:\n  Authorization: Bearer abc\n# Authorization: Bearer <access_token>\nhttps:\n"
        );
        let back = set_value(&out, &["auth"], &Value::Null).unwrap();
        assert_eq!(back, server);
    }

    #[test]
    fn flow_empty_parent_accepts_children() {
        let text = "default: {}\nother: 1\n";
        let out = set_value(text, &["default", "groupCD"], &v("5")).unwrap();
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed["default"]["groupCD"], v("5"));
        assert_eq!(parsed["other"], v("1"));
    }

    #[test]
    fn crlf_and_missing_trailing_newline_are_kept() {
        let text = "port: 2536\r\nurl: http://localhost:2536";
        let out = set_value(text, &["port"], &v("3000")).unwrap();
        assert_eq!(out, "port: 3000\r\nurl: http://localhost:2536");
    }

    #[test]
    fn top_level_list_with_same_indent_items() {
        let text = "masterQQ:\n- \"stdin\"\n# 注释\nmaster:\n  - \"stdin:stdin\"\n";
        let out = set_value(text, &["masterQQ"], &v("[stdin, 10001]")).unwrap();
        assert_eq!(out, "masterQQ:\n  - stdin\n  - 10001\n# 注释\nmaster:\n  - \"stdin:stdin\"\n");
    }

    #[test]
    fn special_keys_and_strings_are_quoted_when_needed() {
        let text = "/→#: true\nchromium_path:\n";
        let out = set_value(text, &["/→#"], &v("false")).unwrap();
        assert!(out.starts_with("/→#: false\n"));
        let out = set_value(&out, &["chromium_path"], &Value::String("C:\\Program Files\\Edge\\msedge.exe".into())).unwrap();
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed["chromium_path"], Value::String("C:\\Program Files\\Edge\\msedge.exe".into()));
        let out = set_value("a: 1\n", &["msg"], &Value::String("私聊: 已禁用 # 真的".into())).unwrap();
        let parsed: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(parsed["msg"], Value::String("私聊: 已禁用 # 真的".into()));
    }

    #[test]
    fn empty_document_gets_first_key() {
        assert_eq!(set_value("", &["port"], &v("1")).unwrap(), "port: 1\n");
    }
}

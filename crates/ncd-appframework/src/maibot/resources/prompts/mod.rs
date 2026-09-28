//! 提示词模板。默认在 `prompts/<语言>/<名>.prompt`，用户改过的另存到 `data/custom_prompts/<语言>/`，
//! 麦麦运行时只读覆盖那份。跑着走 WebUI 接口：上游改完会清它自己的缓存，直接改盘上文件它可能一直读旧的；
//! 停着直接改盘上文件，版本记录的格式和上游一样，两种方式换着用不丢版本。

mod disk;
mod live;

use std::collections::BTreeSet;

use ncd_domain::AppInstance;
use ncd_host::Host;
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::maibot::api::MaiBotSession;

/// 麦麦在用的语言。上游只认环境变量 `MAIBOT_LOCALE`，桌面端启动时不设，它就用 zh-CN
pub const ACTIVE_LANGUAGE: &str = "zh-CN";

/// 目录里的一个模板
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPromptInfo {
    /// 文件名，带 `.prompt`
    pub name: String,
    /// 上游元信息里的中文名；没写就是空串，界面拿文件名顶
    pub display_name: String,
    pub description: String,
    pub advanced: bool,
    /// 改过：`data/custom_prompts` 下有这个模板的覆盖
    pub customized: bool,
    pub version_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPromptLanguage {
    pub language: String,
    pub prompts: Vec<MaiBotPromptInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPromptCatalog {
    pub languages: Vec<MaiBotPromptLanguage>,
    pub active_language: String,
    /// 麦麦在跑：改完立刻生效；停着改的下次启动生效
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPromptVersion {
    pub id: String,
    pub label: String,
    /// 秒级时间戳。停着时从版本清单读，旧格式的覆盖没有记录就是 0
    pub created_at: f64,
    pub modified_at: f64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPromptFile {
    pub language: String,
    pub name: String,
    /// 在用的内容：改过就是覆盖，没改过就是默认
    pub content: String,
    /// 默认内容一起给：对比、占位符校验都要它，省一次来回
    pub default_content: String,
    pub customized: bool,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_version_id: Option<String>,
    /// 新的在前
    pub versions: Vec<MaiBotPromptVersion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotPromptAction {
    /// 存成在用的。`version_id` 给了就覆盖那个版本，不给就新建一个；`label` 空着用上游的默认名
    Save {
        language: String,
        name: String,
        content: String,
        label: String,
        version_id: Option<String>,
    },
    /// 换成某个版本的内容
    Activate {
        language: String,
        name: String,
        version_id: String,
    },
    DeleteVersion {
        language: String,
        name: String,
        version_id: String,
    },
    /// 恢复默认：删掉覆盖，版本都留着
    Restore { language: String, name: String },
}

impl MaiBotPromptAction {
    fn target(&self) -> (&str, &str) {
        match self {
            Self::Save { language, name, .. }
            | Self::Activate { language, name, .. }
            | Self::DeleteVersion { language, name, .. }
            | Self::Restore { language, name } => (language, name),
        }
    }
}

/// 在哪改：跑着走接口，停着改盘上文件
pub enum MaiBotPromptTarget<'a> {
    Live(&'a MaiBotSession),
    Disk {
        host: &'a dyn Host,
        instance: &'a AppInstance,
    },
}

pub async fn catalog(t: MaiBotPromptTarget<'_>) -> Result<MaiBotPromptCatalog, AppFrameworkError> {
    match t {
        MaiBotPromptTarget::Live(s) => live::catalog(s).await,
        MaiBotPromptTarget::Disk { host, instance } => disk::catalog(host, instance).await,
    }
}

pub async fn file(
    t: MaiBotPromptTarget<'_>,
    language: &str,
    name: &str,
) -> Result<MaiBotPromptFile, AppFrameworkError> {
    check_names(language, name)?;
    match t {
        MaiBotPromptTarget::Live(s) => live::file(s, language, name).await,
        MaiBotPromptTarget::Disk { host, instance } => {
            disk::file(host, instance, language, name).await
        }
    }
}

/// 某个版本的内容，预览用
pub async fn version(
    t: MaiBotPromptTarget<'_>,
    language: &str,
    name: &str,
    version_id: &str,
) -> Result<String, AppFrameworkError> {
    check_names(language, name)?;
    check_version_id(version_id)?;
    match t {
        MaiBotPromptTarget::Live(s) => live::version(s, language, name, version_id).await,
        MaiBotPromptTarget::Disk { host, instance } => {
            disk::version(host, instance, language, name, version_id).await
        }
    }
}

pub async fn act(
    t: MaiBotPromptTarget<'_>,
    action: &MaiBotPromptAction,
) -> Result<MaiBotPromptFile, AppFrameworkError> {
    let (language, name) = action.target();
    check_names(language, name)?;
    match action {
        MaiBotPromptAction::Save {
            version_id: Some(id),
            ..
        }
        | MaiBotPromptAction::Activate { version_id: id, .. }
        | MaiBotPromptAction::DeleteVersion { version_id: id, .. } => check_version_id(id)?,
        _ => {}
    }
    match t {
        MaiBotPromptTarget::Live(s) => live::act(s, action).await,
        MaiBotPromptTarget::Disk { host, instance } => disk::act(host, instance, action).await,
    }
}

// 上游自己也拦，但停着改盘时没有上游；拼进路径前先挡住 `..` 和分隔符
fn check_names(language: &str, name: &str) -> Result<(), AppFrameworkError> {
    let lang_ok = !language.is_empty()
        && language.len() <= 32
        && language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let stem = name.strip_suffix(".prompt").unwrap_or("");
    let name_ok = !stem.is_empty()
        && name.len() <= 128
        && !stem.contains("..")
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if lang_ok && name_ok {
        Ok(())
    } else {
        Err(AppFrameworkError::Validation(format!(
            "提示词名不对：{language}/{name}"
        )))
    }
}

fn check_version_id(id: &str) -> Result<(), AppFrameworkError> {
    let ok = !id.is_empty()
        && id != "."
        && id != ".."
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(AppFrameworkError::Validation(format!("版本号不对：{id}")))
    }
}

/// 照 Python `string.Formatter.parse` 取出每个 `{…}` 的参数名：`{a.b}`、`{a[0]}`、`{a!r}`、`{a:>5}` 都算 a，
/// `{{` `}}` 是字面括号。`{}` 这种没名字的原样留成空串，由调用方决定怎么报
pub fn prompt_fields(text: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' if chars.get(i + 1) == Some(&'{') => i += 2,
            '}' if chars.get(i + 1) == Some(&'}') => i += 2,
            '}' => return Err("有一个单独的 }，字面的花括号要写成 }}".into()),
            '{' => {
                // 格式说明里还能再套 {}，按层数找配对的 }
                let mut depth = 1;
                let mut j = i + 1;
                while j < chars.len() {
                    match chars[j] {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if j >= chars.len() {
                    return Err("有一个 { 没有配对的 }，字面的花括号要写成 {{".into());
                }
                let field: String = chars[i + 1..j].iter().collect();
                out.push(field_base(&field)?);
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    Ok(out)
}

fn field_base(field: &str) -> Result<String, String> {
    let mut end = field.len();
    let mut in_bracket = false;
    for (idx, c) in field.char_indices() {
        match c {
            '[' if !in_bracket => in_bracket = true,
            ']' if in_bracket => in_bracket = false,
            '{' if !in_bracket => return Err(format!("参数名里不能再有 {{：{{{field}}}")),
            ':' | '!' if !in_bracket => {
                end = idx;
                break;
            }
            _ => {}
        }
    }
    let name = &field[..end];
    Ok(name.split(['.', '[']).next().unwrap_or("").to_string())
}

/// 存之前的检查，和上游一致再多两条：上游只比占位符，空模板和 `{}` 它放过去了，
/// 但它加载模板时会因为这两样整个失败，麦麦连启动都起不来
pub fn check_prompt(content: &str, default_content: &str) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("提示词不能是空的".into());
    }
    let fields = prompt_fields(content)?;
    if fields.iter().any(String::is_empty) {
        return Err("不能有空的 {}：要写参数名，字面的花括号写成 {{ }}".into());
    }
    let want: BTreeSet<String> = prompt_fields(default_content)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| !f.is_empty())
        .collect();
    let got: BTreeSet<String> = fields.into_iter().collect();
    let missing: Vec<String> = want.difference(&got).map(|f| format!("{{{f}}}")).collect();
    let extra: Vec<String> = got.difference(&want).map(|f| format!("{{{f}}}")).collect();
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("少了 {}", missing.join("、")));
    }
    if !extra.is_empty() {
        parts.push(format!("多了 {}（麦麦给不出这些参数）", extra.join("、")));
    }
    if parts.is_empty() {
        Ok(())
    } else {
        Err(parts.join("；"))
    }
}

#[cfg(test)]
mod tests;

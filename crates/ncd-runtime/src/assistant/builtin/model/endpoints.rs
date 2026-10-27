//! 各 API 格式共享地址校验，保留用户指定的完整接口路径。

use ncd_domain::assistant::AssistantApiFormat;
use reqwest::Url;

use super::{ModelError, is_loopback};

#[cfg(test)]
pub(in crate::assistant) fn endpoint_url(base: &str) -> Result<Url, ModelError> {
    endpoint_url_for_format(base, AssistantApiFormat::OpenAiChatCompletions)
}

pub(in crate::assistant) fn endpoint_url_for_format(
    base: &str,
    format: AssistantApiFormat,
) -> Result<Url, ModelError> {
    let input = base.trim();
    if input.is_empty()
        || input.len() > 4096
        || base.chars().any(char::is_control)
        || input.contains('\\')
    {
        return Err(ModelError::InvalidEndpoint);
    }
    let authority = input
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    let mut url = Url::parse(input).map_err(|_| ModelError::InvalidEndpoint)?;
    if authority.is_none_or(|value| value.contains('@'))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.has_host()
        || (url.scheme() != "https" && !(url.scheme() == "http" && is_loopback(&url)))
    {
        return Err(ModelError::InvalidEndpoint);
    }
    let path = url.path().trim_end_matches('/');
    let suffix = match format {
        AssistantApiFormat::OpenAiChatCompletions => "chat/completions",
        AssistantApiFormat::OpenAiResponses => "responses",
        AssistantApiFormat::AnthropicMessages => "messages",
        AssistantApiFormat::GeminiGenerateContent => {
            let path = if path.is_empty() { "/v1beta" } else { path }.to_owned();
            url.set_path(&path);
            return Ok(url);
        }
    };
    let path = if path.is_empty() {
        format!("/v1/{suffix}")
    } else if path.ends_with("/v1") {
        format!("{path}/{suffix}")
    } else {
        path.to_owned()
    };
    url.set_path(&path);
    Ok(url)
}

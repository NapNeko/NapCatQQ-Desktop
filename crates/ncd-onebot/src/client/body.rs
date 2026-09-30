//! 有上限地读完一个 HTTP 响应体。
//!
//! 上游回包原样进内存（还要再解析一遍），一个出了毛病的上游或者被劫持的隧道回一个几 GB 的
//! 响应，`text()` 会照单全收。调试台用到的三处（OneBot HTTP、NapCat / SnowLuma 调试接口）
//! 都走这里，同一条上限，读到超出就停手。

/// 单个响应体的上限。合法的大回包（群成员列表、合并转发）也远到不了这个量级
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum BodyError {
    #[error("响应超过 {} MiB，没有读完", .limit / (1024 * 1024))]
    TooLarge { limit: usize },
    #[error("读取响应失败：{0}")]
    Read(#[source] reqwest::Error),
}

/// 读完响应体，超过 `limit` 字节就报 [`BodyError::TooLarge`]。声明了 `Content-Length`
/// 且已超出的，不读直接拒绝
pub async fn read_body_limited(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, BodyError> {
    let too_large = BodyError::TooLarge { limit };
    if response
        .content_length()
        .is_some_and(|len| usize::try_from(len).map_or(true, |len| len > limit))
    {
        return Err(too_large);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(BodyError::Read)? {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(too_large);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// 同上，按 UTF-8 解成文本；非法字节换成替换字符（与 reqwest 的 `text()` 一致）
pub async fn read_text_limited(
    response: reqwest::Response,
    limit: usize,
) -> Result<String, BodyError> {
    let bytes = read_body_limited(response, limit).await?;
    Ok(String::from_utf8(bytes)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned()))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// 服务端要活到响应体读完，所以一起交回去
    async fn get(body: &str) -> (MockServer, reqwest::Response) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(server.uri())
            .send()
            .await
            .unwrap();
        (server, response)
    }

    #[tokio::test]
    async fn bodies_within_the_limit_are_read_in_full() {
        let (_server, response) = get("你好，世界").await;
        assert_eq!(read_text_limited(response, 64).await.unwrap(), "你好，世界");
        // 恰好等于上限也行
        let (_server, response) = get("abcd").await;
        assert_eq!(read_body_limited(response, 4).await.unwrap(), b"abcd");
    }

    #[tokio::test]
    async fn bodies_over_the_limit_are_refused() {
        let (_server, response) = get("abcde").await;
        let err = read_body_limited(response, 4).await.unwrap_err();
        assert!(matches!(err, BodyError::TooLarge { limit: 4 }), "{err:?}");
        assert!(
            BodyError::TooLarge {
                limit: MAX_BODY_BYTES
            }
            .to_string()
            .contains("64 MiB")
        );
    }
}

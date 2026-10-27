use crate::Headers;
use crate::webc::{Error, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, RequestBuilder, StatusCode};
use serde_json::Value;

/// A simple reqwest client wrapper for this library.
/// Here are some good default
/// ```rust
/// let reqwest_client = reqwest::Client::builder()
///    .tcp_nodelay(true)
///    .gzip(true)
///    .pool_max_idle_per_host(4)
///    .http2_keep_alive_interval(Some(Duration::from_secs(20)))
///    .http2_keep_alive_timeout(Duration::from_secs(10))
///    .http2_keep_alive_while_idle(true)
///    .http2_adaptive_window(true)
///    .build()
///    .map_err(Error::Reqwest)?;
/// ```
#[derive(Debug)]
pub struct WebClient {
	reqwest_client: reqwest::Client,
}

// region:    --- Constructors

impl WebClient {
	pub fn from_reqwest_client(reqwest_client: reqwest::Client) -> Self {
		WebClient { reqwest_client }
	}
}

// endregion: --- Constructors

// region:    --- Web Method Implementation

impl WebClient {
	pub async fn do_get(&self, url: &str, headers: &Headers) -> Result<WebResponse> {
		let reqwest_builder = apply_headers(self.reqwest_client.request(Method::GET, url), headers)?;
		let reqwest_res = reqwest_builder.send().await?;

		let response = WebResponse::from_reqwest_response(reqwest_res).await?;

		Ok(response)
	}

	pub async fn do_post(&self, url: &str, headers: &Headers, content: &Value) -> Result<WebResponse> {
		let reqwest_builder = self.new_req_builder(url, headers, content)?;

		let reqwest_res = reqwest_builder.send().await?;

		let response = WebResponse::from_reqwest_response(reqwest_res).await?;

		Ok(response)
	}

	pub fn new_req_builder(&self, url: &str, headers: &Headers, content: &Value) -> Result<RequestBuilder> {
		let method = Method::POST;

		let mut reqwest_builder = apply_headers(self.reqwest_client.request(method, url), headers)?;
		reqwest_builder = reqwest_builder.json(content);

		Ok(reqwest_builder)
	}
}

fn apply_headers(mut request: RequestBuilder, headers: &Headers) -> Result<RequestBuilder> {
	for (name, value) in headers.iter() {
		let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| Error::RequestHeadersInvalid)?;
		let mut value = HeaderValue::from_str(value).map_err(|_| Error::RequestHeadersInvalid)?;
		if matches!(
			name.as_str(),
			"authorization" | "proxy-authorization" | "x-api-key" | "x-goog-api-key"
		) {
			value.set_sensitive(true);
		}
		request = request.header(name, value);
	}
	Ok(request)
}

#[cfg(test)]
mod header_tests {
	use super::*;

	#[test]
	fn credentials_remain_sensitive_when_building_get_and_post_requests() {
		let headers = Headers::from([
			("Authorization", "Bearer fixture-auth-secret"),
			("X-Api-Key", "fixture-anthropic-secret"),
			("X-Goog-Api-Key", "fixture-gemini-secret"),
		]);
		let client = WebClient::from_reqwest_client(reqwest::Client::new());
		let post = client
			.new_req_builder(
				"https://example.test/v1/chat/completions",
				&headers,
				&serde_json::json!({}),
			)
			.unwrap()
			.build()
			.unwrap();
		let get = apply_headers(reqwest::Client::new().get("https://example.test/v1/models"), &headers)
			.unwrap()
			.build()
			.unwrap();
		for request in [post, get] {
			for name in ["authorization", "x-api-key", "x-goog-api-key"] {
				assert!(request.headers()[name].is_sensitive());
			}
			let debug = format!("{request:?}");
			assert!(!debug.contains("fixture-auth-secret"));
			assert!(!debug.contains("fixture-anthropic-secret"));
			assert!(!debug.contains("fixture-gemini-secret"));
		}
	}

	#[test]
	fn invalid_header_errors_never_echo_the_value() {
		let headers = Headers::from(("x-api-key", "fixture-secret\ninvalid"));
		let error = apply_headers(reqwest::Client::new().get("https://example.test"), &headers)
			.err()
			.unwrap();
		assert_eq!(error.to_string(), "Request headers are invalid");
		assert!(!format!("{error:?}").contains("fixture-secret"));
	}
}
// endregion: --- Web Method Implementation

// region:    --- WebResponse

// NOTE: This is not a non-streaming web response (assumed to be JSON for this library).
//       Streaming is handled with event-source or custom streams (for example, for Cohere).

#[derive(Debug)]
pub struct WebResponse {
	#[allow(unused)]
	pub status: StatusCode,
	pub body: Value,
}

impl WebResponse {
	/// Note 1: For now, assume only a JSON response.
	/// Note 2: Currently, the WebResponse holds a Value (parsed from the entire body), and then the caller
	///         can cherry-pick/deserialize further. In the future, we might consider returning `body: String`
	///         to enable more optimized parsing, allowing for selective parsing constrained by the structure.
	pub(crate) async fn from_reqwest_response(mut res: reqwest::Response) -> Result<WebResponse> {
		let status = res.status();

		if !status.is_success() {
			let headers = res.headers().clone();
			let body = res.text().await?;
			return Err(Error::ResponseFailedStatus {
				status,
				body,
				headers: Box::new(headers),
			});
		}

		// Move the headers into a new HeaderMap
		let headers = res.headers_mut().drain().filter_map(|(n, v)| n.map(|n| (n, v)));
		let header_map = HeaderMap::from_iter(headers);

		// Capture the body
		let ct = header_map.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default();
		let body = res.text().await?;

		let body = if ct.starts_with("application/json") {
			let value: Value = serde_json::from_str(&body).map_err(|err| Error::ResponseFailedInvalidJson {
				body,
				cause: err.to_string(),
			})?;
			value
		} else {
			return Err(Error::ResponseFailedNotJson {
				content_type: ct.to_string(),
				body,
			});
		};

		Ok(WebResponse { status, body })
	}
}

// endregion: --- WebResponse

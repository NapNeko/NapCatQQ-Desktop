use serde_json::Value;

/// Application-owned limits and protocol checks, applied before SDK buffering or parsing.
/// Errors should contain fixed descriptions rather than provider data or credentials.
pub trait ChatStreamGuard: Send + Sync + std::fmt::Debug {
	fn max_response_bytes(&self) -> usize {
		8 * 1024 * 1024
	}

	fn max_frame_bytes(&self) -> usize {
		256 * 1024
	}

	fn validate_request(&self, _payload: &Value) -> crate::Result<()> {
		Ok(())
	}

	fn validate_content_type(&self, _content_type: &str) -> crate::Result<()> {
		Ok(())
	}

	fn validate_frame(&self, _event: Option<&str>, _data: &str) -> crate::Result<()> {
		Ok(())
	}

	fn validate_eof(&self) -> crate::Result<()> {
		Ok(())
	}
}

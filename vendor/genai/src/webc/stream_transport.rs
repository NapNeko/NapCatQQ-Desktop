//! Deadlines around HTTP response headers and each raw body read.
//! Parsing/heartbeats must never change the meaning of transport progress.
use crate::chat::ChatStreamGuard;
use crate::error::BoxError;
use bytes::Bytes;
use futures::{StreamExt, stream::BoxStream};
use std::{fmt, sync::Arc, time::Duration};

/// The transport phase that exceeded its configured streaming timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamTimeout {
	/// The request did not receive response headers in time.
	Headers,
	/// No raw response body chunk arrived in time.
	Read,
}
impl fmt::Display for StreamTimeout {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(match self {
			Self::Headers => "model response headers timed out",
			Self::Read => "model response body read timed out",
		})
	}
}
impl std::error::Error for StreamTimeout {}

pub(crate) struct StreamRequest {
	request: reqwest::RequestBuilder,
	header_timeout: Option<Duration>,
	read_timeout: Option<Duration>,
	guard: Option<Arc<dyn ChatStreamGuard>>,
}
impl StreamRequest {
	pub(crate) fn new(
		request: reqwest::RequestBuilder,
		header_timeout: Option<Duration>,
		read_timeout: Option<Duration>,
	) -> Self {
		Self {
			request,
			header_timeout,
			read_timeout,
			guard: None,
		}
	}
	pub(crate) fn with_guard(mut self, guard: Option<Arc<dyn ChatStreamGuard>>) -> Self {
		self.guard = guard;
		self
	}
	pub(crate) fn guard(&self) -> Option<Arc<dyn ChatStreamGuard>> {
		self.guard.clone()
	}
	pub(crate) async fn send(self) -> Result<StreamResponse, BoxError> {
		let response = match self.header_timeout {
			Some(timeout) => tokio::time::timeout(timeout, self.request.send())
				.await
				.map_err(|_| Box::new(StreamTimeout::Headers) as BoxError)?,
			None => self.request.send().await,
		}
		.map_err(|error| Box::new(error) as BoxError)?;
		Ok(StreamResponse {
			response,
			read_timeout: self.read_timeout,
			guard: self.guard,
		})
	}
}

pub(crate) struct StreamResponse {
	response: reqwest::Response,
	read_timeout: Option<Duration>,
	guard: Option<Arc<dyn ChatStreamGuard>>,
}
impl StreamResponse {
	pub(crate) fn headers(&self) -> &reqwest::header::HeaderMap {
		self.response.headers()
	}
	pub(crate) fn status(&self) -> reqwest::StatusCode {
		self.response.status()
	}
	pub(crate) fn bytes_stream(self) -> BoxStream<'static, Result<Bytes, BoxError>> {
		let timeout = self.read_timeout;
		let max_bytes = self.guard.as_ref().map(|guard| guard.max_response_bytes());
		let stream = self.response.bytes_stream().boxed();
		futures::stream::unfold(Some((stream, 0usize)), move |state| async move {
			let (mut stream, total) = state?;
			let next = match timeout {
				Some(timeout) => match tokio::time::timeout(timeout, stream.next()).await {
					Ok(next) => next,
					Err(_) => return Some((Err(Box::new(StreamTimeout::Read) as BoxError), None)),
				},
				None => stream.next().await,
			};
			match next {
				Some(Ok(bytes)) => {
					let next_total = total.checked_add(bytes.len());
					if max_bytes.is_some_and(|limit| next_total.is_none_or(|next| next > limit)) {
						return Some((
							Err(Box::new(crate::Error::StreamGuardRejected {
								reason: "response byte limit exceeded",
							}) as BoxError),
							None,
						));
					}
					Some((Ok(bytes), Some((stream, next_total.unwrap_or(usize::MAX)))))
				}
				Some(Err(e)) => Some((Err(Box::new(e) as BoxError), None)),
				None => None,
			}
		})
		.boxed()
	}
	pub(crate) async fn text(self) -> Result<String, BoxError> {
		// Guarded applications never consume or retain provider error bodies.
		if self.guard.is_some() {
			return Ok(String::new());
		}
		if self.read_timeout.is_none() {
			return self.response.text().await.map_err(|error| Box::new(error) as BoxError);
		}
		let mut stream = self.bytes_stream();
		let mut bytes = Vec::new();
		while let Some(chunk) = stream.next().await {
			bytes.extend_from_slice(&chunk?);
		}
		Ok(String::from_utf8_lossy(&bytes).into_owned())
	}
}

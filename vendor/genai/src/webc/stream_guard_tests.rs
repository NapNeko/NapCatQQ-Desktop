use super::*;
use crate::chat::{ChatStreamGuard, CollectorSink};
use crate::{Error, Result};
use futures::StreamExt;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug)]
struct Guard {
	response_bytes: usize,
	frame_bytes: usize,
	reject_frames: bool,
	reject_eof: bool,
}

impl Default for Guard {
	fn default() -> Self {
		Self {
			response_bytes: 1024,
			frame_bytes: 512,
			reject_frames: false,
			reject_eof: false,
		}
	}
}

impl ChatStreamGuard for Guard {
	fn max_response_bytes(&self) -> usize {
		self.response_bytes
	}
	fn max_frame_bytes(&self) -> usize {
		self.frame_bytes
	}
	fn validate_content_type(&self, value: &str) -> Result<()> {
		if value.starts_with("text/event-stream") {
			Ok(())
		} else {
			Err(Error::StreamGuardRejected {
				reason: "unexpected content type",
			})
		}
	}
	fn validate_frame(&self, _event: Option<&str>, _data: &str) -> Result<()> {
		if self.reject_frames {
			Err(Error::StreamGuardRejected {
				reason: "invalid provider frame",
			})
		} else {
			Ok(())
		}
	}
	fn validate_eof(&self) -> Result<()> {
		if self.reject_eof {
			Err(Error::StreamGuardRejected {
				reason: "missing terminal frame",
			})
		} else {
			Ok(())
		}
	}
}

async fn serve(status: &str, content_type: &str, body: &str) -> String {
	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	let response = format!(
		"HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
		body.len()
	);
	tokio::spawn(async move {
		let (mut socket, _) = listener.accept().await.unwrap();
		let mut request = [0u8; 4096];
		let _ = socket.read(&mut request).await;
		let _ = socket.write_all(response.as_bytes()).await;
	});
	format!("http://{address}")
}

fn request(url: &str, guard: Guard) -> StreamRequest {
	StreamRequest::new(reqwest::Client::new().post(url), None, None).with_guard(Some(Arc::new(guard)))
}

#[tokio::test]
async fn guarded_http_error_never_retains_provider_body() {
	let marker = "provider-error-may-echo-api-key";
	let url = serve("400 Bad Request", "application/json", marker).await;
	let mut stream = WebStream::new_with_sse(request(&url, Guard::default()));
	let error = stream.next().await.unwrap().unwrap_err();
	let error = error.downcast::<Error>().unwrap();
	match *error {
		Error::HttpError { status, body, .. } => {
			assert_eq!(status.as_u16(), 400);
			assert!(body.is_empty());
		}
		_ => panic!("expected HTTP status error"),
	}
}

#[tokio::test]
async fn guarded_response_rejects_total_byte_limit_before_frame_callback() {
	let url = serve("200 OK", "text/event-stream", &"data: 123\n\n".repeat(20)).await;
	let mut stream = WebStream::new_with_sse(request(
		&url,
		Guard {
			response_bytes: 10,
			..Guard::default()
		},
	));
	let error = stream.next().await.unwrap().unwrap_err();
	assert!(error.to_string().contains("response byte limit exceeded"));
}

#[tokio::test]
async fn guarded_response_rejects_frame_limit_before_parser() {
	let url = serve("200 OK", "text/event-stream", "data: 123456789\n\n").await;
	let mut stream = WebStream::new_with_sse(request(
		&url,
		Guard {
			frame_bytes: 8,
			..Guard::default()
		},
	));
	let error = stream.next().await.unwrap().unwrap_err();
	assert!(error.to_string().contains("frame byte limit exceeded"));
}

#[tokio::test]
async fn frame_rejection_precedes_observers_and_adapter_parsing() {
	let url = serve(
		"200 OK",
		"text/event-stream",
		"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":4294967295}]}}]}\n\n",
	)
	.await;
	let sink = Arc::new(CollectorSink::new());
	let frame_tap = FrameTap::new(
		sink.clone(),
		crate::chat::FrameCtx::new(crate::ModelIden::new(crate::adapter::AdapterKind::OpenAI, "test")),
	);
	let mut stream = EventSourceStream::new(request(
		&url,
		Guard {
			reject_frames: true,
			..Guard::default()
		},
	))
	.with_frame_tap(Some(frame_tap));
	assert!(matches!(stream.next().await, Some(Ok(Event::Open))));
	let error = stream.next().await.unwrap().unwrap_err();
	assert!(error.to_string().contains("invalid provider frame"));
	assert!(sink.is_empty());
	assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn guard_rejects_json_content_type_and_incomplete_eof() {
	let url = serve("200 OK", "application/json", "{}").await;
	let mut stream = WebStream::new_with_sse(request(&url, Guard::default()));
	assert!(
		stream
			.next()
			.await
			.unwrap()
			.unwrap_err()
			.to_string()
			.contains("unexpected content type")
	);
	let url = serve("200 OK", "text/event-stream", "data: {}\n\n").await;
	let mut stream = WebStream::new_with_sse(request(
		&url,
		Guard {
			reject_eof: true,
			..Guard::default()
		},
	));
	assert!(stream.next().await.unwrap().is_ok());
	assert!(
		stream
			.next()
			.await
			.unwrap()
			.unwrap_err()
			.to_string()
			.contains("missing terminal frame")
	);
}

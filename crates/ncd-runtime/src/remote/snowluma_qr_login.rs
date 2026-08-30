use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::{
    SnowlumaQrFailureCategory, SnowlumaQrLoginResult, SnowlumaQrLoginSession,
};
use ncd_host::{Host, HostCommand, HostPath};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const BASE_WIDTH: u32 = 320;
const BASE_HEIGHT: u32 = 460;
const CAPTURE_SCRIPT: &str = r#"
import ctypes, os, sys, time
from PIL import ImageGrab

display, path = sys.argv[1], sys.argv[2]
x, y, width, height = (int(value) for value in sys.argv[3:7])
os.environ['DISPLAY'] = display
x11 = ctypes.CDLL('libX11.so.6')
xtst = ctypes.CDLL('libXtst.so.6')
display_handle = x11.XOpenDisplay(display.encode())
if not display_handle:
    raise RuntimeError('x11 display unavailable')
try:
    for click_x, click_y in ((width // 2, height * 280 // 460), (width * 120 // 320, height * 424 // 460), (width * 120 // 320, height * 422 // 460)):
        xtst.XTestFakeMotionEvent(display_handle, 0, x + click_x, y + click_y, 0)
        xtst.XTestFakeButtonEvent(display_handle, 1, 1, 0)
        xtst.XTestFakeButtonEvent(display_handle, 1, 0, 0)
        x11.XFlush(display_handle)
        time.sleep(1.0)
    image = ImageGrab.grab(bbox=(x, y, x + width, y + height))
    image.save(path, 'PNG')
finally:
    x11.XCloseDisplay(display_handle)
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QrLoginPoint {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QrLoginWindowGeometry {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum QrCaptureError {
    #[error("unsupported display")]
    UnsupportedDisplay,
    #[error("unsupported window geometry")]
    UnsupportedGeometry,
    #[error("unsafe remote capture path")]
    UnsafePath,
}

pub fn validate_display(display: &str) -> Result<(), QrCaptureError> {
    let valid = display.len() >= 2
        && display.starts_with(':')
        && display[1..].chars().all(|ch| ch.is_ascii_digit());
    valid.then_some(()).ok_or(QrCaptureError::UnsupportedDisplay)
}

pub fn calibrated_click_points(
    geometry: QrLoginWindowGeometry,
) -> Result<Vec<QrLoginPoint>, QrCaptureError> {
    if geometry.width == 0
        || geometry.height == 0
        || geometry.width < BASE_WIDTH / 2
        || geometry.height < BASE_HEIGHT / 2
    {
        return Err(QrCaptureError::UnsupportedGeometry);
    }
    let point = |x: u32, y: u32| QrLoginPoint {
        x: geometry.x + x.saturating_mul(geometry.width) / BASE_WIDTH,
        y: geometry.y + y.saturating_mul(geometry.height) / BASE_HEIGHT,
    };
    Ok(vec![point(BASE_WIDTH / 2, 280), point(120, 424), point(120, 422)])
}

fn validate_remote_path(path: &HostPath) -> Result<(), QrCaptureError> {
    let value = path.as_posix();
    if !value.starts_with('/')
        || value.contains("..")
        || value.chars().any(|ch| ch == '\n' || ch == '\r' || ch == '\0')
    {
        return Err(QrCaptureError::UnsafePath);
    }
    Ok(())
}

pub trait QrDecoder: Send + Sync {
    fn decode(&self, frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory>;
}

pub struct UnavailableQrDecoder;

impl QrDecoder for UnavailableQrDecoder {
    fn decode(&self, _frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory> {
        Err(SnowlumaQrFailureCategory::DecoderUnavailable)
    }
}

pub struct SnowlumaQrCaptureRequest {
    pub session: SnowlumaQrLoginSession,
    pub display: String,
    pub geometry: QrLoginWindowGeometry,
    pub remote_png: HostPath,
    pub cancel: Option<CancellationToken>,
}

pub struct SnowlumaQrCaptureService {
    decoder: Arc<dyn QrDecoder>,
}

impl SnowlumaQrCaptureService {
    pub fn new(decoder: Arc<dyn QrDecoder>) -> Self {
        Self { decoder }
    }

    pub async fn capture_and_decode(
        &self,
        host: &dyn Host,
        request: SnowlumaQrCaptureRequest,
    ) -> SnowlumaQrLoginResult {
        let fallback = |reason| SnowlumaQrLoginResult::FallbackNoVnc {
            session: request.session.clone(),
            reason,
        };
        if validate_display(&request.display).is_err()
            || calibrated_click_points(request.geometry).is_err()
            || validate_remote_path(&request.remote_png).is_err()
        {
            return fallback(SnowlumaQrFailureCategory::UnsupportedVariant);
        }

        let mut command = HostCommand::new("python3")
            .arg("-c")
            .arg(CAPTURE_SCRIPT)
            .arg(&request.display)
            .arg(request.remote_png.as_posix())
            .args([
                request.geometry.x.to_string(),
                request.geometry.y.to_string(),
                request.geometry.width.to_string(),
                request.geometry.height.to_string(),
            ])
            .timeout(Duration::from_secs(12));
        if let Some(cancel) = request.cancel.clone() {
            command = command.cancel_token(cancel);
        }
        let output = match host.run_to_string(command).await {
            Ok(output) if output.success() => output,
            _ => {
                let _ = host.remove_file(&request.remote_png).await;
                return fallback(SnowlumaQrFailureCategory::CaptureFailed);
            }
        };
        let _ = output;

        let local_path = temporary_frame_path();
        let download_result = host.download(&request.remote_png, &local_path).await;
        let frame = match download_result {
            Ok(()) => tokio::fs::read(&local_path).await.ok(),
            Err(_) => None,
        };
        let _ = tokio::fs::remove_file(&local_path).await;
        let _ = host.remove_file(&request.remote_png).await;
        let Some(frame) = frame else {
            return fallback(SnowlumaQrFailureCategory::CaptureFailed);
        };

        match self.decoder.decode(&frame) {
            Ok(payload) if valid_payload(&payload) => SnowlumaQrLoginResult::Payload {
                session: request.session,
                payload,
            },
            Ok(_) => fallback(SnowlumaQrFailureCategory::DecodeFailed),
            Err(reason) => fallback(reason),
        }
    }
}

fn temporary_frame_path() -> PathBuf {
    std::env::temp_dir().join(format!("ncd-qr-{}.png", rand::random::<u128>()))
}

fn valid_payload(payload: &str) -> bool {
    !payload.is_empty() && payload.len() <= 4096 && !payload.chars().any(char::is_control)
}

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::{SnowlumaQrFailureCategory, SnowlumaQrLoginResult, SnowlumaQrLoginSession};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

const BASE_WIDTH: u32 = 320;
const BASE_HEIGHT: u32 = 460;
const MAX_FRAME_BYTES: u64 = 4 * 1024 * 1024;
const CAPTURE_SCRIPT: &str = r#"
import ctypes, os, sys, time
from PIL import ImageGrab

display, path = sys.argv[1], sys.argv[2]
x, y, width, height, action = (int(value) for value in sys.argv[3:8])
os.environ['DISPLAY'] = display
x11 = ctypes.CDLL('libX11.so.6')
xtst = ctypes.CDLL('libXtst.so.6')
display_handle = x11.XOpenDisplay(display.encode())
if not display_handle:
    raise RuntimeError('x11 display unavailable')
try:
    points = ((width // 2, height * 280 // 460), (width * 120 // 320, height * 424 // 460), (width * 120 // 320, height * 422 // 460))
    click_x, click_y = points[action]
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
    #[error("ambiguous QQ window")]
    AmbiguousWindow,
}

pub fn validate_display(display: &str) -> Result<(), QrCaptureError> {
    let valid = display.len() >= 2
        && display.starts_with(':')
        && display[1..].chars().all(|ch| ch.is_ascii_digit());
    valid
        .then_some(())
        .ok_or(QrCaptureError::UnsupportedDisplay)
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
    Ok(vec![
        point(BASE_WIDTH / 2, 280),
        point(120, 424),
        point(120, 422),
    ])
}
fn parse_geometry(output: &str) -> Option<QrLoginWindowGeometry> {
    if let Some(geometry_text) = output.lines().find_map(|line| {
        line.split_whitespace().find(|part| {
            let mut parts = part.split(['x', '+']);
            part.contains('x')
                && part.contains('+')
                && parts.next().and_then(|v| v.parse::<u32>().ok()).is_some()
                && parts.next().and_then(|v| v.parse::<u32>().ok()).is_some()
        })
    }) {
        let mut values = geometry_text
            .split(['x', '+'])
            .map(|value| value.parse::<u32>().ok());
        return Some(QrLoginWindowGeometry {
            width: values.next()??,
            height: values.next()??,
            x: values.next()??,
            y: values.next()??,
        });
    }
    let value = |prefix: &str| {
        output
            .lines()
            .find_map(|line| line.trim().strip_prefix(prefix)?.trim().parse::<u32>().ok())
    };
    Some(QrLoginWindowGeometry {
        x: value("Absolute upper-left X:")?,
        y: value("Absolute upper-left Y:")?,
        width: value("Width:")?,
        height: value("Height:")?,
    })
}
fn parse_window_geometry(output: &str) -> Option<(u64, QrLoginWindowGeometry)> {
    let line = output.lines().find(|line| line.contains("\"QQ\""))?;
    let id_text = line
        .split_whitespace()
        .find(|part| part.starts_with("0x"))?;
    let window_id = u64::from_str_radix(id_text.trim_start_matches("0x"), 16).ok()?;
    Some((window_id, parse_geometry(line)?))
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
    pub window_id: u64,
    pub geometry: QrLoginWindowGeometry,
    pub cancel: Option<CancellationToken>,
}

pub struct SnowlumaQrCaptureService {
    decoder: Arc<dyn QrDecoder>,
}

impl SnowlumaQrCaptureService {
    pub fn new(decoder: Arc<dyn QrDecoder>) -> Self {
        Self { decoder }
    }

    pub async fn capture_current_variant(
        &self,
        host: &dyn Host,
        session: SnowlumaQrLoginSession,
    ) -> SnowlumaQrLoginResult {
        let fallback = |reason| SnowlumaQrLoginResult::FallbackNoVnc {
            session: session.clone(),
            reason,
        };
        if host.os() != Os::Linux || host.locality() != Locality::Remote {
            return fallback(SnowlumaQrFailureCategory::UnsupportedVariant);
        }
        let display = match host
            .run_to_string(
                HostCommand::new("printenv")
                    .arg("DISPLAY")
                    .timeout(Duration::from_secs(2)),
            )
            .await
        {
            Ok(output) if output.success() => output.stdout.trim().to_string(),
            _ => return fallback(SnowlumaQrFailureCategory::CapabilityUnavailable),
        };
        if validate_display(&display).is_err() {
            return fallback(SnowlumaQrFailureCategory::CapabilityUnavailable);
        }
        let window = match host
            .run_to_string(
                HostCommand::new("xwininfo")
                    .arg("-root")
                    .arg("-tree")
                    .timeout(Duration::from_secs(3)),
            )
            .await
        {
            Ok(output) if output.success() => parse_window_geometry(&output.stdout),
            _ => None,
        };
        let Some((window_id, geometry)) = window else {
            return fallback(SnowlumaQrFailureCategory::AmbiguousBinding);
        };
        self.capture_and_decode(
            host,
            SnowlumaQrCaptureRequest {
                session,
                display,
                window_id,
                geometry,
                cancel: None,
            },
        )
        .await
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
        {
            return fallback(SnowlumaQrFailureCategory::UnsupportedVariant);
        }
        let remote_dir = temporary_remote_dir(host.id());
        let remote_png = remote_dir.join("frame.png");
        let setup = host
            .run_to_string(
                HostCommand::new("mkdir")
                    .arg("-m")
                    .arg("700")
                    .arg(remote_dir.as_posix()),
            )
            .await;
        if !matches!(setup, Ok(output) if output.success()) {
            return fallback(SnowlumaQrFailureCategory::CapabilityUnavailable);
        }
        let local_path = temporary_frame_path();
        let mut last_frame = None;
        let mut previous_frame: Option<Vec<u8>> = None;
        for action in 0..3 {
            let geometry = match host
                .run_to_string(
                    HostCommand::new("xwininfo")
                        .arg("-id")
                        .arg(format!("0x{:x}", request.window_id))
                        .timeout(Duration::from_secs(3)),
                )
                .await
            {
                Ok(output) if output.success() => parse_geometry(&output.stdout),
                _ => None,
            };
            let Some(geometry) = geometry else {
                let _ = host.remove_dir_all(&remote_dir).await;
                return fallback(SnowlumaQrFailureCategory::AmbiguousBinding);
            };
            let mut command = HostCommand::new("python3")
                .arg("-c")
                .arg(CAPTURE_SCRIPT)
                .arg(&request.display)
                .arg(remote_png.as_posix())
                .args([
                    geometry.x.to_string(),
                    geometry.y.to_string(),
                    geometry.width.to_string(),
                    geometry.height.to_string(),
                    action.to_string(),
                ])
                .timeout(Duration::from_secs(12));
            if let Some(cancel) = request.cancel.clone() {
                command = command.cancel_token(cancel);
            }
            if !matches!(host.run_to_string(command).await, Ok(output) if output.success()) {
                let _ = host.remove_dir_all(&remote_dir).await;
                return fallback(SnowlumaQrFailureCategory::CaptureFailed);
            }
            let downloaded = host.download(&remote_png, &local_path).await.is_ok();
            if downloaded {
                let within_limit = tokio::fs::metadata(&local_path)
                    .await
                    .ok()
                    .is_some_and(|meta| meta.len() <= MAX_FRAME_BYTES);
                if within_limit {
                    let frame = tokio::fs::read(&local_path).await.ok();
                    if frame
                        .as_ref()
                        .is_some_and(|frame| previous_frame.as_ref() == Some(frame))
                    {
                        let _ = tokio::fs::remove_file(&local_path).await;
                        let _ = host.remove_dir_all(&remote_dir).await;
                        return fallback(SnowlumaQrFailureCategory::CaptureFailed);
                    }
                    if let Some(frame) = frame {
                        previous_frame = Some(frame.clone());
                        last_frame = Some(frame);
                    }
                }
            }
            if last_frame.is_none() {
                let _ = tokio::fs::remove_file(&local_path).await;
                let _ = host.remove_dir_all(&remote_dir).await;
                return fallback(SnowlumaQrFailureCategory::CaptureFailed);
            }
        }
        let _ = tokio::fs::remove_file(&local_path).await;
        let _ = host.remove_dir_all(&remote_dir).await;
        let Some(frame) = last_frame else {
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
fn temporary_remote_dir(host_id: &str) -> HostPath {
    let safe_host = host_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect::<String>();
    HostPath::from_posix(format!(
        "/tmp/ncd-qr-{safe_host}-{}",
        rand::random::<u128>()
    ))
}
fn valid_payload(payload: &str) -> bool {
    !payload.is_empty() && payload.len() <= 4096 && !payload.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_xwininfo_id_geometry_fields() {
        let output =
            "Absolute upper-left X: 12\nAbsolute upper-left Y: 34\nWidth: 640\nHeight: 920\n";
        assert_eq!(
            parse_geometry(output),
            Some(QrLoginWindowGeometry {
                x: 12,
                y: 34,
                width: 640,
                height: 920
            })
        );
    }

    #[test]
    fn parses_tree_geometry_token() {
        let output = "0x123 \"QQ\" (normal) 320x460+8+9";
        assert_eq!(
            parse_window_geometry(output),
            Some((
                0x123,
                QrLoginWindowGeometry {
                    x: 8,
                    y: 9,
                    width: 320,
                    height: 460
                }
            ))
        );
    }
}

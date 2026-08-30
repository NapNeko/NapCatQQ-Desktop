use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::{SnowlumaQrFailureCategory, SnowlumaQrLoginResult, SnowlumaQrLoginSession};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use thiserror::Error;

const BASE_WIDTH: u32 = 320;
const BASE_HEIGHT: u32 = 460;
const MAX_FRAME_BYTES: u64 = 4 * 1024 * 1024;
const CAPTURE_SCRIPT: &str = r#"
import ctypes, os, sys, time
from PIL import ImageGrab

display, xauthority, path = sys.argv[1:4]
x, y, width, height, action, delay_ms = (int(value) for value in sys.argv[4:10])
os.environ['DISPLAY'] = display
os.environ['XAUTHORITY'] = xauthority
x11 = ctypes.CDLL('libX11.so.6')
xtst = ctypes.CDLL('libXtst.so.6')
x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
x11.XCloseDisplay.restype = ctypes.c_int
x11.XFlush.argtypes = [ctypes.c_void_p]
x11.XFlush.restype = ctypes.c_int
xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeMotionEvent.restype = ctypes.c_int
xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeButtonEvent.restype = ctypes.c_int
handle = x11.XOpenDisplay(display.encode())
if not handle: raise RuntimeError('x11 display unavailable')
try:
    points = ((width // 2, height * 280 // 460), (width * 120 // 320, height * 424 // 460), (width * 120 // 320, height * 422 // 460))
    if action >= 0:
        click_x, click_y = points[action]
        if xtst.XTestFakeMotionEvent(handle, 0, x + click_x, y + click_y, 0) == 0: raise RuntimeError('x11 motion failed')
        if xtst.XTestFakeButtonEvent(handle, 1, 1, 0) == 0: raise RuntimeError('x11 press failed')
        if xtst.XTestFakeButtonEvent(handle, 1, 0, 0) == 0: raise RuntimeError('x11 release failed')
        if x11.XFlush(handle) == 0: raise RuntimeError('x11 flush failed')
        time.sleep(delay_ms / 1000.0)
finally:
    x11.XCloseDisplay(handle)
ImageGrab.grab(bbox=(x, y, x + width, y + height)).save(path, 'PNG')
"#;

pub trait QrDecoder: Send + Sync {
    fn decode(&self, frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory>;
}

pub struct UnavailableQrDecoder;

impl QrDecoder for UnavailableQrDecoder {
    fn decode(&self, _frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory> {
        Err(SnowlumaQrFailureCategory::DecoderUnavailable)
    }
}

pub struct QuircsQrDecoder;

impl QrDecoder for QuircsQrDecoder {
    fn decode(&self, frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory> {
        let image = image::load_from_memory(frame)
            .map_err(|_| SnowlumaQrFailureCategory::DecodeFailed)?
            .into_luma8();
        let mut decoder = quircs::Quirc::default();
        for code in decoder.identify(
            image.width() as usize,
            image.height() as usize,
            image.as_raw(),
        ) {
            let decoded = code
                .map_err(|_| SnowlumaQrFailureCategory::DecodeFailed)?
                .decode()
                .map_err(|_| SnowlumaQrFailureCategory::DecodeFailed)?;
            let payload = String::from_utf8(decoded.payload)
                .map_err(|_| SnowlumaQrFailureCategory::DecodeFailed)?;
            if valid_payload(&payload) {
                return Ok(payload);
            }
        }
        Err(SnowlumaQrFailureCategory::DecodeFailed)
    }
}
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

/// Accepts only the X11 display form used by the validated SnowLuma runtime (`:0`,
/// `:1`, ...). Unix socket, host-qualified, and screen-suffixed forms are rejected
/// because the calibrated remote capture contract does not cover them.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowCandidate {
    window_id: u64,
    geometry: QrLoginWindowGeometry,
}

fn parse_window_candidates(output: &str) -> Vec<WindowCandidate> {
    output
        .lines()
        .filter_map(|line| {
            let id_text = line
                .split_whitespace()
                .find(|part| part.starts_with("0x"))?;
            let window_id = u64::from_str_radix(id_text.trim_start_matches("0x"), 16).ok()?;
            Some(WindowCandidate {
                window_id,
                geometry: parse_geometry(line)?,
            })
        })
        .collect()
}

fn parse_xprop_identity(output: &str) -> Option<(u32, String, String)> {
    let pid = output.lines().find_map(|line| {
        line.strip_prefix("_NET_WM_PID(CARDINAL) = ")?
            .trim()
            .parse()
            .ok()
    })?;
    let class = output
        .lines()
        .find_map(|line| line.strip_prefix("WM_CLASS(STRING) = "))?;
    let mut class_parts = class.split(',').map(|part| part.trim().trim_matches('"'));
    let instance = class_parts.next()?;
    let class_name = class_parts.next()?;
    let title = output.lines().find_map(|line| {
        line.strip_prefix("WM_NAME(STRING) = ")?
            .trim()
            .trim_matches('"')
            .to_string()
            .into()
    })?;
    (instance == "qq" && class_name == "QQ" && title == "QQ").then_some((
        pid,
        title,
        class_name.to_string(),
    ))
}

fn window_matches_identity(output: &str, expected_pid: u32) -> bool {
    parse_xprop_identity(output).is_some_and(|(pid, _, _)| pid == expected_pid)
}

async fn resolve_unique_window(host: &dyn Host, expected_pid: u32) -> Option<WindowCandidate> {
    let tree = host
        .run_to_string(
            HostCommand::new("xwininfo")
                .arg("-root")
                .arg("-tree")
                .timeout(Duration::from_secs(3)),
        )
        .await
        .ok()?
        .stdout;
    let mut matches = Vec::new();
    for candidate in parse_window_candidates(&tree) {
        let output = host
            .run_to_string(
                HostCommand::new("xprop")
                    .arg("-id")
                    .arg(format!("0x{:x}", candidate.window_id))
                    .arg("WM_CLASS")
                    .arg("_NET_WM_PID")
                    .arg("WM_NAME")
                    .timeout(Duration::from_secs(2)),
            )
            .await
            .ok()?;
        if window_matches_identity(&output.stdout, expected_pid) {
            matches.push(candidate);
        }
    }
    (matches.len() == 1).then(|| matches[0])
}
fn parse_runtime_env(output: &str) -> Option<(String, String)> {
    let mut values = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let display = values.next()?.to_string();
    let xauthority = values.next()?.to_string();
    (values.next().is_none()).then_some((display, xauthority))
}

fn validate_xauthority(value: &str) -> bool {
    value.starts_with('/') && !value.chars().any(char::is_control) && value.len() <= 4096
}
pub struct SnowlumaQrCaptureRequest {
    pub session: SnowlumaQrLoginSession,
    pub display: String,
    pub xauthority: String,
    pub expected_pid: u32,
}

pub struct SnowlumaQrCaptureService {
    _decoder: Arc<dyn QrDecoder>,
}

impl SnowlumaQrCaptureService {
    pub fn new(decoder: Arc<dyn QrDecoder>) -> Self {
        Self { _decoder: decoder }
    }

    pub async fn capture_current_variant(
        &self,
        host: &dyn Host,
        session: SnowlumaQrLoginSession,
        expected_pid: u32,
    ) -> SnowlumaQrLoginResult {
        let fallback = |reason| SnowlumaQrLoginResult::FallbackNoVnc {
            session: session.clone(),
            reason,
        };
        if expected_pid == 0 || host.os() != Os::Linux || host.locality() != Locality::Remote {
            return fallback(SnowlumaQrFailureCategory::UnsupportedVariant);
        }
        let env = match host
            .run_to_string(
                HostCommand::new("printenv")
                    .arg("DISPLAY")
                    .arg("XAUTHORITY")
                    .timeout(Duration::from_secs(2)),
            )
            .await
        {
            Ok(output) if output.success() => parse_runtime_env(&output.stdout),
            _ => None,
        };
        let Some((display, xauthority)) = env else {
            return fallback(SnowlumaQrFailureCategory::CapabilityUnavailable);
        };
        if let Err(reason) = validate_capture_preflight(
            host.os(),
            host.locality(),
            &display,
            &xauthority,
            expected_pid,
        ) {
            return fallback(reason);
        }
        let xauth_path = HostPath::from_posix(xauthority.clone());
        if !matches!(host.exists(&xauth_path).await, Ok(true)) {
            return fallback(SnowlumaQrFailureCategory::CapabilityUnavailable);
        }
        let Some(_window) = resolve_unique_window(host, expected_pid).await else {
            return fallback(SnowlumaQrFailureCategory::AmbiguousBinding);
        };
        self.capture_and_decode(
            host,
            SnowlumaQrCaptureRequest {
                session,
                display,
                xauthority,
                expected_pid,
            },
        )
        .await
    }
    // Kept private so callers cannot bypass the host, environment, and window identity gates above.
    async fn capture_and_decode(
        &self,
        host: &dyn Host,
        request: SnowlumaQrCaptureRequest,
    ) -> SnowlumaQrLoginResult {
        let fallback = |reason| SnowlumaQrLoginResult::FallbackNoVnc {
            session: request.session.clone(),
            reason,
        };
        let remote_dir = temporary_remote_dir(host.id());
        let remote_png = remote_dir.join("frame.png");
        let local_path = temporary_frame_path();
        if !matches!(host.run_to_string(HostCommand::new("mkdir").arg("-m").arg("700").arg(remote_dir.as_posix())).await, Ok(output) if output.success())
        {
            cleanup_capture(host, &remote_dir, &local_path).await;
            return fallback(SnowlumaQrFailureCategory::CapabilityUnavailable);
        }
        let Some(mut previous) =
            capture_frame(host, &request, &remote_png, &local_path, -1, 0).await
        else {
            cleanup_capture(host, &remote_dir, &local_path).await;
            return fallback(SnowlumaQrFailureCategory::CaptureFailed);
        };
        let baseline = previous.clone();
        for (action, wait_ms) in [(0, 1200), (1, 1000), (2, 1000)] {
            let Some(window) = resolve_unique_window(host, request.expected_pid).await else {
                cleanup_capture(host, &remote_dir, &local_path).await;
                return fallback(SnowlumaQrFailureCategory::AmbiguousBinding);
            };
            if calibrated_click_points(window.geometry).is_err() {
                cleanup_capture(host, &remote_dir, &local_path).await;
                return fallback(SnowlumaQrFailureCategory::UnsupportedVariant);
            }
            let Some(frame) =
                capture_frame(host, &request, &remote_png, &local_path, action, wait_ms).await
            else {
                cleanup_capture(host, &remote_dir, &local_path).await;
                return fallback(SnowlumaQrFailureCategory::CaptureFailed);
            };
            if frame == previous {
                cleanup_capture(host, &remote_dir, &local_path).await;
                return fallback(SnowlumaQrFailureCategory::CaptureFailed);
            }
            previous = frame;
        }
        cleanup_capture(host, &remote_dir, &local_path).await;
        match decode_changed_frame(self._decoder.as_ref(), &baseline, &previous) {
            Ok(payload) => SnowlumaQrLoginResult::Payload {
                session: request.session,
                payload,
            },
            Err(reason) => fallback(reason),
        }
    }
}

async fn capture_frame(
    host: &dyn Host,
    request: &SnowlumaQrCaptureRequest,
    remote: &HostPath,
    local: &std::path::Path,
    action: i32,
    wait_ms: i32,
) -> Option<Vec<u8>> {
    let window = resolve_unique_window(host, request.expected_pid).await?;
    calibrated_click_points(window.geometry).ok()?;
    let _ = tokio::fs::remove_file(local).await;
    let output = host
        .run_to_string(
            HostCommand::new("python3")
                .arg("-c")
                .arg(CAPTURE_SCRIPT)
                .arg(&request.display)
                .arg(&request.xauthority)
                .arg(remote.as_posix())
                .args([
                    window.geometry.x.to_string(),
                    window.geometry.y.to_string(),
                    window.geometry.width.to_string(),
                    window.geometry.height.to_string(),
                    action.to_string(),
                    wait_ms.to_string(),
                ])
                .timeout(Duration::from_secs(12)),
        )
        .await
        .ok()?;
    if !output.success() {
        return None;
    }
    let remote_size = host
        .run_to_string(
            HostCommand::new("stat")
                .arg("-c")
                .arg("%s")
                .arg(remote.as_posix())
                .timeout(Duration::from_secs(2)),
        )
        .await
        .ok()?
        .stdout
        .trim()
        .parse::<u64>()
        .ok()?;
    if remote_size == 0
        || remote_size > MAX_FRAME_BYTES
        || host.download(remote, local).await.is_err()
    {
        return None;
    }
    let metadata = tokio::fs::metadata(local).await.ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_FRAME_BYTES {
        return None;
    }
    tokio::fs::read(local).await.ok()
}

async fn cleanup_capture(host: &dyn Host, remote_dir: &HostPath, local_path: &std::path::Path) {
    let _ = tokio::fs::remove_file(local_path).await;
    let _ = host.remove_dir_all(remote_dir).await;
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

fn validate_capture_preflight(
    os: Os,
    locality: Locality,
    display: &str,
    xauthority: &str,
    expected_pid: u32,
) -> Result<(), SnowlumaQrFailureCategory> {
    if expected_pid == 0 || os != Os::Linux || locality != Locality::Remote {
        return Err(SnowlumaQrFailureCategory::UnsupportedVariant);
    }
    if validate_display(display).is_err() || !validate_xauthority(xauthority) {
        return Err(SnowlumaQrFailureCategory::CapabilityUnavailable);
    }
    Ok(())
}

fn decode_changed_frame(
    decoder: &dyn QrDecoder,
    previous: &[u8],
    current: &[u8],
) -> Result<String, SnowlumaQrFailureCategory> {
    if previous == current {
        return Err(SnowlumaQrFailureCategory::CaptureFailed);
    }
    let payload = decoder.decode(current)?;
    valid_payload(&payload)
        .then_some(payload)
        .ok_or(SnowlumaQrFailureCategory::DecodeFailed)
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
    fn parses_tree_geometry_candidates_without_first_match_assumption() {
        let output = "0x123 \"QQ\" (normal) 320x460+8+9\n0x456 \"QQ\" (normal) 320x460+18+19";
        assert_eq!(parse_window_candidates(output).len(), 2);
        assert_eq!(parse_window_candidates(output)[0].window_id, 0x123);
    }

    #[test]
    fn accepts_only_exact_qq_class_and_title_identity() {
        let output = "WM_CLASS(STRING) = \"qq\", \"QQ\"\n_NET_WM_PID(CARDINAL) = 1234\nWM_NAME(STRING) = \"QQ\"";
        assert_eq!(
            parse_xprop_identity(output),
            Some((1234, "QQ".into(), "QQ".into()))
        );
        assert!(!window_matches_identity(
            &output.replace("1234", "1235"),
            1234
        ));
        assert!(!window_matches_identity(
            &output.replace("WM_NAME(STRING) = \"QQ\"", "WM_NAME(STRING) = \"Other\""),
            1234,
        ));
    }

    #[test]
    fn parses_runtime_display_and_authority_as_two_values() {
        assert_eq!(
            parse_runtime_env(":0\n/root/.Xauthority\n"),
            Some((":0".into(), "/root/.Xauthority".into()))
        );
        assert!(parse_runtime_env(":0\n").is_none());
        assert!(!validate_xauthority("relative/.Xauthority"));
    }
    struct TestDecoder;
    impl QrDecoder for TestDecoder {
        fn decode(&self, frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory> {
            (frame == b"new-frame")
                .then_some("https://example.test/login".to_string())
                .ok_or(SnowlumaQrFailureCategory::DecodeFailed)
        }
    }

    #[test]
    fn changed_frame_with_injected_decoder_returns_payload() {
        let result = decode_changed_frame(&TestDecoder, b"old-frame", b"new-frame");
        assert!(matches!(result, Ok(payload) if payload == "https://example.test/login"));
    }

    #[test]
    fn unchanged_frame_is_terminal_before_decoder() {
        let result = decode_changed_frame(&TestDecoder, b"same", b"same");
        assert_eq!(result, Err(SnowlumaQrFailureCategory::CaptureFailed));
    }

    #[test]
    fn preflight_rejects_wrong_host_and_invalid_x11_facts() {
        assert_eq!(
            validate_capture_preflight(Os::Windows, Locality::Remote, ":0", "/tmp/xauth", 1),
            Err(SnowlumaQrFailureCategory::UnsupportedVariant)
        );
        assert_eq!(
            validate_capture_preflight(Os::Linux, Locality::Local, ":0", "/tmp/xauth", 1),
            Err(SnowlumaQrFailureCategory::UnsupportedVariant)
        );
        assert_eq!(
            validate_capture_preflight(Os::Linux, Locality::Remote, "bad", "/tmp/xauth", 1),
            Err(SnowlumaQrFailureCategory::CapabilityUnavailable)
        );
        assert_eq!(
            validate_capture_preflight(Os::Linux, Locality::Remote, ":0", "relative", 1),
            Err(SnowlumaQrFailureCategory::CapabilityUnavailable)
        );
    }
}

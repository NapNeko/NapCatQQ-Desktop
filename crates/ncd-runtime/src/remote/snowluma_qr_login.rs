use std::sync::Arc;
use std::time::Duration;

use ncd_domain::{SnowlumaQrFailureCategory, SnowlumaQrLoginResult, SnowlumaQrLoginSession};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use thiserror::Error;

const BASE_WIDTH: u32 = 320;
const BASE_HEIGHT: u32 = 460;

pub trait QrDecoder: Send + Sync {
    fn decode(&self, frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory>;
}

pub struct UnavailableQrDecoder;

impl QrDecoder for UnavailableQrDecoder {
    fn decode(&self, _frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory> {
        Err(SnowlumaQrFailureCategory::DecoderUnavailable)
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
        if let Some((pid, title, _)) = parse_xprop_identity(&output.stdout) {
            if pid == expected_pid && title == "QQ" {
                matches.push(candidate);
            }
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
        if validate_display(&display).is_err() || !validate_xauthority(&xauthority) {
            return fallback(SnowlumaQrFailureCategory::CapabilityUnavailable);
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
            },
        )
        .await
    }
    pub async fn capture_and_decode(
        &self,
        _host: &dyn Host,
        request: SnowlumaQrCaptureRequest,
    ) -> SnowlumaQrLoginResult {
        // OCR/accessibility anchors for the Chinese/light QQ build are not exposed by Host.
        // Refuse to send any input until an anchor-aware implementation is available.
        SnowlumaQrLoginResult::FallbackNoVnc {
            session: request.session,
            reason: SnowlumaQrFailureCategory::CapabilityUnavailable,
        }
    }
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
        assert!(parse_xprop_identity(&output.replace("1234", "1235")).is_some());
        assert!(
            parse_xprop_identity(
                &output.replace("WM_NAME(STRING) = \"QQ\"", "WM_NAME(STRING) = \"Other\"")
            )
            .is_none()
        );
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
}

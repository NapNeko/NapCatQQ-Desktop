use async_trait::async_trait;
use bytes::Bytes;
use ncd_domain::{SnowlumaQrFailureCategory, SnowlumaQrLoginResult, SnowlumaQrLoginSession};
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath, Locality,
    Os,
};
use ncd_runtime::remote::snowluma_qr_login::{
    QrDecoder, QrLoginPoint, QrLoginWindowGeometry, QuircsQrDecoder, SnowlumaQrCaptureService,
    UnavailableQrDecoder, calibrated_click_points, validate_display,
};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn current_variant_recalculates_clicks_from_window_geometry() {
    let geometry = QrLoginWindowGeometry {
        x: 480,
        y: 130,
        width: 320,
        height: 460,
    };
    assert_eq!(
        calibrated_click_points(geometry).unwrap(),
        vec![
            QrLoginPoint { x: 640, y: 410 },
            QrLoginPoint { x: 600, y: 554 },
            QrLoginPoint { x: 600, y: 552 },
        ]
    );
}

#[test]
fn display_validation_accepts_only_numeric_x11_display() {
    assert!(validate_display(":0").is_ok());
    assert!(validate_display(":12").is_ok());
    assert!(validate_display(":0.0").is_err());
    assert!(validate_display("unix/:0").is_err());
    assert!(validate_display(":0; touch /tmp/pwned").is_err());
}

#[test]
fn missing_decoder_is_an_explicit_novnc_fallback() {
    let error = UnavailableQrDecoder.decode(&[]).unwrap_err();
    assert_eq!(error, SnowlumaQrFailureCategory::DecoderUnavailable);
}

#[test]
fn serialized_fallback_keeps_the_reason_visible() {
    let session = ncd_domain::SnowlumaQrLoginSession {
        server_id: "srv".into(),
        bot_id: "10001".into(),
        session_id: ncd_domain::QrLoginSessionId::new("session").unwrap(),
        capture_generation: 0,
    };
    let result = ncd_domain::SnowlumaQrLoginResult::FallbackNoVnc {
        session,
        reason: SnowlumaQrFailureCategory::DecoderUnavailable,
    };
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["status"], "fallback_no_vnc");
    assert_eq!(json["reason"], "decoder_unavailable");
}

#[test]
fn quircs_decoder_decodes_synthetic_png_in_memory() {
    let frame = include_bytes!("fixtures/synthetic-qr.png");
    assert_eq!(
        QuircsQrDecoder.decode(frame).unwrap(),
        "https://example.test/qr-fixture"
    );
}

#[tokio::test]
async fn service_runs_bound_capture_and_cleans_up_on_payload() {
    let host = ScriptedHost::success(7_001);
    let decoder = Arc::new(TestDecoder::new(
        b"final-frame",
        "https://example.test/login",
    ));
    let service = SnowlumaQrCaptureService::new(decoder.clone());
    let result = service
        .capture_current_variant(&host, test_session(), 7_001)
        .await;

    assert!(matches!(
        result,
        SnowlumaQrLoginResult::Payload { payload, .. }
            if payload == "https://example.test/login"
    ));
    assert_eq!(decoder.calls(), vec![b"final-frame".to_vec()]);
    host.assert_success_order();
}

#[tokio::test]
async fn service_falls_back_and_cleans_up_when_frame_download_fails() {
    let host = ScriptedHost::download_failure(7_001);
    let decoder = Arc::new(TestDecoder::new(b"never", "https://example.test/never"));
    let service = SnowlumaQrCaptureService::new(decoder.clone());
    let result = service
        .capture_current_variant(&host, test_session(), 7_001)
        .await;

    assert!(matches!(
        result,
        SnowlumaQrLoginResult::FallbackNoVnc {
            reason: SnowlumaQrFailureCategory::CaptureFailed,
            ..
        }
    ));
    assert!(decoder.calls().is_empty());
    host.assert_failure_cleanup();
}

#[tokio::test]
async fn service_rejects_local_host_before_remote_preflight() {
    let host = ScriptedHost::new(7_001, Os::Linux, Locality::Local, false);
    let decoder = Arc::new(TestDecoder::new(b"never", "https://example.test/never"));
    let service = SnowlumaQrCaptureService::new(decoder.clone());

    let result = service
        .capture_current_variant(&host, test_session(), 7_001)
        .await;

    assert!(matches!(
        result,
        SnowlumaQrLoginResult::FallbackNoVnc {
            reason: SnowlumaQrFailureCategory::UnsupportedVariant,
            ..
        }
    ));
    assert!(host.events().is_empty());
    assert!(decoder.calls().is_empty());
}

fn test_session() -> SnowlumaQrLoginSession {
    SnowlumaQrLoginSession {
        server_id: "srv".into(),
        bot_id: "10001".into(),
        session_id: ncd_domain::QrLoginSessionId::new("service-test").unwrap(),
        capture_generation: 1,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Command { program: String, args: Vec<String> },
    Exists(String),
    Download { remote: String, local: PathBuf },
    RemoveDir(String),
}

struct ScriptState {
    events: Vec<Event>,
    capture_index: usize,
    current_frame: Vec<u8>,
    download_failure: bool,
}

struct ScriptedHost {
    state: Mutex<ScriptState>,
    shell: BashShell,
    os: Os,
    locality: Locality,
    pid: u32,
}

impl ScriptedHost {
    fn success(pid: u32) -> Self {
        Self::new(pid, Os::Linux, Locality::Remote, false)
    }

    fn download_failure(pid: u32) -> Self {
        Self::new(pid, Os::Linux, Locality::Remote, true)
    }

    fn new(pid: u32, os: Os, locality: Locality, download_failure: bool) -> Self {
        Self {
            state: Mutex::new(ScriptState {
                events: Vec::new(),
                capture_index: 0,
                current_frame: Vec::new(),
                download_failure,
            }),
            shell: BashShell,
            os,
            locality,
            pid,
        }
    }

    fn events(&self) -> Vec<Event> {
        self.state.lock().events.clone()
    }

    fn assert_success_order(&self) {
        let events = self.events();
        let commands: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Command { program, args } => Some((program.as_str(), args)),
                _ => None,
            })
            .collect();
        let env_index = events
            .iter()
            .position(
                |event| matches!(event, Event::Command { program, .. } if program == "printenv"),
            )
            .unwrap();
        let exists_index = events
            .iter()
            .position(|event| matches!(event, Event::Exists(path) if path == "/tmp/xauth"))
            .unwrap();
        assert!(env_index < exists_index);

        let env_command = commands
            .iter()
            .find(|(program, _)| *program == "printenv")
            .unwrap();
        assert_eq!(env_command.1, &["DISPLAY", "XAUTHORITY"]);
        let capture_args: Vec<_> = commands
            .iter()
            .filter(|(program, _)| *program == "python3")
            .map(|(_, args)| args)
            .collect();
        assert!(
            capture_args
                .iter()
                .all(|args| args[2] == ":0" && args[3] == "/tmp/xauth")
        );
        let xwininfo_count = commands
            .iter()
            .filter(|(program, _)| *program == "xwininfo")
            .count();
        let xprop_args: Vec<_> = commands
            .iter()
            .filter(|(program, _)| *program == "xprop")
            .map(|(_, args)| args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect();
        assert_eq!(xwininfo_count, 8);
        assert_eq!(xprop_args.len(), 8);
        assert!(
            xprop_args
                .iter()
                .all(|args| { args == &["-id", "0xc00003", "WM_CLASS", "_NET_WM_PID", "WM_NAME"] })
        );

        let actions: Vec<_> = commands
            .iter()
            .filter(|(program, _)| *program == "python3")
            .map(|(_, args)| args[9].parse::<i32>().unwrap())
            .collect();
        assert_eq!(actions, vec![-1, 0, 1, 2]);
        let stat_indices: Vec<_> = events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                matches!(event, Event::Command { program, args } if program == "stat" && args[0] == "-c")
                    .then_some(index)
            })
            .collect();
        let download_indices: Vec<_> = events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| matches!(event, Event::Download { .. }).then_some(index))
            .collect();
        assert_eq!(stat_indices.len(), 4);
        assert_eq!(download_indices.len(), 4);
        assert!(
            stat_indices
                .iter()
                .zip(download_indices)
                .all(|(stat, download)| stat < &download)
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::RemoveDir(_)))
                .count(),
            1
        );
        for event in events.iter().filter_map(|event| match event {
            Event::Download { local, .. } => Some(local),
            _ => None,
        }) {
            assert!(!event.exists(), "temporary frame should be removed");
        }
    }

    fn assert_failure_cleanup(&self) {
        let events = self.events();
        let actions: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Command { program, args } if program == "python3" => {
                    Some(args[9].parse::<i32>().unwrap())
                }
                _ => None,
            })
            .collect();
        assert_eq!(actions, vec![-1]);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::RemoveDir(_)))
                .count(),
            1
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::Download { .. }))
        );
        for event in events.iter().filter_map(|event| match event {
            Event::Download { local, .. } => Some(local),
            _ => None,
        }) {
            assert!(!event.exists(), "failed download must leave no stale frame");
        }
    }
}

#[derive(Default)]
struct TestDecoder {
    expected: Vec<u8>,
    payload: String,
    calls: Mutex<Vec<Vec<u8>>>,
}

impl TestDecoder {
    fn new(expected: &[u8], payload: &str) -> Self {
        Self {
            expected: expected.to_vec(),
            payload: payload.to_string(),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<Vec<u8>> {
        self.calls.lock().clone()
    }
}

impl QrDecoder for TestDecoder {
    fn decode(&self, frame: &[u8]) -> Result<String, SnowlumaQrFailureCategory> {
        self.calls.lock().push(frame.to_vec());
        if frame == self.expected {
            Ok(self.payload.clone())
        } else {
            Err(SnowlumaQrFailureCategory::DecodeFailed)
        }
    }
}

#[async_trait]
impl Host for ScriptedHost {
    fn os(&self) -> Os {
        self.os
    }

    fn arch(&self) -> Arch {
        Arch::X86_64
    }

    fn locality(&self) -> Locality {
        self.locality
    }

    fn id(&self) -> &str {
        "test-host"
    }

    fn shell(&self) -> &dyn ncd_host::HostShell {
        &self.shell
    }

    fn pkg_manager(&self) -> Option<&dyn ncd_host::PackageManager> {
        None
    }

    async fn read_file(&self, _path: &HostPath) -> Result<Bytes, HostError> {
        Err(HostError::Unsupported {
            operation: "read_file",
        })
    }

    async fn write_file(&self, _path: &HostPath, _bytes: &[u8]) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "write_file",
        })
    }

    async fn list_dir(&self, _path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Err(HostError::Unsupported {
            operation: "list_dir",
        })
    }

    async fn create_dir_all(&self, _path: &HostPath) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "create_dir_all",
        })
    }

    async fn remove_file(&self, _path: &HostPath) -> Result<(), HostError> {
        Ok(())
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        self.state
            .lock()
            .events
            .push(Event::RemoveDir(path.as_posix().to_string()));
        Ok(())
    }

    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        self.state
            .lock()
            .events
            .push(Event::Exists(path.as_posix().to_string()));
        Ok(path.as_posix() == "/tmp/xauth")
    }

    async fn upload(&self, _local: &Path, _remote: &HostPath) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "upload",
        })
    }

    async fn download(&self, remote: &HostPath, local: &Path) -> Result<(), HostError> {
        let (failure, frame) = {
            let mut state = self.state.lock();
            state.events.push(Event::Download {
                remote: remote.as_posix().to_string(),
                local: local.to_path_buf(),
            });
            (state.download_failure, state.current_frame.clone())
        };
        if failure {
            return Err(HostError::command_failed(
                "download",
                Some(1),
                "scripted failure",
            ));
        }
        tokio::fs::write(local, frame)
            .await
            .map_err(HostError::from)
    }

    async fn extract_archive(
        &self,
        _archive: &HostPath,
        _dest: &HostPath,
        _kind: ArchiveKind,
    ) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "extract_archive",
        })
    }

    async fn spawn(&self, _cmd: HostCommand) -> Result<Box<dyn ncd_host::HostProcess>, HostError> {
        Err(HostError::Unsupported { operation: "spawn" })
    }

    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        let program = cmd.program.clone();
        let args = cmd.args.clone();
        let mut state = self.state.lock();
        state.events.push(Event::Command {
            program: program.clone(),
            args: args.clone(),
        });
        let stdout = match program.as_str() {
            "printenv" => ":0\n/tmp/xauth\n".to_string(),
            "xwininfo" => "0xc00003 320x460+480+130 QQ\n".to_string(),
            "xprop" => format!(
                "_NET_WM_PID(CARDINAL) = {}\nWM_CLASS(STRING) = \"qq\", \"QQ\"\nWM_NAME(STRING) = \"QQ\"\n",
                self.pid
            ),
            "mkdir" => String::new(),
            "python3" => {
                let frames = [
                    b"baseline".as_slice(),
                    b"fresh-1",
                    b"fresh-2",
                    b"final-frame",
                ];
                state.current_frame = frames[state.capture_index].to_vec();
                state.capture_index += 1;
                String::new()
            }
            "stat" => state.current_frame.len().to_string(),
            _ => {
                return Err(HostError::Unsupported {
                    operation: "scripted command",
                });
            }
        };
        Ok(CommandOutput {
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
        })
    }
}

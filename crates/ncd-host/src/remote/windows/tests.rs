use super::files::{protected_path, remove_directory_script, rename_script, target_path};
use super::*;

fn decode_script(line: &str) -> String {
    let bytes = STANDARD
        .decode(line.split_whitespace().last().unwrap())
        .unwrap();
    let units = bytes
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).unwrap()
}

#[test]
fn encoded_entry_does_not_expose_script_to_login_shell() {
    let script = "Write-Output '中文; $(whoami) `x` & >'; exit 7";
    let line = powershell_line(script);
    assert!(!line.contains("Write-Output"));
    assert_eq!(decode_script(&line), script);
}

#[test]
fn noninteractive_command_without_input_still_sends_ssh_eof() {
    let command = HostCommand::new("whoami.exe");
    let (_, prepared) = prepare_exec(command_line(&command, false).unwrap(), &command);
    assert_eq!(prepared.stdin, Some(Vec::new()));
}

#[test]
fn long_exec_uses_short_bootstrap_and_preserves_binary_stdin_after_script_frame() {
    let command = HostCommand::new("powershell.exe")
        .args(["-Command", &format!("Write-Output '{}'", "字".repeat(1500))])
        .stdin(vec![0, 255, 10, 13]);
    let original = command_line(&command, false).unwrap();
    assert!(original.len() > 8191);
    let (line, prepared) = prepare_exec(original.clone(), &command);
    assert!(line.len() < 7_000);
    assert!(decode_script(&line).contains("[BitConverter]::ToUInt32"));
    let stdin = prepared.stdin.unwrap();
    let length = u32::from_le_bytes(stdin[..4].try_into().unwrap()) as usize;
    assert_eq!(
        &stdin[4..4 + length],
        original.rsplit(' ').next().unwrap().as_bytes()
    );
    assert_eq!(&stdin[4 + length..], &[0, 255, 10, 13]);
}

#[cfg(windows)]
#[test]
fn powershell_long_exec_round_trips_binary_stdin_through_hidden_cmd() {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut payload = "中文路径🧪\r\nsecond line\n".as_bytes().to_vec();
    payload.extend_from_slice(&[0, 255, 0, 254, 13, 10]);
    let command = HostCommand::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OpenStandardInput().CopyTo([Console]::OpenStandardOutput());exit 0",
        ])
        .env("NCD_ISOLATED_TEST_PADDING", "x".repeat(2500))
        .stdin(payload.clone());
    let original = command_line(&command, false).unwrap();
    assert!(
        original.len() > 8191,
        "the test must exceed cmd.exe's command line limit"
    );
    let (line, prepared) = prepare_exec(original, &command);
    assert!(line.len() < 7000);
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/s", "/c", &line])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start the isolated Windows PowerShell command");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(prepared.stdin.as_deref().unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "PowerShell failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout, payload,
        "PowerShell must preserve every stdin byte"
    );
}

#[test]
fn native_command_uses_byte_streams_and_literal_environment_keys() {
    let command = HostCommand::new("/c/Program Files/Test/a.exe")
        .args(["", "a\"b", "x & y"])
        .working_dir(HostPath::from_windows(r"D:\测试\it's"))
        .env("X'];whoami;#", "v';boom")
        .stdin(b"hello\n".to_vec());
    let script = decode_script(&command_line(&command, false).unwrap());
    assert!(script.contains("$s.FileName='C:\\Program Files\\Test\\a.exe'"));
    assert!(script.contains("$s.WorkingDirectory='D:\\测试\\it''s'"));
    assert!(script.contains("$s.EnvironmentVariables['X''];whoami;#']='v'';boom'"));
    assert!(script.contains("$p.StandardOutput.BaseStream.CopyToAsync"));
    assert!(script.contains("$ncdRemaining=6;"));
    assert!(script.contains("Unexpected end of command input"));
    assert!(!script.contains("[Console]::OpenStandardInput().CopyTo"));
}

#[cfg(windows)]
#[tokio::test]
async fn framed_stdin_completes_while_the_ssh_input_pipe_stays_open() {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;

    let payload = vec![0, 255, 13, 10, 254, 0];
    let command = HostCommand::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OpenStandardInput().CopyTo([Console]::OpenStandardOutput())",
        ])
        .env("NCD_STDIN_FRAME_PADDING", "x".repeat(2500))
        .stdin(payload.clone());
    let (line, prepared) = prepare_exec(command_line(&command, false).unwrap(), &command);
    let encoded = line.rsplit(' ').next().unwrap();
    let mut child = tokio::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            encoded,
        ])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input
        .write_all(prepared.stdin.as_deref().unwrap())
        .await
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .expect("a complete input frame must not wait for SSH EOF")
        .unwrap();
    drop(input);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, payload);
    assert!(output.stderr.is_empty());
}

#[test]
fn native_argument_escaping_preserves_empty_quotes_and_terminal_backslashes() {
    assert_eq!(windows_argument(""), "\"\"");
    assert_eq!(windows_argument("a\"b"), "\"a\\\"b\"");
    assert_eq!(windows_argument("C:\\space dir\\"), "\"C:\\space dir\\\\\"");
}

#[test]
fn child_powershell_json_output_explicitly_uses_utf8() {
    let arguments = powershell_arguments(
        "powershell.exe",
        &[
            "-NoProfile".into(),
            "-Command".into(),
            "Write-Output '中文路径'".into(),
        ],
    )
    .unwrap();
    assert!(arguments[2].starts_with("$ProgressPreference='SilentlyContinue';"));
    assert!(arguments[2].contains("[Console]::OutputEncoding="));
    assert!(arguments[2].contains("Write-Output '中文路径'"));
    let arguments = powershell_arguments(
        "pwsh.exe",
        &[
            "-EncodedCommand".into(),
            STANDARD.encode(
                "Write-Output '中文'"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
        ],
    )
    .unwrap();
    let script = decode_script(&format!("-EncodedCommand {}", arguments[1]));
    assert!(script.contains("$OutputEncoding=[Console]::OutputEncoding"));
    assert!(script.contains("Write-Output '中文'"));
}

#[test]
fn target_paths_use_remote_home_and_normalize_drive_and_parent_segments() {
    assert_eq!(
        target_path(&HostPath::from_posix("cache/x"), r"E:\Users\远端").unwrap(),
        r"E:\Users\远端\cache\x"
    );
    assert_eq!(
        target_path(&HostPath::from_posix("/d/a/../b"), r"C:\Users\u").unwrap(),
        r"D:\b"
    );
    assert_eq!(
        target_path(&HostPath::from_windows(r"\\server\share\a"), r"C:\Users\u").unwrap(),
        r"\\server\share\a"
    );
    assert!(target_path(&HostPath::from_posix("/opt/foo"), r"C:\Users\u").is_err());
    assert!(target_path(&HostPath::from_windows(r"\\?\C:\Windows"), r"C:\Users\u").is_err());
    for device in [
        r"\\.\C:\Windows",
        r"\\?\UNC\server\share\x",
        r"\??\C:\Windows",
    ] {
        assert!(
            target_path(&HostPath::from_windows(device), r"C:\Users\u").is_err(),
            "accepted {device}"
        );
    }
    for device in ["///?/c/Windows", "///./c/Windows", "///??/c/Windows"] {
        assert!(
            target_path(&HostPath::from_posix(device), r"C:\Users\u").is_err(),
            "accepted {device}"
        );
    }
    assert!(target_path(&HostPath::from_posix("C:relative"), r"C:\Users\u").is_err());
}

#[test]
fn recursive_removal_rejects_drive_unc_and_user_roots() {
    let home = String::from(r"D:\Users\u");
    assert!(protected_path(r"C:\", &[&home]));
    assert!(protected_path(r"\\server\share\", &[&home]));
    assert!(protected_path(r"D:\Users\U", &[&home]));
    assert!(protected_path(r"C:\Windows", &[&home]));
    assert!(!protected_path(r"D:\Users\u\cache\job-1", &[&home]));
}

#[test]
fn file_rename_replaces_existing_pointer_without_truncation_or_directory_overwrite() {
    let script = rename_script(r"D:\QQ\current-next.txt", r"D:\QQ\current.txt");
    assert!(script.contains("[IO.File]::Replace($from,$to,$null)"));
    assert!(script.contains("[IO.File]::Move($from,$to)"));
    assert!(script.contains("throw 'Cannot overwrite a directory'"));
    assert!(!script.contains("WriteAllText"));
    assert!(!script.contains("Remove-Item"));
}

#[test]
fn directory_removal_handles_links_before_enumerating_children() {
    let script = remove_directory_script(r"D:\Apps\old");
    let link_check = script
        .find("if($attributes -band [IO.FileAttributes]::ReparsePoint)")
        .unwrap();
    let enumerate = script
        .find("[IO.Directory]::EnumerateFileSystemEntries($item)")
        .unwrap();
    assert!(link_check < enumerate);
    assert!(script.contains("Refusing to delete through a reparse ancestor"));
    assert!(!script.contains("-Recurse"));
    assert!(!script.contains("::Delete($item,$true)"));
}

#[test]
fn windows_probe_requires_identity_and_supported_protocol() {
    let valid = r#"{"v":1,"osVersion":"Windows","architecture":"AMD64","userSid":"S-1-5-21-1","username":"u","sessionId":0,"elevated":false,"homeDir":"D:\\u","programData":"D:\\pd","tempDir":"D:\\tmp","managedRoot":"D:\\pd\\ncd","userRoot":"D:\\pd\\ncd\\users\\abc","agent":null}"#;
    assert!(parse_windows_info(valid).is_ok());
    assert!(parse_windows_info(&valid.replace("\"v\":1", "\"v\":2")).is_err());
}

#[test]
fn windows_terminal_has_no_linux_shell_prefix() {
    let mut request =
        crate::PtyRequest::new(crate::PtyProgram::LoginShell, crate::PtySize::default());
    request.cwd = Some(HostPath::from_windows(r"D:\bot"));
    let line = pty_line(&request).unwrap();
    assert!(!line.contains("-NonInteractive"));
    let script = decode_script(&line);
    assert!(script.contains("Set-Location -LiteralPath 'D:\\bot'"));
    assert!(script.contains("'-NoExit'"));
    assert!(!script.contains("sh -c"));
}

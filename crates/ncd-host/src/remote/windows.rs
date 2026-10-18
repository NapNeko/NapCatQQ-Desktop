//! Windows SSH 主机；只复用 SSH 通道，不复用 Linux shell 语义。
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use ncd_domain::windows_remote::{
    RemoteWindowsInfo, WINDOWS_AGENT_MAX_FRAME, WINDOWS_AGENT_PROTOCOL, WindowsAgentAction,
    WindowsAgentRequest, WindowsAgentResponse, WindowsAgentResult,
};

use super::raw_exec::PROBE_TIMEOUT;
use super::{ConnectionConfig, RemoteLinuxHost, TunnelHandle, TunnelSpec};
use crate::shell::{HostShell, PowerShellShell};
use crate::{
    Arch, CommandOutput, Host, HostCommand, HostError, HostPath, HostProcess, Locality, Os,
    SshDialTarget, StreamSource,
};

mod files;
mod keys;
#[cfg(test)]
mod tests;

const MAX_COMMAND_OUTPUT: usize = 64 * 1024 * 1024;

// ConsoleStream 在 Windows SSH 管道下可能阻塞；SafeFileHandle 不拥有进程标准句柄。
const RAW_INPUT_SCRIPT: &str = r#"if($null -eq ('NcdRemoteStdin' -as [type])){Add-Type -TypeDefinition 'using System;using System.IO;using System.Runtime.InteropServices;using Microsoft.Win32.SafeHandles;public static class NcdRemoteStdin{[DllImport("kernel32.dll",SetLastError=true)]static extern IntPtr GetStdHandle(int id);public static Stream Open(){IntPtr value=GetStdHandle(-10);if(value==IntPtr.Zero||value==new IntPtr(-1))throw new System.ComponentModel.Win32Exception();return new FileStream(new SafeFileHandle(value,false),FileAccess.Read,4096,false);}}'};if($null -eq $ncdIn){$ncdIn=[NcdRemoteStdin]::Open()};"#;

pub struct RemoteWindowsHost {
    transport: RemoteLinuxHost,
    info: RemoteWindowsInfo,
    architecture: Arch,
    shell: PowerShellShell,
}

/// 先识别操作系统，未识别的远端不得落为 Linux Host。
pub async fn connect_remote_host(
    id: impl Into<String>,
    config: ConnectionConfig,
) -> Result<Arc<dyn Host>, HostError> {
    connect_remote_host_with_platform(id, config, None).await
}

pub async fn connect_remote_host_with_platform(
    id: impl Into<String>,
    config: ConnectionConfig,
    expected: Option<&str>,
) -> Result<Arc<dyn Host>, HostError> {
    if !matches!(expected, None | Some("auto" | "windows" | "linux")) {
        return Err(HostError::InvalidArgument {
            reason: "远端平台必须为auto/windows/linux".into(),
        });
    }
    let transport = RemoteLinuxHost::connect(id, config).await?;
    let probe = HostCommand::new("platform_probe").timeout(PROBE_TIMEOUT);
    let windows = transport
        .run_raw(
            &powershell_line(WINDOWS_INFO_SCRIPT),
            &probe,
            None,
            WINDOWS_AGENT_MAX_FRAME,
        )
        .await?;
    if windows.success() {
        if expected == Some("linux") {
            return Err(HostError::InvalidArgument {
                reason: "目标是Windows，与档案指定的Linux不符".into(),
            });
        }
        let info = parse_windows_info(&windows.stdout)?;
        return Ok(Arc::new(RemoteWindowsHost::from_transport(
            transport, info,
        )?));
    }
    let unix = transport.run_raw("uname -s", &probe, None, 4096).await?;
    if unix.success() && unix.stdout.trim() == "Linux" {
        if expected == Some("windows") {
            return Err(HostError::InvalidArgument {
                reason: "目标是Linux，与档案指定的Windows不符".into(),
            });
        }
        return Ok(Arc::new(transport));
    }
    Err(HostError::Unsupported {
        operation: "remote_platform_not_windows_or_linux",
    })
}

impl RemoteWindowsHost {
    pub async fn connect(
        id: impl Into<String>,
        config: ConnectionConfig,
    ) -> Result<Self, HostError> {
        let transport = RemoteLinuxHost::connect(id, config).await?;
        let cmd = HostCommand::new("windows_probe").timeout(PROBE_TIMEOUT);
        let output = transport
            .run_raw(
                &powershell_line(WINDOWS_INFO_SCRIPT),
                &cmd,
                None,
                WINDOWS_AGENT_MAX_FRAME,
            )
            .await?;
        ensure_success(output.clone(), "windows_probe")?;
        Self::from_transport(transport, parse_windows_info(&output.stdout)?)
    }

    fn from_transport(
        transport: RemoteLinuxHost,
        info: RemoteWindowsInfo,
    ) -> Result<Self, HostError> {
        let architecture = match info.architecture.to_ascii_lowercase().as_str() {
            "amd64" | "x64" | "x86_64" => Arch::X86_64,
            "arm64" | "aarch64" => Arch::Aarch64,
            "x86" | "i386" | "i686" => Arch::X86,
            _ => {
                return Err(HostError::Unsupported {
                    operation: "windows_architecture",
                });
            }
        };
        Ok(Self {
            transport,
            info,
            architecture,
            shell: PowerShellShell,
        })
    }

    async fn script(&self, script: &str) -> Result<CommandOutput, HostError> {
        let cmd = HostCommand::new("powershell.exe");
        let script = format!(
            "$ProgressPreference='SilentlyContinue';[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false);{script}"
        );
        let (line, cmd) = prepare_exec(powershell_line(&script), &cmd);
        self.track(
            self.transport
                .run_raw(&line, &cmd, None, MAX_COMMAND_OUTPUT)
                .await,
        )
        .await
    }

    fn checked_command(&self, cmd: &HostCommand) -> Result<String, HostError> {
        if cmd.elevated && !self.info.elevated {
            return Err(HostError::ElevationFailed {
                locality: "remote",
                reason: "ElevationRequired：请在目标 Windows 上以管理员身份安装或授权 Agent".into(),
            });
        }
        command_line(cmd, false)
    }

    async fn track<T>(&self, result: Result<T, HostError>) -> Result<T, HostError> {
        let broken = result.as_ref().err().is_some_and(|error| error.is_disconnect()
            || matches!(error, HostError::Io(io) if matches!(io.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::NotConnected | std::io::ErrorKind::UnexpectedEof)));
        if broken {
            self.transport.invalidate_connection().await;
        }
        result
    }
}

#[async_trait]
impl Host for RemoteWindowsHost {
    fn os(&self) -> Os {
        Os::Windows
    }
    fn arch(&self) -> Arch {
        self.architecture
    }
    fn locality(&self) -> Locality {
        Locality::Remote
    }
    fn id(&self) -> &str {
        self.transport.id()
    }
    fn shell(&self) -> &dyn HostShell {
        &self.shell
    }
    fn ssh_dial_target(&self) -> Option<SshDialTarget> {
        self.transport.ssh_dial_target()
    }
    fn trusted_ssh_host_key(&self) -> Option<String> {
        self.transport.trusted_ssh_host_key()
    }
    fn target_home(&self) -> Option<HostPath> {
        Some(HostPath::from_windows(&self.info.home_dir))
    }
    fn target_managed_root(&self) -> Option<HostPath> {
        Some(HostPath::from_windows(&self.info.user_root))
    }
    async fn invalidate_connection(&self) {
        self.transport.invalidate_connection().await;
    }
    async fn is_healthy(&self) -> bool {
        self.transport.is_healthy().await
    }
    async fn install_authorized_key(&self, line: &str, marker: &str) -> Result<(), HostError> {
        self.edit_key(Some(line), marker).await
    }
    async fn remove_authorized_key(&self, marker: &str) -> Result<(), HostError> {
        self.edit_key(None, marker).await
    }
    async fn secure_private_key(&self, path: &HostPath) -> Result<(), HostError> {
        self.protect_key(path).await
    }

    async fn remote_windows_info(&self) -> Result<RemoteWindowsInfo, HostError> {
        let mut info = self.info.clone();
        let binary = HostPath::from_windows(&info.managed_root).join("agent/current/ncd-agent.exe");
        if matches!(self.exists(&binary).await, Ok(true)) {
            let request = WindowsAgentRequest {
                v: WINDOWS_AGENT_PROTOCOL,
                request_id: "host-info".into(),
                action: WindowsAgentAction::Hello,
            };
            if let Ok(response) = self.windows_agent_request(request).await {
                if let WindowsAgentResult::Hello(status) = response.result {
                    info.agent = Some(status);
                }
            }
        }
        Ok(info)
    }

    async fn windows_agent_request(
        &self,
        request: WindowsAgentRequest,
    ) -> Result<WindowsAgentResponse, HostError> {
        let mut bytes = serde_json::to_vec(&request).map_err(invalid_json)?;
        if bytes.len() >= WINDOWS_AGENT_MAX_FRAME {
            return Err(HostError::InvalidArgument {
                reason: "Agent请求超过帧上限".into(),
            });
        }
        bytes.push(b'\n');
        let binary =
            HostPath::from_windows(&self.info.managed_root).join("agent/current/ncd-agent.exe");
        let timeout_seconds = match &request.action {
            WindowsAgentAction::BotStart(_) | WindowsAgentAction::BotConfigure(_) => 300,
            WindowsAgentAction::Launch(_) => 60,
            WindowsAgentAction::BotQr { .. } => 90,
            _ => 30,
        };
        let cmd = HostCommand::new(binary.as_posix())
            .args(["bridge", "--root", &self.info.managed_root])
            .stdin(bytes)
            .timeout(Duration::from_secs(timeout_seconds));
        let (line, cmd) = prepare_exec(self.checked_command(&cmd)?, &cmd);
        let output = self
            .track(
                self.transport
                    .run_raw(&line, &cmd, None, WINDOWS_AGENT_MAX_FRAME)
                    .await,
            )
            .await?;
        ensure_success(output.clone(), "windows_agent_bridge")?;
        let response: WindowsAgentResponse =
            serde_json::from_str(output.stdout.trim().trim_start_matches('\u{feff}'))
                .map_err(invalid_json)?;
        if response.v != WINDOWS_AGENT_PROTOCOL || response.request_id != request.request_id {
            return Err(HostError::InvalidArgument {
                reason: "Agent协议版本或请求标识不匹配".into(),
            });
        }
        Ok(response)
    }

    async fn spawn(&self, cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        let (line, cmd) = prepare_exec(self.checked_command(&cmd)?, &cmd);
        self.track(self.transport.spawn_raw(&line, &cmd).await)
            .await
    }
    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        let (line, cmd) = prepare_exec(self.checked_command(&cmd)?, &cmd);
        self.track(
            self.transport
                .run_raw(&line, &cmd, None, MAX_COMMAND_OUTPUT)
                .await,
        )
        .await
    }
    async fn run_streaming(
        &self,
        cmd: HostCommand,
        on_line: Box<dyn FnMut(StreamSource, String) + Send>,
    ) -> Result<CommandOutput, HostError> {
        let (line, cmd) = prepare_exec(self.checked_command(&cmd)?, &cmd);
        self.track(
            self.transport
                .run_raw(&line, &cmd, Some(on_line), MAX_COMMAND_OUTPUT)
                .await,
        )
        .await
    }
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        self.track(self.transport.open_tunnel(spec).await).await
    }
    async fn open_pty(&self, request: crate::PtyRequest) -> Result<crate::PtySession, HostError> {
        let line = pty_line(&request)?;
        self.track(self.transport.open_raw_pty(request, Some(line)).await)
            .await
    }

    // 文件方法由同一实现模块提供。
    async fn read_file(&self, path: &HostPath) -> Result<bytes::Bytes, HostError> {
        self.track(self.read(path).await).await
    }
    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        self.track(self.write(path, bytes).await).await
    }
    async fn list_dir(&self, path: &HostPath) -> Result<Vec<crate::DirEntry>, HostError> {
        self.list(path).await
    }
    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        self.mkdir(path).await
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        self.remove(path, false).await
    }
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        self.remove(path, true).await
    }
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), HostError> {
        self.track(self.move_path(from, to).await).await
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        self.path_exists(path).await
    }
    async fn file_size(&self, path: &HostPath) -> Result<Option<u64>, HostError> {
        self.size(path).await
    }
    async fn upload(&self, local: &std::path::Path, remote: &HostPath) -> Result<(), HostError> {
        self.track(self.put(local, remote).await).await
    }
    async fn download(&self, remote: &HostPath, local: &std::path::Path) -> Result<(), HostError> {
        self.track(self.get(remote, local).await).await
    }
    async fn download_url(&self, url: &str, dest: &HostPath) -> Result<(), HostError> {
        self.fetch(url, dest).await
    }
    async fn extract_archive(
        &self,
        archive: &HostPath,
        dest: &HostPath,
        kind: crate::ArchiveKind,
    ) -> Result<(), HostError> {
        self.extract(archive, dest, kind).await
    }
    async fn list_drives(&self) -> Result<Vec<crate::DriveEntry>, HostError> {
        self.drives().await
    }
}

fn invalid_json(error: serde_json::Error) -> HostError {
    HostError::InvalidArgument {
        reason: format!("Windows远端协议: {error}"),
    }
}
fn ensure_success(output: CommandOutput, program: &str) -> Result<(), HostError> {
    if output.success() {
        Ok(())
    } else {
        Err(HostError::command_failed(
            program,
            output.exit_code,
            output.stderr,
        ))
    }
}
pub(super) fn ps_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub fn powershell_line(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    format!(
        "powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand {}",
        STANDARD.encode(bytes)
    )
}

fn prepare_exec(line: String, command: &HostCommand) -> (String, HostCommand) {
    let mut command = command.clone();
    if line.len() <= 7_000 {
        // 非交互命令没有输入时也发送 SSH EOF，避免 Windows shell 等待后续输入。
        command.stdin.get_or_insert_with(Vec::new);
        return (line, command);
    }
    // 默认 OpenSSH shell 为 cmd.exe，长度上限 8191；长度帧避免 ConsoleStream 依赖 EOF 或预读 Agent stdin。
    let encoded = line.rsplit(' ').next().unwrap_or_default();
    let mut stdin =
        Vec::with_capacity(encoded.len() + 4 + command.stdin.as_ref().map_or(0, Vec::len));
    stdin.extend_from_slice(
        &u32::try_from(encoded.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    stdin.extend_from_slice(encoded.as_bytes());
    if let Some(payload) = command.stdin.take() {
        stdin.extend(payload);
    }
    command.stdin = Some(stdin);
    let bootstrap = format!(
        "$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue';{RAW_INPUT_SCRIPT}function Read-NcdExact([byte[]]$bytes){{$offset=0;while($offset -lt $bytes.Length){{$count=$ncdIn.Read($bytes,$offset,$bytes.Length-$offset);if($count -le 0){{throw 'Unexpected end of script frame'}};$offset+=$count}}}};$ncdSize=New-Object byte[] 4;Read-NcdExact $ncdSize;$ncdLength=[BitConverter]::ToUInt32($ncdSize,0);if($ncdLength -eq 0 -or $ncdLength -gt 65536){{throw 'Invalid script frame'}};$ncdHeader=New-Object byte[] $ncdLength;Read-NcdExact $ncdHeader;$ncdScript=[Text.Encoding]::Unicode.GetString([Convert]::FromBase64String([Text.Encoding]::ASCII.GetString($ncdHeader)));& ([ScriptBlock]::Create($ncdScript))"
    );
    (powershell_line(&bootstrap), command)
}

const WINDOWS_INFO_SCRIPT: &str = r#"$ErrorActionPreference='Stop';[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false);$id=[Security.Principal.WindowsIdentity]::GetCurrent();$principal=New-Object Security.Principal.WindowsPrincipal($id);$pd=[Environment]::GetFolderPath('CommonApplicationData');$homeDir=[Environment]::GetFolderPath('UserProfile');if(!$pd -or !$homeDir){throw 'Windows target directories unavailable'};$root=[IO.Path]::Combine($pd,'NapCatQQ Desktop','remote');$hash=[Security.Cryptography.SHA256]::Create();try{$sidKey=([BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($id.User.Value)))).Replace('-','').ToLowerInvariant()}finally{$hash.Dispose()};$arch=$env:PROCESSOR_ARCHITEW6432;if(!$arch){$arch=$env:PROCESSOR_ARCHITECTURE};[ordered]@{v=1;osVersion=[Environment]::OSVersion.VersionString;architecture=$arch;userSid=$id.User.Value;username=$id.Name;sessionId=[Diagnostics.Process]::GetCurrentProcess().SessionId;elevated=$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);homeDir=$homeDir;programData=$pd;tempDir=[IO.Path]::GetTempPath();managedRoot=$root;userRoot=[IO.Path]::Combine($root,'users',$sidKey);agent=$null}|ConvertTo-Json -Compress"#;

fn parse_windows_info(json: &str) -> Result<RemoteWindowsInfo, HostError> {
    let info: RemoteWindowsInfo =
        serde_json::from_str(json.trim().trim_start_matches('\u{feff}')).map_err(invalid_json)?;
    if info.v != WINDOWS_AGENT_PROTOCOL
        || info.home_dir.is_empty()
        || info.program_data.is_empty()
        || info.managed_root.is_empty()
        || info.user_root.is_empty()
        || info.user_sid.is_empty()
    {
        return Err(HostError::InvalidArgument {
            reason: "目标Windows身份或目录探测不完整".into(),
        });
    }
    Ok(info)
}

fn command_line(command: &HostCommand, interactive: bool) -> Result<String, HostError> {
    if command.program.contains('\0') || command.args.iter().any(|arg| arg.contains('\0')) {
        return Err(HostError::InvalidArgument {
            reason: "Windows命令含NUL字符".into(),
        });
    }
    let program = HostPath::windows_program_from_posix(&command.program);
    let prepared_args = powershell_arguments(&program, &command.args)?;
    let arguments = prepared_args
        .iter()
        .map(|arg| windows_argument(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let mut script = String::from(
        "$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue';[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false);",
    );
    if interactive {
        if let Some(cwd) = &command.working_dir {
            script.push_str(&format!(
                "Set-Location -LiteralPath {};",
                ps_literal(&cwd.render_for(Os::Windows))
            ));
        }
        for (key, value) in &command.environment {
            script.push_str(&format!(
                "[Environment]::SetEnvironmentVariable({},{},'Process');",
                ps_literal(key),
                ps_literal(value)
            ));
        }
        script.push_str(&format!(
            "& {} {};exit $LASTEXITCODE",
            ps_literal(&program),
            command
                .args
                .iter()
                .map(|arg| ps_literal(arg))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    } else {
        // 用进程标准字节流，避免 PowerShell 5.1 原生命令参数/空字符串及 stdin 重编码。
        script.push_str(&format!("$s=New-Object Diagnostics.ProcessStartInfo;$s.FileName={};$s.Arguments={};$s.UseShellExecute=$false;$s.CreateNoWindow=$true;$s.RedirectStandardInput=$true;$s.RedirectStandardOutput=$true;$s.RedirectStandardError=$true;", ps_literal(&program), ps_literal(&arguments)));
        if let Some(cwd) = &command.working_dir {
            script.push_str(&format!(
                "$s.WorkingDirectory={};Set-Location -LiteralPath $s.WorkingDirectory;",
                ps_literal(&cwd.render_for(Os::Windows))
            ));
        }
        for (key, value) in &command.environment {
            script.push_str(&format!("$s.EnvironmentVariables[{}]={};[Environment]::SetEnvironmentVariable({},{},'Process');", ps_literal(key), ps_literal(value), ps_literal(key), ps_literal(value)));
        }
        script.push_str("if(![IO.Path]::IsPathRooted($s.FileName)){$resolved=Get-Command -Name $s.FileName -CommandType Application -ErrorAction Stop|Select-Object -First 1;$s.FileName=$resolved.Source};");
        let args_literal = format!(
            "@({})",
            prepared_args
                .iter()
                .map(|argument| ps_literal(argument))
                .collect::<Vec<_>>()
                .join(",")
        );
        script.push_str(&format!("if([IO.Path]::GetExtension($s.FileName) -in @('.cmd','.bat')){{$values={args_literal};foreach($value in $values){{if($value.IndexOfAny([char[]]@(34,37,33,10,13)) -ge 0){{throw 'Batch arguments contain unsupported quotes or expansion characters; invoke the underlying executable instead'}}}};$s.Arguments='/d /s /c '+[char]34+[char]34+$s.FileName+[char]34+' '+$s.Arguments+[char]34;$s.FileName=$env:ComSpec}};"));
        script.push_str("$p=New-Object Diagnostics.Process;$p.StartInfo=$s;try{[void]$p.Start();$out=$p.StandardOutput.BaseStream.CopyToAsync([Console]::OpenStandardOutput());$err=$p.StandardError.BaseStream.CopyToAsync([Console]::OpenStandardError());");
        if let Some(input) = &command.stdin {
            // Windows SSH 的 ConsoleStream 可能收不到 EOF；只读本次请求约定的字节数。
            script.push_str(RAW_INPUT_SCRIPT);
            script.push_str(&format!("$ncdRemaining={};$ncdBuffer=New-Object byte[] 8192;while($ncdRemaining -gt 0){{$ncdRead=$ncdIn.Read($ncdBuffer,0,[int][Math]::Min($ncdRemaining,$ncdBuffer.Length));if($ncdRead -le 0){{throw 'Unexpected end of command input'}};$p.StandardInput.BaseStream.Write($ncdBuffer,0,$ncdRead);$ncdRemaining-=$ncdRead}};", input.len()));
        }
        script.push_str("$p.StandardInput.Close();$p.WaitForExit();[void]$out.GetAwaiter().GetResult();[void]$err.GetAwaiter().GetResult();exit $p.ExitCode}catch{[Console]::Error.WriteLine($_.Exception.Message);exit 1}");
    }
    let line = powershell_line(&script);
    if line.len() > 32_000 {
        return Err(HostError::InvalidArgument {
            reason: "Windows命令超过系统命令行上限，请改为上传脚本文件执行".into(),
        });
    }
    Ok(line)
}

fn windows_argument(value: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for character in value.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        if character == '"' {
            result.push_str(&"\\".repeat(slashes * 2 + 1));
        } else {
            result.push_str(&"\\".repeat(slashes));
        }
        slashes = 0;
        result.push(character);
    }
    result.push_str(&"\\".repeat(slashes * 2));
    result.push('"');
    result
}

fn powershell_arguments(program: &str, arguments: &[String]) -> Result<Vec<String>, HostError> {
    let name = program
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    let mut arguments = arguments.to_vec();
    if !matches!(
        name.as_str(),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe"
    ) {
        return Ok(arguments);
    }
    let index = arguments.iter().position(|argument| {
        matches!(
            argument.to_ascii_lowercase().as_str(),
            "-command" | "-encodedcommand"
        )
    });
    let Some(index) = index.filter(|index| *index + 1 < arguments.len()) else {
        return Ok(arguments);
    };
    if arguments[index + 1] == "-" {
        return Ok(arguments);
    }
    let encoded = arguments[index].eq_ignore_ascii_case("-EncodedCommand");
    let script = if encoded {
        let bytes =
            STANDARD
                .decode(&arguments[index + 1])
                .map_err(|_| HostError::InvalidArgument {
                    reason: "PowerShell EncodedCommand不是有效base64".into(),
                })?;
        if bytes.len() % 2 != 0 {
            return Err(HostError::InvalidArgument {
                reason: "PowerShell EncodedCommand必须使用UTF-16LE".into(),
            });
        }
        let units = bytes
            .chunks_exact(2)
            .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|_| HostError::InvalidArgument {
            reason: "PowerShell EncodedCommand包含无效UTF-16".into(),
        })?
    } else {
        arguments[index + 1].clone()
    };
    let script = format!(
        "$ProgressPreference='SilentlyContinue';[Console]::OutputEncoding=New-Object Text.UTF8Encoding($false);$OutputEncoding=[Console]::OutputEncoding;& {{\n{script}\n}}"
    );
    arguments[index + 1] = if encoded {
        STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    } else {
        script
    };
    Ok(arguments)
}

fn pty_line(request: &crate::PtyRequest) -> Result<String, HostError> {
    let mut command = match &request.program {
        crate::PtyProgram::LoginShell => {
            HostCommand::new("powershell.exe").args(["-NoLogo", "-NoProfile", "-NoExit"])
        }
        crate::PtyProgram::Program { program, args } => {
            HostCommand::new(program).args(args.clone())
        }
        crate::PtyProgram::Script(script) => {
            let mut prefix = String::from("$ErrorActionPreference='Stop';");
            if let Some(cwd) = &request.cwd {
                prefix.push_str(&format!(
                    "Set-Location -LiteralPath {};",
                    ps_literal(&cwd.render_for(Os::Windows))
                ));
            }
            for (key, value) in &request.env {
                prefix.push_str(&format!(
                    "[Environment]::SetEnvironmentVariable({},{},'Process');",
                    ps_literal(key),
                    ps_literal(value)
                ));
            }
            prefix.push_str(script);
            return checked_pty_line(powershell_line(&prefix).replace(" -NonInteractive", ""));
        }
    };
    command.working_dir = request.cwd.clone();
    command.environment = request.env.clone();
    checked_pty_line(command_line(&command, true)?.replace(" -NonInteractive", ""))
}

fn checked_pty_line(line: String) -> Result<String, HostError> {
    if line.len() > 7_000 {
        return Err(HostError::InvalidArgument {
            reason: "Windows终端启动命令过长，请将脚本上传到目标机后执行".into(),
        });
    }
    Ok(line)
}

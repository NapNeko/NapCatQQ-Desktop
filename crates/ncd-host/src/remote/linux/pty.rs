//! 远端交互终端：在现有 SSH 会话上开一条带 pty 的通道
//!
//! 只在开通道那一下拿会话锁，之后通道自己跑。主连接被自愈作废时，已开的终端通道
//! 仍握着 russh 会话的发送端，照常活到用户关掉或网络真断。

use std::time::Duration;

use bytes::Bytes;
use russh::ChannelMsg;
use russh::client::Msg;

use crate::command::HostCommand;
use crate::error::HostError;
use crate::host::Host;
use crate::pty::{
    PtyBackend, PtyExit, PtyInput, PtyProgram, PtyRequest, PtySession, pty_channel_pair,
};
use crate::shell::{BashShell, HostShell};

use super::{RemoteLinuxHost, build_remote_command_line};

/// 远端看到的终端类型；xterm.js 支持 256 色和真彩色
const TERM_NAME: &str = "xterm-256color";

/// 关通道时最多等多久，sshd 偶尔要等前台进程收完 SIGHUP 才回 Close
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

impl RemoteLinuxHost {
    pub(super) async fn open_pty_channel(&self, req: PtyRequest) -> Result<PtySession, HostError> {
        let line = remote_pty_command_line(&req);
        let channel = {
            let guard = self.handle.lock().await;
            let Some(session) = guard.as_ref() else {
                return Err(HostError::remote_disconnected("ssh session poisoned"));
            };
            match session.channel_open_session().await {
                Ok(channel) => channel,
                Err(e) => {
                    drop(guard);
                    Host::invalidate_connection(self).await;
                    return Err(HostError::remote_disconnected(format!("open channel: {e}")));
                }
            }
        };

        channel
            .request_pty(
                true,
                TERM_NAME,
                u32::from(req.size.cols),
                u32::from(req.size.rows),
                0,
                0,
                &[],
            )
            .await
            .map_err(|e| HostError::remote_disconnected(format!("request pty: {e}")))?;
        let started = match &line {
            None => channel.request_shell(true).await,
            Some(line) => channel.exec(true, line.as_bytes()).await,
        };
        started.map_err(|e| HostError::remote_disconnected(format!("start shell: {e}")))?;

        let (session, backend) = pty_channel_pair();
        tokio::spawn(drive_remote_pty(channel, backend));
        Ok(session)
    }
}

/// 把请求拼成交给登录 shell 的一行；None 表示直接要登录 shell
pub(super) fn remote_pty_command_line(req: &PtyRequest) -> Option<String> {
    let shell = BashShell;
    match &req.program {
        PtyProgram::LoginShell => None,
        PtyProgram::Program { program, args } => {
            let mut cmd = HostCommand::new(program.clone()).args(args.iter().cloned());
            for (key, value) in req.env.iter().filter(|(k, _)| is_env_name(k)) {
                cmd = cmd.env(key.clone(), value.clone());
            }
            if let Some(cwd) = &req.cwd {
                cmd = cmd.working_dir(cwd.clone());
            }
            Some(build_remote_command_line(&shell, &cmd))
        }
        PtyProgram::Script(script) => {
            let mut line = String::new();
            if let Some(cwd) = &req.cwd {
                line.push_str("cd ");
                line.push_str(&shell.escape(cwd.as_posix()));
                line.push_str(" 2>/dev/null; ");
            }
            for (key, value) in req.env.iter().filter(|(k, _)| is_env_name(k)) {
                line.push_str("export ");
                line.push_str(key);
                line.push('=');
                line.push_str(&shell.escape(value));
                line.push_str("; ");
            }
            line.push_str(script);
            Some(line)
        }
    }
}

/// 变量名只收 shell 认的标识符，防止拼进命令行时被当成语法
fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 通道的主循环：输入、改大小、输出三路一起转
///
/// 输出发不出去（上层读慢了）时先攒一块、暂停读通道，但输入照收，
/// 刷屏时按 Ctrl+C 仍能送到远端
async fn drive_remote_pty(mut channel: russh::Channel<Msg>, mut backend: PtyBackend) {
    let mut pending: Option<Bytes> = None;
    let mut exit_code: Option<i32> = None;
    let mut exited = false;

    let outcome = loop {
        tokio::select! {
            biased;
            input = backend.input.recv() => match input {
                Some(PtyInput::Write(data)) => {
                    if let Err(e) = channel.data(&data[..]).await {
                        break PtyExit::Disconnected(format!("写入失败: {e}"));
                    }
                }
                Some(PtyInput::Resize(size)) => {
                    let _ = channel
                        .window_change(u32::from(size.cols), u32::from(size.rows), 0, 0)
                        .await;
                }
                Some(PtyInput::Close) | None => {
                    close_channel(&channel).await;
                    break PtyExit::Exited(exit_code);
                }
            },
            permit = backend.output.reserve(), if pending.is_some() => match permit {
                Ok(permit) => {
                    if let Some(bytes) = pending.take() {
                        permit.send(bytes);
                    }
                }
                Err(_) => {
                    close_channel(&channel).await;
                    break PtyExit::Exited(exit_code);
                }
            },
            msg = channel.wait(), if pending.is_none() => match msg {
                Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    pending = Some(Bytes::copy_from_slice(&data));
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exited = true;
                    exit_code = i32::try_from(exit_status).ok();
                }
                Some(ChannelMsg::ExitSignal { .. }) => {
                    exited = true;
                    exit_code = None;
                }
                Some(ChannelMsg::Failure) => {
                    close_channel(&channel).await;
                    break PtyExit::Disconnected("服务器拒绝了终端请求".into());
                }
                Some(ChannelMsg::Close) => break closed_outcome(exited, exit_code),
                None => break closed_outcome(exited, exit_code),
                Some(_) => {}
            },
        }
    };

    // 循环外还剩一块没交出去的输出：尽量送到，上层可能正等着看最后一行
    if let Some(bytes) = pending.take() {
        let _ = backend.output.send(bytes).await;
    }
    let _ = backend.exit.send(outcome);
}

fn closed_outcome(exited: bool, exit_code: Option<i32>) -> PtyExit {
    if exited {
        PtyExit::Exited(exit_code)
    } else {
        PtyExit::Disconnected("连接已断开".into())
    }
}

async fn close_channel(channel: &russh::Channel<Msg>) {
    let _ = channel.eof().await;
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, channel.close()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::HostPath;
    use crate::pty::PtySize;

    fn request(program: PtyProgram) -> PtyRequest {
        PtyRequest::new(program, PtySize::default())
    }

    #[test]
    fn login_shell_asks_for_a_plain_shell() {
        assert_eq!(
            remote_pty_command_line(&request(PtyProgram::LoginShell)),
            None
        );
    }

    #[test]
    fn program_gets_cd_and_env_prefix() {
        let mut req = request(PtyProgram::Program {
            program: "docker".into(),
            args: vec!["exec".into(), "-it".into(), "ncbot-1".into(), "sh".into()],
        });
        req.cwd = Some(HostPath::from_posix("/home/u/my dir"));
        req.env.insert("LANG".into(), "C.UTF-8".into());
        let line = remote_pty_command_line(&req).unwrap_or_default();
        assert_eq!(
            line,
            "cd '/home/u/my dir' && LANG=C.UTF-8 docker exec -it ncbot-1 sh"
        );
    }

    #[test]
    fn script_keeps_body_and_skips_bad_env_names() {
        let mut req = request(PtyProgram::Script("exec bash -l".into()));
        req.cwd = Some(HostPath::from_posix("/srv/app"));
        req.env
            .insert("VIRTUAL_ENV".into(), "/srv/app/.venv".into());
        req.env.insert("BAD;rm -rf".into(), "x".into());
        let line = remote_pty_command_line(&req).unwrap_or_default();
        assert_eq!(
            line,
            "cd /srv/app 2>/dev/null; export VIRTUAL_ENV=/srv/app/.venv; exec bash -l"
        );
    }

    #[test]
    fn env_names_are_shell_identifiers() {
        assert!(is_env_name("PATH"));
        assert!(is_env_name("_x1"));
        assert!(!is_env_name("1X"));
        assert!(!is_env_name("A-B"));
        assert!(!is_env_name(""));
    }
}

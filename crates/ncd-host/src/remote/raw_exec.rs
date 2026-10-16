//! 不附加平台 shell 语法的 SSH exec。
use std::time::Duration;

use russh::ChannelMsg;

use super::linux::{RemoteLinuxHost, force_kill_remote_channel};
use crate::command::DEFAULT_COMMAND_TIMEOUT;
use crate::{CommandOutput, HostCommand, HostError, StreamSource};

impl RemoteLinuxHost {
    pub(super) async fn run_raw(
        &self,
        line: &str,
        cmd: &HostCommand,
        mut on_line: Option<Box<dyn FnMut(StreamSource, String) + Send>>,
        max_output: usize,
    ) -> Result<CommandOutput, HostError> {
        let timeout = cmd.timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT);
        let deadline = tokio::time::Instant::now() + timeout;
        let mut channel =
            tokio::time::timeout_at(deadline, self.open_raw_channel(line, cmd.stdin.as_deref()))
                .await
                .map_err(|_| HostError::Timeout {
                    operation: "ssh_exec_open",
                })??;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut stdout_pending = Vec::new();
        let mut stderr_pending = Vec::new();
        let mut exit_code = None;
        loop {
            let next = async {
                if let Some(cancel) = &cmd.cancel {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => Err(HostError::Cancelled),
                        message = channel.wait() => Ok(message),
                    }
                } else {
                    Ok(channel.wait().await)
                }
            };
            let message = match tokio::time::timeout_at(deadline, next).await {
                Ok(Ok(message)) => message,
                result => {
                    force_kill_remote_channel(&channel).await;
                    return Err(match result {
                        Ok(Err(error)) => error,
                        _ => HostError::Timeout {
                            operation: "ssh_exec",
                        },
                    });
                }
            };
            match message {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                    if let Some(callback) = on_line.as_mut() {
                        crate::stream_chunk::feed_stream_chunk(
                            &mut stdout_pending,
                            &data,
                            |line| callback(StreamSource::Stdout, line),
                        );
                    }
                }
                Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                    stderr.extend_from_slice(&data);
                    if let Some(callback) = on_line.as_mut() {
                        crate::stream_chunk::feed_stream_chunk(
                            &mut stderr_pending,
                            &data,
                            |line| callback(StreamSource::Stderr, line),
                        );
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    exit_code = i32::try_from(exit_status).ok()
                }
                Some(ChannelMsg::ExitSignal { .. }) => exit_code = None,
                Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
            if stdout.len().saturating_add(stderr.len()) > max_output {
                force_kill_remote_channel(&channel).await;
                return Err(HostError::InvalidArgument {
                    reason: "远端命令输出超过允许大小".into(),
                });
            }
        }
        if let Some(callback) = on_line.as_mut() {
            crate::stream_chunk::flush_stream_remainder(&mut stdout_pending, |line| {
                callback(StreamSource::Stdout, line)
            });
            crate::stream_chunk::flush_stream_remainder(&mut stderr_pending, |line| {
                callback(StreamSource::Stderr, line)
            });
        }
        if exit_code.is_none() {
            return Err(HostError::remote_disconnected(
                "SSH channel closed without an exit status",
            ));
        }
        Ok(CommandOutput {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }
}

pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

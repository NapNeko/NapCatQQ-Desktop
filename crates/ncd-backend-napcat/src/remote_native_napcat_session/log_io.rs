//! 按文件身份和字节偏移读取有上限的远端日志，避免每次重传整份日志。
use std::time::Duration;

use ncd_host::{Host, HostCommand, HostError};

pub(super) const LOG_READ_TIMEOUT: Duration = Duration::from_secs(8);
const LOG_CHUNK_BYTES: u64 = 512 * 1024;
const READ_LOG: &str = r#"import os, sys
path, offset, previous, seed, limit = sys.argv[1:]
try:
    with open(path, 'rb') as stream:
        info = os.fstat(stream.fileno())
        identity = str(info.st_dev) + ':' + str(info.st_ino)
        offset, limit = int(offset), int(limit)
        reset = previous != identity or offset > info.st_size
        start = max(0, info.st_size - limit) if seed == '1' else (0 if reset else offset)
        stream.seek(start)
        data = stream.read(limit)
        sys.stdout.buffer.write((identity + ' ' + str(start) + ' ' + str(len(data)) + '\n').encode() + data)
except FileNotFoundError:
    print('missing')
"#;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct LogCursor {
    identity: String,
    offset: u64,
}

pub(super) struct LogChunk {
    pub cursor: LogCursor,
    pub text: String,
    pub reset: bool,
}

fn decode(output: String, previous: &LogCursor) -> Result<Option<LogChunk>, HostError> {
    if output.trim() == "missing" {
        return Ok(None);
    }
    let invalid = || HostError::Io(std::io::Error::other("远端日志读取返回无效偏移"));
    let (header, text) = output.split_once('\n').ok_or_else(invalid)?;
    let mut fields = header.split_whitespace();
    let identity = fields.next().ok_or_else(invalid)?;
    let (device, inode) = identity.split_once(':').ok_or_else(invalid)?;
    if device.parse::<u64>().is_err() || inode.parse::<u64>().is_err() {
        return Err(invalid());
    }
    let start = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(invalid)?;
    let count = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(invalid)?;
    if fields.next().is_some()
        || count > LOG_CHUNK_BYTES
        || text.len() > LOG_CHUNK_BYTES as usize * 3
    {
        return Err(invalid());
    }
    let offset = start.checked_add(count).ok_or_else(invalid)?;
    Ok(Some(LogChunk {
        reset: previous.identity != identity || start != previous.offset,
        cursor: LogCursor {
            identity: identity.to_owned(),
            offset,
        },
        text: text.to_owned(),
    }))
}

pub(super) async fn read_log(
    host: &dyn Host,
    path: &str,
    cursor: &LogCursor,
    seed: bool,
) -> Result<Option<LogChunk>, HostError> {
    let command = HostCommand::new("python3")
        .args(["-I", "-B", "-c", READ_LOG, path])
        .arg(cursor.offset.to_string())
        .arg(&cursor.identity)
        .arg(if seed { "1" } else { "0" })
        .arg(LOG_CHUNK_BYTES.to_string())
        .timeout(LOG_READ_TIMEOUT);
    let output = tokio::time::timeout(LOG_READ_TIMEOUT, host.run_to_string(command))
        .await
        .map_err(|_| HostError::Timeout {
            operation: "napcat_log_read",
        })??;
    if !output.success() {
        return Err(HostError::Io(std::io::Error::other(
            "读取远端 NapCat 日志失败",
        )));
    }
    decode(output.stdout, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_tracks_remote_bytes_and_detects_equal_length_rotation() {
        let first = decode("1:2 100 4\ntext".into(), &LogCursor::default())
            .unwrap()
            .unwrap();
        assert_eq!(first.cursor.offset, 104);
        let appended = decode("1:2 104 3\nnew".into(), &first.cursor)
            .unwrap()
            .unwrap();
        assert!(!appended.reset);
        let rotated = decode("1:3 0 4\ntext".into(), &first.cursor)
            .unwrap()
            .unwrap();
        assert!(rotated.reset);
        assert_eq!(rotated.cursor.offset, 4);
    }

    #[test]
    fn malformed_or_unbounded_log_responses_are_rejected() {
        for output in [
            "not-a-frame",
            "1:2 0 524289\n",
            "1:2 18446744073709551615 1\nx",
        ] {
            assert!(decode(output.into(), &LogCursor::default()).is_err());
        }
        assert!(
            decode("missing\n".into(), &LogCursor::default())
                .unwrap()
                .is_none()
        );
    }
}

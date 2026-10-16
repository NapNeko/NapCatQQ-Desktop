//! 限长协议帧，拒绝未终止和超长输入。
use ncd_domain::windows_remote::WINDOWS_AGENT_MAX_FRAME;
use serde::{Serialize, de::DeserializeOwned};
use std::io::{BufRead, Write};

pub fn read_frame<T: DeserializeOwned>(reader: &mut impl BufRead) -> Result<Option<T>, String> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().map_err(|e| e.to_string())?;
        if buffer.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err("unterminated protocol frame".into())
            };
        }
        let count = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buffer.len(), |n| n + 1);
        if bytes.len() + count > WINDOWS_AGENT_MAX_FRAME {
            return Err("protocol frame exceeds limit".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        let done = bytes.last() == Some(&b'\n');
        reader.consume(count);
        if done {
            break;
        }
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid protocol frame: {e}"))
}

pub fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() + 1 > WINDOWS_AGENT_MAX_FRAME {
        return Err("protocol frame exceeds limit".into());
    }
    writer
        .write_all(&bytes)
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truncated_frame_is_never_accepted() {
        assert!(
            read_frame::<serde_json::Value>(&mut std::io::Cursor::new(b"{}".as_slice())).is_err()
        );
    }
    #[test]
    fn reads_only_one_request_at_a_time() {
        let mut input = std::io::Cursor::new(b"1\n2\n".as_slice());
        assert_eq!(read_frame::<u32>(&mut input).unwrap(), Some(1));
        assert_eq!(read_frame::<u32>(&mut input).unwrap(), Some(2));
    }
    #[test]
    fn oversized_unterminated_input_stops_at_the_frame_boundary() {
        let input = vec![b'x'; WINDOWS_AGENT_MAX_FRAME + 1];
        let mut input = std::io::Cursor::new(input);
        assert!(read_frame::<serde_json::Value>(&mut input).is_err());
        assert!(input.position() <= WINDOWS_AGENT_MAX_FRAME as u64);
    }
}

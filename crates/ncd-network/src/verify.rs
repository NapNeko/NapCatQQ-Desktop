//! SHA256 校验：下载完成后比对期望哈希，mismatch 让上层切镜像。
//!
//! race / chunked 都在下载结束后调一次。提取到一处避免两份副本漂移
//! （国内代理"长度对、内容是另一份缓存"的投毒 case 全靠这层兜底）。

use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::error::NetworkError;

/// 算 dest 文件的 SHA256（64-hex 小写），与 expected 严格比对。expected 为 None/空串
/// 视为无 hash 数据跳过，直接返回字节数。mismatch 返 ChecksumMismatch 让上层切镜像。
pub(crate) async fn verify_sha256_if_needed(
    dest: &Path,
    expected: Option<&str>,
) -> Result<u64, NetworkError> {
    let metadata = fs::metadata(dest).await?;
    let size = metadata.len();
    let expected = match expected {
        Some(h) if !h.is_empty() => h,
        _ => return Ok(size),
    };

    let mut file = fs::File::open(dest).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = hex::encode(hasher.finalize());
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(NetworkError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(size)
}

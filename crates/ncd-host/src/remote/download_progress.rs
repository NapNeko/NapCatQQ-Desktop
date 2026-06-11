//! 远程下载进度解析器：解析 wget/curl 的进度输出。

use std::time::{Duration, Instant};

/// 下载进度更新。
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// 已下载字节数
    pub downloaded_bytes: u64,
    /// 百分比（0-100）
    pub percent: u8,
    /// 速度（字节/秒）
    pub speed_bps: u64,
}

/// wget 进度解析器。
///
/// wget --progress=dot:mega 输出格式：
/// ```text
///      0K ........ ........ ........ ........ ........  2% 1.23M 5s
///   2048K ........ ........ ........ ........ ........  5% 2.45M 3s
/// ```
pub struct WgetProgressParser {
    last_update: Option<Instant>,
    update_interval: Duration,
}

impl WgetProgressParser {
    pub fn new() -> Self {
        Self {
            last_update: None,
            update_interval: Duration::from_secs(1),
        }
    }

    /// 解析一行输出，返回进度更新（限流：每秒最多 1 次）。
    pub fn parse_line(&mut self, line: &str) -> Option<DownloadProgress> {
        // 限流
        if let Some(last) = self.last_update {
            if last.elapsed() < self.update_interval {
                return None;
            }
        }

        // 正则匹配：(\d+)K.*?(\d+)%.*?(\d+\.\d+)([KMG])
        let line = line.trim();
        if !line.contains('%') {
            return None;
        }

        // 提取百分比
        let percent = line
            .split('%')
            .next()?
            .split_whitespace()
            .last()?
            .parse::<u8>()
            .ok()?;

        // 提取已下载量（K 为单位）
        let kb_str = line.split_whitespace().next()?.trim_end_matches('K');
        let downloaded_kb = kb_str.parse::<u64>().ok()?;
        let downloaded_bytes = downloaded_kb * 1024;

        // 提取速度（简化：匹配 "1.23M" 格式）
        let speed_bps = parse_speed_from_line(line).unwrap_or(0);

        self.last_update = Some(Instant::now());

        Some(DownloadProgress {
            downloaded_bytes,
            percent,
            speed_bps,
        })
    }
}

/// curl 进度解析器。
///
/// curl --progress-bar 输出格式：
/// ```text
/// ###                                                    5.2%
/// ######                                                10.5%
/// ```
pub struct CurlProgressParser {
    last_update: Option<Instant>,
    update_interval: Duration,
}

impl CurlProgressParser {
    pub fn new() -> Self {
        Self {
            last_update: None,
            update_interval: Duration::from_secs(1),
        }
    }

    pub fn parse_line(&mut self, line: &str) -> Option<DownloadProgress> {
        if let Some(last) = self.last_update {
            if last.elapsed() < self.update_interval {
                return None;
            }
        }

        let line = line.trim();
        if !line.contains('%') {
            return None;
        }

        // 提取百分比（格式：### 10.5%）
        let percent_str = line.split('%').next()?.split_whitespace().last()?;
        let percent_f = percent_str.parse::<f32>().ok()?;
        let percent = percent_f.round() as u8;

        self.last_update = Some(Instant::now());

        Some(DownloadProgress {
            downloaded_bytes: 0, // curl 进度条不显示字节数
            percent,
            speed_bps: 0,
        })
    }
}

/// 从行中解析速度（"1.23M" → 字节/秒）。
fn parse_speed_from_line(line: &str) -> Option<u64> {
    // 简化实现：查找类似 "1.23M" 的速度标识
    for token in line.split_whitespace() {
        if let Some(num_part) = token.strip_suffix('M') {
            if let Ok(val) = num_part.parse::<f64>() {
                return Some((val * 1024.0 * 1024.0) as u64);
            }
        }
        if let Some(num_part) = token.strip_suffix('K') {
            if let Ok(val) = num_part.parse::<f64>() {
                return Some((val * 1024.0) as u64);
            }
        }
        if let Some(num_part) = token.strip_suffix('G') {
            if let Ok(val) = num_part.parse::<f64>() {
                return Some((val * 1024.0 * 1024.0 * 1024.0) as u64);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wget_progress_parser() {
        let mut parser = WgetProgressParser::new();
        let line = "  2048K ........ ........ ........ ........  5% 2.45M 3s";
        let progress = parser.parse_line(line).unwrap();
        assert_eq!(progress.percent, 5);
        assert_eq!(progress.downloaded_bytes, 2048 * 1024);
    }

    #[test]
    fn test_curl_progress_parser() {
        let mut parser = CurlProgressParser::new();
        let line = "######                                                10.5%";
        let progress = parser.parse_line(line).unwrap();
        assert_eq!(progress.percent, 11); // 四舍五入
    }

    #[test]
    fn test_parse_speed() {
        assert_eq!(parse_speed_from_line("2048K ... 2.45M 3s"), Some(2_560_000));
        assert_eq!(parse_speed_from_line("... 512K ..."), Some(524_288));
    }
}

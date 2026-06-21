//! 解析 wget / curl 的进度输出

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub percent: u8,
    pub speed_bps: u64,
}

/// wget --progress=dot:mega 每行形如 "2048K ........  5% 2.45M 3s"
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

    /// 每秒最多返回一次,避免刷爆 IPC
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

        let percent = line
            .split('%')
            .next()?
            .split_whitespace()
            .last()?
            .parse::<u8>()
            .ok()?;

        let kb_str = line.split_whitespace().next()?.trim_end_matches('K');
        let downloaded_kb = kb_str.parse::<u64>().ok()?;
        let downloaded_bytes = downloaded_kb * 1024;

        let speed_bps = parse_speed_from_line(line).unwrap_or(0);

        self.last_update = Some(Instant::now());

        Some(DownloadProgress {
            downloaded_bytes,
            percent,
            speed_bps,
        })
    }
}

/// curl --progress-bar 进度行形如 "######  10.5%",只含百分比
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

fn parse_speed_from_line(line: &str) -> Option<u64> {
    // wget 行里已下载量在 % 之前,速度在 % 之后,靠 % 定界才不会把已下载量当速度
    let after_percent = line.split('%').nth(1)?;
    for token in after_percent.split_whitespace() {
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
        // 2048K 是已下载量(% 前),2.5M 是速度(% 后),应取后者
        assert_eq!(
            parse_speed_from_line("2048K ........  5% 2.5M 3s"),
            Some(2_621_440)
        );
        assert_eq!(parse_speed_from_line("5% 512K 3s"), Some(524_288));
    }
}

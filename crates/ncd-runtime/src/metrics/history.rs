use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use ncd_domain::MetricsHistoryPoint;

pub fn append_history_point(path: &Path, point: &MetricsHistoryPoint) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir history: {e}"))?;
    }
    let line = serde_json::to_string(point).map_err(|e| format!("serialize history: {e}"))?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open history: {e}"))?;
    writeln!(f, "{line}").map_err(|e| format!("write history: {e}"))?;
    Ok(())
}

pub fn load_history(
    path: &Path,
    from_ms: Option<u64>,
    to_ms: Option<u64>,
) -> Result<Vec<MetricsHistoryPoint>, String> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let f = File::open(path).map_err(|e| format!("read history: {e}"))?;
    let reader = BufReader::new(f);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read history line: {e}"))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let point: MetricsHistoryPoint = match serde_json::from_str(line) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if let Some(from) = from_ms {
            if point.at_ms < from {
                continue;
            }
        }
        if let Some(to) = to_ms {
            if point.at_ms > to {
                continue;
            }
        }
        out.push(point);
    }
    Ok(out)
}

pub fn prune_history_file(path: &Path, retention_days: u32, now_ms: u64) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let retention_ms = u64::from(retention_days.max(1)).saturating_mul(86_400_000);
    let cutoff = now_ms.saturating_sub(retention_ms);
    let points = load_history(path, Some(cutoff), None)?;
    // 条数软顶：约 5 万
    const HARD_CAP: usize = 50_000;
    let points = if points.len() > HARD_CAP {
        points[points.len() - HARD_CAP..].to_vec()
    } else {
        points
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir history: {e}"))?;
    }
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut f = File::create(&tmp).map_err(|e| format!("create history tmp: {e}"))?;
        for p in &points {
            let line = serde_json::to_string(p).map_err(|e| e.to_string())?;
            writeln!(f, "{line}").map_err(|e| e.to_string())?;
        }
    }
    fs::rename(&tmp, path).map_err(|e| format!("rename history: {e}"))?;
    Ok(())
}

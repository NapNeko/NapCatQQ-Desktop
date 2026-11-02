//! JSONL 流式加载、按需补读与压缩修复；不改变已落盘的历史内容。

use std::collections::{HashSet, VecDeque};
use std::io::ErrorKind;
use std::path::Path;

use ncd_domain::onebot_debug::{DebugHistoryEntry, DebugStorageNotice};
use serde::Deserialize;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
};

use super::super::params::serialized_len;
use super::files::{commit_tmp, create_tmp, discard_tmp, notice, quarantine, strip_bom};
use super::history::{HISTORY_KEEP, HistoryMem};

pub(super) const HISTORY_FILE: &str = "history.jsonl";
/// 留出追加余量；失败退避也由文件保留策略定义。
pub(super) const HISTORY_COMPACT_AT: usize = 1200;
pub(super) const COMPACT_BACKOFF: usize = 200;
const READ_ONLY_RESPONSE_BUDGET: usize = 1024 * 1024;
pub(super) const READ_ONLY_HISTORY_LINE_LIMIT: u64 = 1024 * 1024;
pub(super) const READ_ONLY_HISTORY_SCAN_LIMIT: u64 = 32 * 1024 * 1024;
pub(super) const READ_ONLY_HISTORY_RETAINED_LIMIT: usize = 8 * 1024 * 1024;

/// MCP 冷读取只验证内容与资源限制，不隔离、修复或压缩 GUI 文件。
pub(super) async fn load_history_read_only(dir: &Path) -> Result<(HistoryMem, usize), String> {
    match tokio::fs::File::open(dir.join(HISTORY_FILE)).await {
        Ok(file) => {
            let scan = scan_history_bounded(file, READ_ONLY_RESPONSE_BUDGET, true)
                .await
                .map_err(|_| "无法只读加载调用历史".to_owned())?;
            if scan.bad > 0 {
                return Err("调用历史含损坏记录，请在桌面调试台处理".to_owned());
            }
            Ok((scan.mem, scan.lines))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Ok((HistoryMem::new(READ_ONLY_RESPONSE_BUDGET), 0))
        }
        Err(_) => Err("无法只读加载调用历史".to_owned()),
    }
}

/// 去掉行尾的 `\n` / `\r\n`（和行首可能有的 BOM）
fn trim_line(buf: &[u8]) -> &[u8] {
    let line = buf.strip_suffix(b"\n").unwrap_or(buf);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    strip_bom(line)
}

fn is_blank(line: &[u8]) -> bool {
    line.iter().all(u8::is_ascii_whitespace)
}

/// 读 `history.jsonl`：一行一条，坏行跳过不连累好行。流式读，内存里只留最新 1000 条，
/// 回包全文按预算留，不会因为文件大就把所有回包都读进来。
/// 返回（内存里的历史，文件现有行数）
pub(super) async fn load_history(
    dir: &Path,
    budget: usize,
    notices: &mut Vec<DebugStorageNotice>,
) -> (HistoryMem, usize) {
    let path = dir.join(HISTORY_FILE);
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return (HistoryMem::new(budget), 0),
        Err(e) => return unreadable_history(&path, budget, notices, &e).await,
    };
    let scan = match scan_history(file, budget).await {
        Ok(scan) => scan,
        Err(e) => return unreadable_history(&path, budget, notices, &e).await,
    };

    let mut lines = scan.lines;
    if scan.bad > 0 {
        // 原件整份挪开留底，好行（最新 1000 条）按行复制回正本；这样坏行不会每次启动都再报一遍，
        // 磁盘上的行数也和内存对得上
        let reason = format!(
            "{} 行历史记录无法解析，已跳过（第 {} 行：{}），其余 {} 条照常保留",
            scan.bad, scan.first_bad_line, scan.first_error, scan.good_count
        );
        match quarantine(&path).await {
            Ok(moved) => {
                let keep: HashSet<usize> = scan.good_tail.iter().copied().collect();
                lines = match rewrite_lines(&moved, &path, |ordinal| keep.contains(&ordinal)).await
                {
                    Ok(kept) => kept,
                    Err(e) => {
                        // 正本已经挪走了，内存里还有；下次追加会从空文件重新开始
                        tracing::warn!("重写调用历史失败: {e}");
                        0
                    }
                };
                notices.push(notice(HISTORY_FILE, Ok(moved), reason));
            }
            Err(e) => notices.push(notice(HISTORY_FILE, Err(e), reason)),
        }
    } else if scan.lines > HISTORY_COMPACT_AT || !scan.ends_with_newline {
        // 太长要压缩；结尾缺换行（崩在追加中途，但那行恰好写完整了）也要重写一遍，
        // 否则下一条追加会和它粘成一行，两条都读不出来
        let skip = scan.lines.saturating_sub(HISTORY_KEEP);
        match rewrite_lines(&path, &path, |ordinal| ordinal >= skip).await {
            Ok(kept) => lines = kept,
            Err(e) => tracing::warn!("重写调用历史失败: {e}"),
        }
    }
    (scan.mem, lines)
}

async fn unreadable_history(
    path: &Path,
    budget: usize,
    notices: &mut Vec<DebugStorageNotice>,
    error: &std::io::Error,
) -> (HistoryMem, usize) {
    let moved = quarantine(path).await;
    notices.push(notice(HISTORY_FILE, moved, format!("读取失败: {error}")));
    (HistoryMem::new(budget), 0)
}

struct HistoryScan {
    mem: HistoryMem,
    /// 非空行总数（含读不出来的）
    lines: usize,
    bad: usize,
    /// 第一处坏行的物理行号（从 1 数）和原因，给提示用
    first_bad_line: usize,
    first_error: String,
    /// 最新至多 1000 条有效行的「非空行序号」，坏行重写时按它挑行
    good_tail: VecDeque<usize>,
    good_count: usize,
    ends_with_newline: bool,
}

async fn scan_history(file: tokio::fs::File, budget: usize) -> std::io::Result<HistoryScan> {
    scan_history_bounded(file, budget, false).await
}

async fn scan_history_bounded(
    file: tokio::fs::File,
    budget: usize,
    read_only: bool,
) -> std::io::Result<HistoryScan> {
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut scan = HistoryScan {
        mem: HistoryMem::new(budget),
        lines: 0,
        bad: 0,
        first_bad_line: 0,
        first_error: String::new(),
        good_tail: VecDeque::new(),
        good_count: 0,
        ends_with_newline: true,
    };
    let mut physical = 0usize;
    let mut scanned_bytes = 0;
    let mut retained_bytes = 0usize;
    let mut retained_lines = VecDeque::new();
    loop {
        buf.clear();
        let count = if read_only {
            read_history_line_bounded(&mut reader, &mut buf, &mut scanned_bytes).await?
        } else {
            reader.read_until(b'\n', &mut buf).await?
        };
        if count == 0 {
            break;
        }
        physical += 1;
        scan.ends_with_newline = buf.ends_with(b"\n");
        let line = trim_line(&buf);
        if is_blank(line) {
            continue;
        }
        let ordinal = scan.lines;
        scan.lines += 1;
        match serde_json::from_slice::<DebugHistoryEntry>(line) {
            Ok(entry) => {
                if read_only {
                    // 回包预算不包含 params/error；按完整原始行计费，限制累计保留量。
                    retained_bytes += count;
                    retained_lines.push_back(count);
                    if retained_lines.len() > HISTORY_KEEP {
                        retained_bytes -= retained_lines.pop_front().unwrap_or(0);
                    }
                    if retained_bytes > READ_ONLY_HISTORY_RETAINED_LIMIT {
                        return Err(std::io::Error::new(
                            ErrorKind::InvalidData,
                            "history retained entries exceed read limit",
                        ));
                    }
                }
                let bytes = entry
                    .response
                    .as_ref()
                    .and_then(serialized_len)
                    .unwrap_or(0);
                // 每读一条就限制回包预算，不累计整个文件的回包。
                scan.mem.push(entry, bytes, true);
                scan.good_count += 1;
                scan.good_tail.push_back(ordinal);
                if scan.good_tail.len() > HISTORY_KEEP {
                    scan.good_tail.pop_front();
                }
            }
            Err(e) => {
                if scan.bad == 0 {
                    scan.first_bad_line = physical;
                    scan.first_error = e.to_string();
                }
                scan.bad += 1;
            }
        }
    }
    Ok(scan)
}

async fn read_history_line_bounded(
    reader: &mut (impl AsyncBufRead + Unpin),
    buf: &mut Vec<u8>,
    scanned_bytes: &mut u64,
) -> std::io::Result<usize> {
    let limit = READ_ONLY_HISTORY_LINE_LIMIT
        .min(READ_ONLY_HISTORY_SCAN_LIMIT.saturating_sub(*scanned_bytes));
    let count = reader.take(limit + 1).read_until(b'\n', buf).await?;
    if count as u64 > limit {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "history line or total scan exceeds read limit",
        ));
    }
    *scanned_bytes += count as u64;
    Ok(count)
}

#[derive(Deserialize)]
struct HistoryId {
    id: String,
}

fn has_history_id(line: &[u8], id: &str, prefix: &[u8]) -> bool {
    if line.starts_with(prefix) {
        return true;
    }
    if let Some(value) = line.strip_prefix(b"{\"id\":") {
        // 正常写入的行将 id 放在最前；不同 ID 只解码这个字符串，不解析回包。
        let mut deserializer = serde_json::Deserializer::from_slice(value);
        return String::deserialize(&mut deserializer).is_ok_and(|candidate| candidate == id);
    }
    // 加载器同样接受字段重排、空白和转义；补读不能因 JSON 字面格式不同而丢掉全文。
    serde_json::from_slice::<HistoryId>(line).is_ok_and(|candidate| candidate.id == id)
}

pub(super) async fn read_history_entry_bounded(
    path: &Path,
    id: &str,
) -> std::io::Result<Option<DebugHistoryEntry>> {
    let prefix = format!(
        "{{\"id\":{},",
        serde_json::to_string(id).map_err(std::io::Error::other)?
    );
    let file = tokio::fs::File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut scanned_bytes = 0;
    let mut found = None;
    loop {
        buf.clear();
        if read_history_line_bounded(&mut reader, &mut buf, &mut scanned_bytes).await? == 0 {
            return Ok(found);
        }
        let line = trim_line(&buf);
        if has_history_id(line, id, prefix.as_bytes()) {
            found = Some(
                serde_json::from_slice(line)
                    .map_err(|error| std::io::Error::new(ErrorKind::InvalidData, error))?,
            );
        }
    }
}

/// 在文件里找 `id` 这一条，读出完整记录。先比前缀或只读 ID，命中后才解析全文。
/// 同 ID 可能存在多行，必须和内存查找一样选择最后一条。
pub(super) async fn read_history_entry(path: &Path, id: &str) -> Option<DebugHistoryEntry> {
    let prefix = format!("{{\"id\":{},", serde_json::to_string(id).ok()?);
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut found = None;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf).await.ok()? == 0 {
            return found;
        }
        let line = trim_line(&buf);
        if has_history_id(line, id, prefix.as_bytes()) {
            found = serde_json::from_slice(line).ok();
        }
    }
}

// ---------------------------------------------------------------------------
// 按行复制
// ---------------------------------------------------------------------------

/// 只复制磁盘上的最新记录，避免把已从内存淘汰的回包一起丢掉。
pub(super) async fn compact_history(path: &Path) -> Result<usize, String> {
    let total = count_lines(path).await?;
    let skip = total.saturating_sub(HISTORY_KEEP);
    rewrite_lines(path, path, |ordinal| ordinal >= skip).await
}

async fn count_lines(path: &Path) -> Result<usize, String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("打开 {} 失败: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut count = 0usize;
    loop {
        buf.clear();
        let read = reader
            .read_until(b'\n', &mut buf)
            .await
            .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
        if read == 0 {
            return Ok(count);
        }
        if !is_blank(trim_line(&buf)) {
            count += 1;
        }
    }
}

/// 把 `src` 里「非空行序号」满足 `keep` 的行按原样复制进临时文件，再原子换成 `dst`
/// （`src` 和 `dst` 可以是同一个文件）。返回复制了几行。
/// 逐行读写，内存里只有一行，被内存预算丢掉的回包不会因此丢失
async fn rewrite_lines(
    src: &Path,
    dst: &Path,
    keep: impl Fn(usize) -> bool,
) -> Result<usize, String> {
    let input = tokio::fs::File::open(src)
        .await
        .map_err(|e| format!("打开 {} 失败: {e}", src.display()))?;
    let (tmp, file) = create_tmp(dst).await?;
    let mut writer = BufWriter::new(file);
    match copy_lines(BufReader::new(input), &mut writer, &keep).await {
        Ok(copied) => {
            commit_tmp(tmp, writer.into_inner(), dst).await?;
            Ok(copied)
        }
        Err(e) => {
            discard_tmp(&tmp, writer.into_inner()).await;
            Err(format!("复制 {} 的内容失败: {e}", src.display()))
        }
    }
}

async fn copy_lines<R, W>(
    mut reader: R,
    writer: &mut W,
    keep: &impl Fn(usize) -> bool,
) -> std::io::Result<usize>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = Vec::new();
    let mut ordinal = 0usize;
    let mut copied = 0usize;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf).await? == 0 {
            break;
        }
        let line = trim_line(&buf);
        if is_blank(line) {
            continue;
        }
        if keep(ordinal) {
            writer.write_all(line).await?;
            writer.write_all(b"\n").await?;
            copied += 1;
        }
        ordinal += 1;
    }
    writer.flush().await?;
    Ok(copied)
}

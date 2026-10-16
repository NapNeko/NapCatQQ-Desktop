use std::path::Path;
use tokio::io::AsyncWriteExt;

pub(super) async fn set_source(
    root: &Path,
    id: &str,
    backend: ncd_domain::BackendType,
) -> Result<(), String> {
    super::config::write_json(
        &root.join("session-logs").join(format!("{id}.source.json")),
        &backend,
    )
    .await
}

pub(super) async fn tail_bot(
    root: &Path,
    id: &str,
    backend: Option<ncd_domain::BackendType>,
    limit: usize,
) -> Option<ncd_domain::LogSnapshot> {
    let source = match backend {
        Some(source) => Some(source),
        None => tokio::fs::read(root.join("session-logs").join(format!("{id}.source.json")))
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ncd_domain::BackendType>(&bytes).ok()),
    };
    let name = if source == Some(ncd_domain::BackendType::SnowLuma) {
        "snowluma-daemon"
    } else {
        id
    };
    tail(root, name, limit).await
}

pub(super) async fn append(root: &Path, name: &str, line: &str) -> Result<(), String> {
    let directory = root.join("session-logs");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    let path = directory.join(format!("{name}.log"));
    if tokio::fs::metadata(&path)
        .await
        .is_ok_and(|meta| meta.len() >= 5 * 1024 * 1024)
    {
        let previous = path.with_extension("previous.log");
        match tokio::fs::remove_file(&previous).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        tokio::fs::rename(&path, previous)
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|error| error.to_string())?;
    file.write_all(format!("{line}\n").as_bytes())
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn tail(root: &Path, id: &str, limit: usize) -> Option<ncd_domain::LogSnapshot> {
    let bytes = tokio::fs::read(root.join("session-logs").join(format!("{id}.log")))
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<_> = text.lines().collect();
    Some(ncd_domain::LogSnapshot {
        total_lines: all.len(),
        lines: all
            .iter()
            .skip(all.len().saturating_sub(limit))
            .map(|line| (*line).to_string())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn log_tail_survives_a_new_reader_and_preserves_unicode() {
        let root = tempfile::tempdir().unwrap();
        append(root.path(), "123", "第一行").await.unwrap();
        append(root.path(), "123", "second").await.unwrap();
        let snapshot = tail(root.path(), "123", 1).await.unwrap();
        assert_eq!(snapshot.total_lines, 2);
        assert_eq!(snapshot.lines, vec!["second"]);
    }

    #[tokio::test]
    async fn snowluma_history_survives_worker_records_being_lost() {
        let root = tempfile::tempdir().unwrap();
        set_source(root.path(), "123", ncd_domain::BackendType::SnowLuma)
            .await
            .unwrap();
        append(root.path(), "snowluma-daemon", "上一会话的 daemon 日志")
            .await
            .unwrap();
        let snapshot = tail_bot(root.path(), "123", None, 20).await.unwrap();
        assert_eq!(snapshot.lines, vec!["上一会话的 daemon 日志"]);
    }

    #[tokio::test]
    async fn napcat_keeps_its_own_log_when_a_snowluma_daemon_log_exists() {
        let root = tempfile::tempdir().unwrap();
        set_source(root.path(), "123", ncd_domain::BackendType::NapCat)
            .await
            .unwrap();
        append(root.path(), "123", "napcat").await.unwrap();
        append(root.path(), "snowluma-daemon", "other protocol")
            .await
            .unwrap();
        let snapshot = tail_bot(root.path(), "123", None, 20).await.unwrap();
        assert_eq!(snapshot.lines, vec!["napcat"]);
    }
}

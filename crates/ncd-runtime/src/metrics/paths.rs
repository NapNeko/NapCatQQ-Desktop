use std::path::{Path, PathBuf};

pub fn metrics_root(data_root: &Path) -> PathBuf {
    data_root.join("metrics")
}

pub fn metrics_bot_dir(data_root: &Path, bot_id: &str) -> PathBuf {
    metrics_root(data_root).join(sanitize_bot_id(bot_id))
}

pub fn stats_path_for_bot(data_root: &Path, bot_id: &str) -> PathBuf {
    metrics_bot_dir(data_root, bot_id).join("net-stats.json")
}

pub fn history_path_for_bot(data_root: &Path, bot_id: &str) -> PathBuf {
    metrics_bot_dir(data_root, bot_id).join("history.jsonl")
}

pub fn nodes_map_path_for_bot(data_root: &Path, bot_id: &str) -> PathBuf {
    metrics_bot_dir(data_root, bot_id).join("nodes.json")
}

/// Desktop 自有探针脚本落盘位置（不进 NC/SL 组件树）
pub fn probe_script_path(data_root: &Path) -> PathBuf {
    metrics_root(data_root).join("ncd-ob11-stats.cjs")
}

fn sanitize_bot_id(bot_id: &str) -> String {
    bot_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

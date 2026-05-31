//! 配置导入导出命令。
//!
//! 导出：把当前数据根下的非敏感配置（应用配置 / Bot 配置 / 远端档案）复制到用户
//! 选定目录的一个带时间戳子目录里。明确不含任何密钥——SSH 密码 / GitHub PAT 都在
//! SecretStore（keyring / 加密文件），加密 key 与本机绑定，跨机器复制无意义且有泄露
//! 风险，所以一律不导出。
//!
//! 导入：从用户选定目录读取这几个文件，先校验 JSON 合法，再原子写回对应位置。导入
//! 后需重启让 BotManager / ServerManager 重新加载。密钥不随包导入，提示用户重配。
//!
//! 文件选择对话框由前端 tauri-plugin-dialog 负责，command 只接收已选好的绝对路径。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::AppState;

/// 参与导入导出的配置文件，相对各自 base 的路径 + 用途描述。
/// (相对 data_root 的路径, 人类可读名)。
const TRANSFER_FILES: &[(&str, &str)] = &[
    ("runtime/config/config.json", "应用配置"),
    ("runtime/config/bot.json", "Bot 配置"),
    ("config/servers.json", "远端服务器档案"),
];

/// 导出结果：落地目录 + 实际导出的文件名清单。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct ConfigExportResult {
    /// 实际写入的导出子目录绝对路径。
    pub export_dir: String,
    /// 成功复制的文件人类可读名（按 TRANSFER_FILES 顺序，缺失的不计入）。
    pub files: Vec<String>,
}

/// 导入结果：来源目录 + 实际导入的文件名清单。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct ConfigImportResult {
    /// 成功导入并写回的文件人类可读名。
    pub files: Vec<String>,
    /// 在来源目录里没找到的文件名（提示用户这些项未覆盖）。
    pub skipped: Vec<String>,
}

/// 导出当前配置到 `dest_dir` 下的带时间戳子目录。
///
/// 子目录名 `napcat-config-<unix_ts>`，避免覆盖用户已有内容。逐个文件复制，源文件
/// 不存在就跳过（不算失败）。返回实际导出的文件清单。
#[tauri::command]
pub async fn export_config(
    state: State<'_, AppState>,
    dest_dir: String,
) -> Result<ConfigExportResult, String> {
    let dest_base = PathBuf::from(&dest_dir);
    if !dest_base.is_dir() {
        return Err(format!("导出目标不是有效目录: {dest_dir}"));
    }

    let stamp = unix_ts();
    let export_dir = dest_base.join(format!("napcat-config-{stamp}"));
    fs::create_dir_all(&export_dir).map_err(|e| format!("创建导出目录失败: {e}"))?;

    let data_root = &state.data_root;
    let mut files = Vec::new();
    for (rel, label) in TRANSFER_FILES {
        let src = data_root.join(rel);
        if !src.is_file() {
            continue;
        }
        // 导出目录扁平化：只取文件名，避免在用户选的目录里重建 runtime/config 层级。
        let file_name = Path::new(rel)
            .file_name()
            .ok_or_else(|| format!("非法配置路径: {rel}"))?;
        let dst = export_dir.join(file_name);
        fs::copy(&src, &dst).map_err(|e| format!("复制 {label} 失败: {e}"))?;
        files.push((*label).to_string());
    }

    Ok(ConfigExportResult {
        export_dir: export_dir.to_string_lossy().to_string(),
        files,
    })
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 从 `src_dir` 导入配置，原子写回当前数据根对应位置。
///
/// 来源目录里的文件按文件名匹配（config.json / bot.json / servers.json）。每个文件
/// 先 parse 成 JSON 校验合法（拒绝把损坏文件写进配置区），再走 ConfigStore 原子写入
/// （自动备份现有文件，失败回滚）。来源缺失的文件计入 skipped，不算失败。
///
/// 不触碰 secrets：导入包本就不含密钥，导入后 SSH 密码 / GitHub PAT 维持现状，
/// 前端提示用户按需重配。
#[tauri::command]
pub async fn import_config(
    state: State<'_, AppState>,
    src_dir: String,
) -> Result<ConfigImportResult, String> {
    use ncd_runtime::{ConfigStore, LocalConfigStore};

    let src_base = PathBuf::from(&src_dir);
    if !src_base.is_dir() {
        return Err(format!("导入来源不是有效目录: {src_dir}"));
    }

    let store = LocalConfigStore::new(&state.data_root);
    let mut files = Vec::new();
    let mut skipped = Vec::new();

    for (rel, label) in TRANSFER_FILES {
        let file_name = Path::new(rel)
            .file_name()
            .ok_or_else(|| format!("非法配置路径: {rel}"))?;
        let src = src_base.join(file_name);
        if !src.is_file() {
            skipped.push((*label).to_string());
            continue;
        }

        // 先 parse 校验，损坏的 JSON 直接拒绝，不污染配置区。
        let text =
            fs::read_to_string(&src).map_err(|e| format!("读取 {label} 失败: {e}"))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("{label} 不是合法 JSON，已中止导入: {e}"))?;

        // 写回目标：相对 data_root 的原路径。走 ConfigStore 原子写 + 自动备份。
        let dst = state.data_root.join(rel);
        store
            .write_json_atomic(&dst, &value)
            .map_err(|e| format!("写入 {label} 失败: {e}"))?;
        files.push((*label).to_string());
    }

    if files.is_empty() {
        return Err("来源目录里没有可识别的配置文件（config.json / bot.json / servers.json）".to_string());
    }

    Ok(ConfigImportResult { files, skipped })
}

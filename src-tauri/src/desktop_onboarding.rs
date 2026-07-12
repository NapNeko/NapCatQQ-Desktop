//! Desktop 新手引导状态：落盘在 data_root/config/desktop-onboarding.json。
//!
//! 与 desktop_consent 独立：协议门禁通过后，前端再问「了解 / 跳过」。
//! 是否弹出只认本文件 status，不探测 bot.json（升级用户也可能想看新版说明）。
//! 设置页「重新查看入门」只更新 lastOpenedAt，不把 status 打回 pending。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const ONBOARDING_FILE: &str = "desktop-onboarding.json";
/// 引导文案/步骤大改时 bump；与磁盘 version 不一致时仍尊重已决策 status，不强制重问。
pub const ONBOARDING_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum DesktopOnboardingError {
    #[error("create {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("rename {from} to {to}: {source}")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    #[error("remove {path}: {source}")]
    Remove {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("serialize onboarding record: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("invalid onboarding status: {0}")]
    InvalidStatus(String),
}

impl DesktopOnboardingError {
    pub fn to_command_string(&self) -> String {
        self.to_string()
    }
}

/// 引导决策状态（serde 用 camelCase 字符串，与前端一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub enum OnboardingStatus {
    /// 从未选择：consent 后应弹出选择页。
    #[default]
    Pending,
    /// 用户点了「跳过，直接使用」。
    Skipped,
    /// 用户选了「了解一下」，内容向导进行中（关闭后通常会写成 completed）。
    Active,
    /// 已看过内容向导或从内容页关闭。
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct DesktopOnboardingState {
    /// 本模块 schema，不是协议 content-hash。
    pub version: u32,
    pub status: OnboardingStatus,
    /// 首次做出了解/跳过决策的时间（ISO）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
    /// 最近一次打开引导 UI（含设置重开）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<String>,
    /// 内容步骤 id（预留；P0 前端可不写）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_step_ids: Vec<String>,
}

impl Default for DesktopOnboardingState {
    fn default() -> Self {
        Self {
            version: ONBOARDING_SCHEMA_VERSION,
            status: OnboardingStatus::Pending,
            decided_at: None,
            last_opened_at: None,
            completed_step_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
#[serde(rename_all = "camelCase")]
pub struct DesktopOnboardingPayload {
    pub state: DesktopOnboardingState,
    /// 是否应在启动时弹出「了解 / 跳过」选择页。
    pub should_prompt_choice: bool,
    /// 当前 schema；前端可用于检测文案版本。
    pub schema_version: u32,
}

/// 读取状态；缺文件或无法解析视为 pending。
/// 是否弹出选择页只看 status==pending，不读 bot 列表。
pub fn load_payload(data_root: &Path) -> DesktopOnboardingPayload {
    let mut state = read_state(data_root).unwrap_or_default();
    if state.version == 0 {
        state.version = ONBOARDING_SCHEMA_VERSION;
    }
    payload_from(state)
}

/// 用户选择「了解一下」：status=active，并刷新时间戳。
pub fn mark_started(data_root: &Path) -> Result<DesktopOnboardingPayload, DesktopOnboardingError> {
    let mut state = read_state(data_root).unwrap_or_default();
    let now = now_iso();
    if state.decided_at.is_none() {
        state.decided_at = Some(now.clone());
    }
    state.last_opened_at = Some(now);
    state.status = OnboardingStatus::Active;
    state.version = ONBOARDING_SCHEMA_VERSION;
    write_state(data_root, &state)?;
    Ok(payload_from(state))
}

/// 用户选择「跳过」。
pub fn mark_skipped(data_root: &Path) -> Result<DesktopOnboardingPayload, DesktopOnboardingError> {
    let mut state = read_state(data_root).unwrap_or_default();
    let now = now_iso();
    if state.decided_at.is_none() {
        state.decided_at = Some(now.clone());
    }
    state.last_opened_at = Some(now);
    state.status = OnboardingStatus::Skipped;
    state.version = ONBOARDING_SCHEMA_VERSION;
    write_state(data_root, &state)?;
    Ok(payload_from(state))
}

/// 内容向导走完或用户从内容页关闭（非设置只读浏览的「完成」语义由前端决定是否调用）。
pub fn mark_completed(
    data_root: &Path,
    completed_step_ids: Option<Vec<String>>,
) -> Result<DesktopOnboardingPayload, DesktopOnboardingError> {
    let mut state = read_state(data_root).unwrap_or_default();
    let now = now_iso();
    if state.decided_at.is_none() {
        state.decided_at = Some(now.clone());
    }
    state.last_opened_at = Some(now);
    state.status = OnboardingStatus::Completed;
    if let Some(ids) = completed_step_ids {
        state.completed_step_ids = ids;
    }
    state.version = ONBOARDING_SCHEMA_VERSION;
    write_state(data_root, &state)?;
    Ok(payload_from(state))
}

/// 设置页重开：只碰 lastOpenedAt，不改 status（skipped/completed 保持）。
pub fn mark_reopened(data_root: &Path) -> Result<DesktopOnboardingPayload, DesktopOnboardingError> {
    let mut state = read_state(data_root).unwrap_or_default();
    state.last_opened_at = Some(now_iso());
    state.version = ONBOARDING_SCHEMA_VERSION;
    // 若仍是 pending（异常路径），重开视为开始了解
    if state.status == OnboardingStatus::Pending {
        state.status = OnboardingStatus::Active;
        if state.decided_at.is_none() {
            state.decided_at = state.last_opened_at.clone();
        }
    }
    write_state(data_root, &state)?;
    Ok(payload_from(state))
}

fn payload_from(state: DesktopOnboardingState) -> DesktopOnboardingPayload {
    let should_prompt_choice = state.status == OnboardingStatus::Pending;
    DesktopOnboardingPayload {
        state,
        should_prompt_choice,
        schema_version: ONBOARDING_SCHEMA_VERSION,
    }
}

fn read_state(data_root: &Path) -> Option<DesktopOnboardingState> {
    let path = data_root.join("config").join(ONBOARDING_FILE);
    let text = fs::read_to_string(path).ok()?;
    let mut state: DesktopOnboardingState = serde_json::from_str(&text).ok()?;
    // 容错：未知/空 status 当 pending
    if state.version == 0 {
        state.version = ONBOARDING_SCHEMA_VERSION;
    }
    Some(state)
}

fn write_state(
    data_root: &Path,
    state: &DesktopOnboardingState,
) -> Result<(), DesktopOnboardingError> {
    let config_dir = data_root.join("config");
    fs::create_dir_all(&config_dir).map_err(|source| DesktopOnboardingError::CreateDir {
        path: config_dir.clone(),
        source,
    })?;

    let path = config_dir.join(ONBOARDING_FILE);
    let tmp = config_dir.join(format!("{ONBOARDING_FILE}.tmp"));
    let bytes = serde_json::to_vec_pretty(state)?;
    fs::write(&tmp, bytes).map_err(|source| DesktopOnboardingError::Write {
        path: tmp.clone(),
        source,
    })?;
    replace_file(&tmp, &path)
}

fn replace_file(from: &Path, to: &Path) -> Result<(), DesktopOnboardingError> {
    if to.exists() {
        fs::remove_file(to).map_err(|source| DesktopOnboardingError::Remove {
            path: to.to_path_buf(),
            source,
        })?;
    }
    fs::rename(from, to).map_err(|source| DesktopOnboardingError::Rename {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        source,
    })
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_prompts_choice() {
        let dir = tempdir().unwrap();
        let payload = load_payload(dir.path());
        assert!(payload.should_prompt_choice);
        assert_eq!(payload.state.status, OnboardingStatus::Pending);
        assert_eq!(payload.schema_version, ONBOARDING_SCHEMA_VERSION);
    }

    #[test]
    fn skip_clears_prompt() {
        let dir = tempdir().unwrap();
        let after = mark_skipped(dir.path()).unwrap();
        assert!(!after.should_prompt_choice);
        assert_eq!(after.state.status, OnboardingStatus::Skipped);
        assert!(after.state.decided_at.is_some());

        let again = load_payload(dir.path());
        assert!(!again.should_prompt_choice);
        assert_eq!(again.state.status, OnboardingStatus::Skipped);
    }

    #[test]
    fn start_and_complete_round_trip() {
        let dir = tempdir().unwrap();
        let started = mark_started(dir.path()).unwrap();
        assert_eq!(started.state.status, OnboardingStatus::Active);
        assert!(!started.should_prompt_choice);

        let done = mark_completed(dir.path(), Some(vec!["intro".into(), "path".into()])).unwrap();
        assert_eq!(done.state.status, OnboardingStatus::Completed);
        assert_eq!(done.state.completed_step_ids.len(), 2);
    }

    #[test]
    fn reopen_keeps_skipped_status() {
        let dir = tempdir().unwrap();
        mark_skipped(dir.path()).unwrap();
        let reopened = mark_reopened(dir.path()).unwrap();
        assert_eq!(reopened.state.status, OnboardingStatus::Skipped);
        assert!(reopened.state.last_opened_at.is_some());
        assert!(!reopened.should_prompt_choice);
    }

    #[test]
    fn bots_present_still_prompts_when_pending() {
        // 升级用户可能已有 Bot，仍应只靠 onboarding 文件决定是否询问
        let dir = tempdir().unwrap();
        let config = dir.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("bot.json"),
            r#"{"info":{"configVersion":1},"bots":[{"bot":{"QQID":10001}}]}"#,
        )
        .unwrap();

        let payload = load_payload(dir.path());
        assert!(payload.should_prompt_choice);
        assert_eq!(payload.state.status, OnboardingStatus::Pending);
        assert!(read_state(dir.path()).is_none());
    }

    #[test]
    fn decided_file_not_overwritten_by_bots() {
        let dir = tempdir().unwrap();
        mark_skipped(dir.path()).unwrap();
        let config = dir.path().join("config");
        fs::write(config.join("bot.json"), r#"{"bots":[{"bot":{"QQID":1}}]}"#).unwrap();
        let payload = load_payload(dir.path());
        assert!(!payload.should_prompt_choice);
        assert_eq!(payload.state.status, OnboardingStatus::Skipped);
    }
}

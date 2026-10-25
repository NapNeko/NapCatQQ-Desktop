//! Qwen 0.26.1 的 system settings 工具上限。

use std::path::Path;

use serde_json::json;

use crate::assistant::acp::AcpLaunch;

use super::{PERSONA, path_string, write_file};

const BUILTINS: &[&str] = &[
    "exec",
    "edit",
    "write_file",
    "read_file",
    "zoom_image",
    "grep_search",
    "glob",
    "run_shell_command",
    "todo_write",
    "save_memory",
    "manage_memory",
    "search_memory",
    "agent",
    "skill",
    "exit_plan_mode",
    "enter_plan_mode",
    "web_fetch",
    "web_search",
    "image_gen",
    "list_directory",
    "lsp",
    "ask_user_question",
    "cron_create",
    "cron_list",
    "cron_delete",
    "loop_wakeup",
    "create_sub_session",
    "list_agents",
    "task_stop",
    "task_create",
    "task_update",
    "task_list",
    "team_create",
    "team_delete",
    "team_plan_approval",
    "request_shutdown",
    "send_message",
    "structured_output",
    "monitor",
    "notebook_edit",
    "read_mcp_resource",
    "advisor",
    "enter_worktree",
    "exit_worktree",
    "workflow",
    "artifact",
    "record_artifact",
    "record_source",
    "report_findings",
    "get_goal",
    "update_goal",
    "omni_downsample_image",
    "omni_downscale_video",
    "omni_downsample_audio",
    "omni_extract_keyframes",
    "omni_extract_audio",
    "omni_clip_video",
    "omni_convert_image",
    "omni_transcribe_audio",
    "omni_clip_image",
    "omni_clip_audio",
    "omni_caption_image",
    "omni_caption_audio",
    "omni_ocr_image",
    "omni_understand_video_segments",
    "omni_recall_media_memory",
    "propose_goal",
    "display_image",
];

pub(super) async fn configure(
    root: &Path,
    native_tools: bool,
    launch: &mut AcpLaunch,
) -> Result<(), String> {
    let settings = if native_tools {
        json!({"disableAllHooks": true, "telemetry": {"enabled": false}, "mcp": {"allowed": ["ncd"]}})
    } else {
        // core: [] 意味着全部可用；保留一个已禁用名称才能同时收紧未来新增 core tool。
        json!({
            "disableAllHooks": true,
            "telemetry": {"enabled": false},
            "tools": {
                "core": ["read_file"], "disabled": BUILTINS,
                "discoveryCommand": "", "callCommand": "", "codeModeOnly": false, "freeform": false
            },
            "permissions": {"deny": BUILTINS},
            "mcp": {"allowed": ["ncd"], "serverCommand": ""},
            "experimental": {"mods": false}
        })
    };
    let bytes =
        serde_json::to_vec_pretty(&settings).map_err(|_| "无法生成 Qwen 专用设置".to_string())?;
    let settings_path = write_file(root, "qwen-system-settings.json", &bytes).await?;
    launch.args.extend([
        "--acp".into(),
        "--extensions".into(),
        "none".into(),
        "--approval-mode".into(),
        "default".into(),
        "--system-prompt".into(),
        PERSONA.into(),
        "--allowed-mcp-server-names".into(),
        "ncd".into(),
    ]);
    launch.env.insert(
        "QWEN_CODE_SYSTEM_SETTINGS_PATH".into(),
        path_string(&settings_path),
    );
    launch
        .env
        .insert("QWEN_CODE_DISABLE_CRON".into(), "1".into());
    Ok(())
}

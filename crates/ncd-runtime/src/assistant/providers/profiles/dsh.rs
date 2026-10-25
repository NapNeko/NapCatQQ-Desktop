//! DSH 0.2.0-rc.2 的运行时工具拒绝器与 profile overlay。

use std::path::Path;

use serde_json::json;

use crate::assistant::acp::AcpLaunch;

use super::{PERSONA, path_string, write_file};

const DISABLED_ROWS: &[&str] = &[
    "tool-plugin-manager",
    "tool-bash",
    "tool-pwsh",
    "tool-jobs",
    "tool-fs",
    "tool-fs-search",
    "tool-skill",
    "tool-subagent-control",
    "tool-subagent-list-agents",
    "tool-subagent",
    "tool-subagent-fork",
    "tool-workflow",
    "tool-todo",
    "tool-goal",
    "tool-web",
    "tool-ralph",
    "agent-instructions",
    "skill-filesystem",
    "workflow-ptc",
    "plugin-manager",
    "config-editor",
];

pub(super) async fn configure(
    root: &Path,
    native_tools: bool,
    launch: &mut AcpLaunch,
) -> Result<(), String> {
    let mode = if native_tools {
        "workspace-write"
    } else {
        "read-only"
    };
    let mut patch = vec![
        json!({"id":"approval", "config":{"policy":"ask"}}),
        json!({"id":"sandbox-policy", "config":{"mode":mode, "workspaceRoot":path_string(&launch.cwd)}}),
        json!({"id":"tools", "config":{"mode":"native"}}),
        json!({"id":"system-prompt", "config":{"personaPrefix":PERSONA, "personaSuffix":""}}),
        json!({"id":"session-log-deepseek", "disabled":true}),
        json!({"id":"session-telemetry-otel", "disabled":true}),
        json!({"id":"hmr", "disabled":true}),
    ];
    if !native_tools {
        for row in DISABLED_ROWS {
            patch.push(json!({"id":row, "disabled":true}));
        }
        // guard 在所有 pre-execute 扩展之后执行，补住未枚举的原生工具。
        let guard = b"export const name = 'ncd-assistant-guard';\nexport const inject = ['tools'];\nexport function apply(ctx) {\n  if (typeof ctx.tools.guard !== 'function') throw new Error('NCD requires DSH tool guards');\n  ctx.tools.guard(call => call.name.startsWith('mcp__ncd__') ? undefined : 'NCD only permits the attached NCD MCP tools');\n}\n";
        let guard_path = write_file(root, "dsh-guard.mjs", guard).await?;
        let url = reqwest::Url::from_file_path(&guard_path)
            .map_err(|_| "无法生成 DSH 工具守卫路径".to_string())?;
        patch.push(json!({"insert":[{"id":"ncd-assistant-guard", "name":url.as_str(), "inject":["tools"]}]}));
    }
    // JSON 也是合法 YAML，保留上游扩展名并避免手工拼接配置标量。
    let patch =
        serde_json::to_vec_pretty(&patch).map_err(|_| "无法生成 DSH 工具限制配置".to_string())?;
    let patch_path = write_file(root, "dsh.patch.yml", &patch).await?;
    launch.args.extend([
        "--profile".into(),
        "acp".into(),
        "--patch".into(),
        path_string(&patch_path),
    ]);
    launch.env.insert("DSH_PERMISSION_MODE".into(), mode.into());
    launch
        .env
        .insert("DSH_TELEMETRY_DISABLED".into(), "1".into());
    Ok(())
}

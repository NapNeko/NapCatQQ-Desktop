use super::*;

fn launch(cwd: PathBuf) -> AcpLaunch {
    AcpLaunch {
        executable: cwd.join("fixture-agent.exe"),
        args: Vec::new(),
        env: Default::default(),
        cwd,
    }
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().expect("fixture directory");
    let workspace = temp.path().join("session workspace");
    let global = temp.path().join("existing-agent-home");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::create_dir_all(&global).expect("global home fixture");
    std::fs::write(global.join("settings.json"), b"fixture global settings")
        .expect("global settings");
    std::fs::write(global.join("auth-fixture"), b"fixture auth marker").expect("auth fixture");
    let workspace = std::fs::canonicalize(workspace).expect("canonical workspace");
    (temp, workspace, global)
}

async fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&tokio::fs::read(path).await.expect("generated config"))
        .expect("structured config")
}

fn row<'a>(patch: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    patch
        .as_array()
        .expect("patch list")
        .iter()
        .find(|row| row["id"] == id)
        .expect("patch row")
}

#[tokio::test]
async fn dsh_overlay_has_execution_guard_and_does_not_replace_auth_profile() {
    let (_temp, workspace, global) = fixture();
    let mut launch = launch(workspace.clone());
    configure(AssistantProvider::Dsh, false, &mut launch)
        .await
        .expect("dsh profile");
    let patch = read_json(&workspace.join(".ncd-agent/dsh.patch.yml")).await;
    assert_eq!(row(&patch, "approval")["config"]["policy"], "ask");
    assert_eq!(row(&patch, "sandbox-policy")["config"]["mode"], "read-only");
    assert_eq!(
        row(&patch, "sandbox-policy")["config"]["workspaceRoot"],
        path_string(&workspace)
    );
    assert_eq!(row(&patch, "tools")["config"]["mode"], "native");
    for id in [
        "tool-bash",
        "tool-pwsh",
        "tool-fs",
        "tool-subagent",
        "tool-plugin-manager",
        "session-log-deepseek",
    ] {
        assert_eq!(row(&patch, id)["disabled"], true);
    }
    let inserted = patch
        .as_array()
        .expect("patch list")
        .iter()
        .find_map(|row| row.get("insert"))
        .expect("guard plugin");
    let plugin = inserted
        .as_array()
        .expect("insert list")
        .first()
        .expect("guard");
    assert_eq!(plugin["inject"], serde_json::json!(["tools"]));
    let guard_path = reqwest::Url::parse(plugin["name"].as_str().expect("plugin URL"))
        .expect("file URL")
        .to_file_path()
        .expect("local plugin");
    assert!(
        tokio::fs::canonicalize(&guard_path)
            .await
            .expect("canonical guard")
            .starts_with(&workspace)
    );
    assert!(
        !tokio::fs::read(guard_path)
            .await
            .expect("guard source")
            .is_empty()
    );
    assert!(!launch.env.contains_key("DSH_HOME"));
    assert_eq!(
        tokio::fs::read(global.join("settings.json"))
            .await
            .expect("original settings"),
        b"fixture global settings"
    );
    assert_eq!(
        tokio::fs::read(global.join("auth-fixture"))
            .await
            .expect("original auth"),
        b"fixture auth marker"
    );
}

#[tokio::test]
async fn qwen_system_settings_close_dynamic_execution_and_ambient_mcp_sources() {
    let (_temp, workspace, _global) = fixture();
    let mut launch = launch(workspace.clone());
    configure(AssistantProvider::Qwen, false, &mut launch)
        .await
        .expect("qwen profile");
    let path = Path::new(
        launch
            .env
            .get("QWEN_CODE_SYSTEM_SETTINGS_PATH")
            .expect("system path"),
    );
    assert!(path.starts_with(&workspace));
    let settings = read_json(path).await;
    let disabled = settings["tools"]["disabled"]
        .as_array()
        .expect("disabled tools");
    for name in [
        "read_file",
        "run_shell_command",
        "exec",
        "agent",
        "skill",
        "monitor",
        "web_fetch",
    ] {
        assert!(disabled.iter().any(|value| value == name));
    }
    assert!(
        settings["tools"]["core"]
            .as_array()
            .expect("nonempty core ceiling")
            .iter()
            .all(|name| disabled.contains(name))
    );
    assert_eq!(settings["tools"]["discoveryCommand"], "");
    assert_eq!(settings["tools"]["callCommand"], "");
    assert_eq!(settings["mcp"]["serverCommand"], "");
    assert_eq!(settings["mcp"]["allowed"], serde_json::json!(["ncd"]));
    assert_eq!(settings["disableAllHooks"], true);
    assert!(
        launch
            .args
            .windows(2)
            .any(|pair| pair == ["--extensions", "none"])
    );
    assert!(
        launch
            .args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "default"])
    );
    assert!(!launch.env.contains_key("QWEN_CODE_HOME"));
}

#[tokio::test]
async fn kimi_project_override_limits_tool_namespace_without_moving_user_login() {
    let (_temp, workspace, _global) = fixture();
    let mut launch = launch(workspace.clone());
    configure(AssistantProvider::Kimi, false, &mut launch)
        .await
        .expect("kimi profile");
    let agent = tokio::fs::read_to_string(workspace.join(".kimi-code/agents/agent.md"))
        .await
        .expect("project agent");
    let header = agent
        .strip_prefix("---\n")
        .expect("frontmatter")
        .split_once("\n---\n")
        .expect("frontmatter end")
        .0;
    assert!(header.lines().any(|line| line == "override: true"));
    let tools = header
        .split_once("tools:\n")
        .expect("tool allowlist")
        .1
        .split_once("subagents:")
        .expect("subagent ceiling")
        .0;
    assert_eq!(
        tools
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>(),
        vec!["- 'mcp__ncd__*'"]
    );
    assert!(!launch.env.contains_key("KIMI_CODE_HOME"));
    assert_eq!(launch.args, vec!["acp"]);
}

#[tokio::test]
async fn minimax_uses_private_data_dir_and_preserves_existing_profile_files() {
    let (_temp, workspace, global) = fixture();
    let mut prepared = launch(workspace.clone());
    configure(AssistantProvider::MiniMax, false, &mut prepared)
        .await
        .expect("minimax profile");
    let home = Path::new(
        prepared
            .env
            .get("MINIMAX_DATA_DIR")
            .expect("private data-dir"),
    );
    assert!(home.starts_with(&workspace) && home != global);
    let settings = read_json(&home.join("config.yaml")).await;
    for key in ["tools", "builtinTools", "skills"] {
        assert_eq!(settings["agents"]["default"][key], serde_json::json!([]));
    }
    for key in ["mavis", "delegation", "webSearch"] {
        assert_eq!(settings["agents"]["default"]["features"][key], false);
    }
    assert_eq!(settings["permissionMode"], "default");
    assert_eq!(
        tokio::fs::read(global.join("auth-fixture"))
            .await
            .expect("existing auth"),
        b"fixture auth marker"
    );
    tokio::fs::write(home.join("auth-fixture"), b"new independent login marker")
        .await
        .expect("independent auth fixture");
    let mut repeat = launch(workspace);
    configure(AssistantProvider::MiniMax, false, &mut repeat)
        .await
        .expect("repeat profile");
    assert_eq!(
        tokio::fs::read(home.join("auth-fixture"))
            .await
            .expect("preserved independent auth"),
        b"new independent login marker"
    );
}

#[tokio::test]
async fn native_tool_opt_in_keeps_minimax_user_home_and_omits_dsh_guard() {
    let (_temp, workspace, _global) = fixture();
    let mut native = launch(workspace.clone());
    configure(AssistantProvider::MiniMax, true, &mut native)
        .await
        .expect("native minimax");
    assert!(!native.env.contains_key("MINIMAX_DATA_DIR"));
    let mut dsh = launch(workspace.clone());
    configure(AssistantProvider::Dsh, true, &mut dsh)
        .await
        .expect("native dsh");
    let patch = read_json(&workspace.join(".ncd-agent/dsh.patch.yml")).await;
    assert!(
        !patch
            .as_array()
            .expect("patch list")
            .iter()
            .any(|row| row.get("insert").is_some())
    );
    assert_eq!(row(&patch, "approval")["config"]["policy"], "ask");
}

#[tokio::test]
async fn profile_generation_rejects_non_directory_and_parent_components() {
    let (_temp, workspace, _global) = fixture();
    assert!(prepare_directory(&workspace, "../outside").await.is_err());
    tokio::fs::write(workspace.join(".ncd-agent"), b"fixture file")
        .await
        .expect("invalid directory fixture");
    assert!(
        configure(AssistantProvider::Dsh, false, &mut launch(workspace))
            .await
            .is_err()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn profile_generation_cannot_follow_directory_symlinks() {
    let (_temp, workspace, global) = fixture();
    std::os::unix::fs::symlink(&global, workspace.join(".ncd-agent")).expect("directory symlink");
    assert!(
        configure(AssistantProvider::Dsh, false, &mut launch(workspace))
            .await
            .is_err()
    );
    assert!(!global.join("dsh.patch.yml").exists());
}

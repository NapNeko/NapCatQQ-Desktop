use super::*;

fn configured(provider: AssistantProvider, args: &[&str]) -> AssistantSettings {
    AssistantSettings {
        providers: vec![AssistantProviderConfig {
            provider,
            extra_args: args.iter().map(|value| (*value).to_owned()).collect(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn rejects_permission_bypasses_configuration_and_secret_arguments() {
    for provider in PROVIDERS {
        for argument in [
            "--yolo",
            "-y",
            "--dangerously-skip-permissions",
            "--permission-mode=auto",
            "--allowTools=Bash",
            "--settings=secrets.json",
            "--config=a=b",
            "--patch=x.yml",
            "--api-key=fixture-secret",
            "--token=fixture-secret",
            "fixture-secret",
            "--",
            "\n",
        ] {
            let error = validate_settings(&configured(provider, &[argument])).err();
            assert!(
                error.is_some(),
                "{provider:?} accepted a policy-changing argument"
            );
            assert!(
                !error
                    .as_deref()
                    .unwrap_or_default()
                    .contains("fixture-secret")
            );
        }
    }
}

#[test]
fn safe_model_arguments_are_literal_and_do_not_accept_command_values() {
    assert!(
        validate_settings(&configured(
            AssistantProvider::Qwen,
            &["--model", "provider/model-v1"]
        ))
        .is_ok()
    );
    assert!(
        validate_settings(&configured(
            AssistantProvider::Custom,
            &["acp", "serve", "--stdio"]
        ))
        .is_ok()
    );
    for value in [
        "--yolo",
        "$(whoami)",
        "`whoami`",
        "x;whoami",
        "x\nwhoami",
        "https://host/?token=fixture-secret",
    ] {
        assert!(
            validate_settings(&configured(AssistantProvider::Qwen, &["--model", value])).is_err()
        );
    }
}

#[test]
fn validates_absolute_paths_and_unique_provider_entries() {
    let mut settings = configured(AssistantProvider::Custom, &[]);
    settings.providers[0].executable_path = Some("relative/agent.exe".into());
    assert!(validate_settings(&settings).is_err());
    let absolute = std::env::temp_dir().join("agent fixture.exe");
    settings.providers[0].executable_path = Some(absolute.to_string_lossy().into_owned());
    assert!(validate_settings(&settings).is_ok());
    settings.providers.push(settings.providers[0].clone());
    assert!(validate_settings(&settings).is_err());
}

#[test]
fn restrictions_fail_closed_for_unknown_versions_but_allow_explicit_native_tools() {
    for (provider, version) in [
        (AssistantProvider::Dsh, "0.2.0-rc.2"),
        (AssistantProvider::Qwen, "0.26.1"),
        (AssistantProvider::Kimi, "2.1.1"),
        (AssistantProvider::MiniMax, "0.6.9"),
    ] {
        assert!(profiles::check_support(provider, Some(version), false).is_ok());
        let unknown = if provider == AssistantProvider::Kimi {
            "2.99.0"
        } else {
            "99.0.0"
        };
        assert!(profiles::check_support(provider, Some(unknown), false).is_err());
        assert!(profiles::check_support(provider, Some(unknown), true).is_ok());
    }
    for provider in [
        AssistantProvider::CodeBuddy,
        AssistantProvider::Qoder,
        AssistantProvider::Trae,
        AssistantProvider::Custom,
    ] {
        let error = profiles::check_support(provider, Some("2.0.0"), false)
            .err()
            .unwrap_or_default();
        assert!(error.contains("允许原生工具"));
        assert!(profiles::check_support(provider, Some("2.0.0"), true).is_ok());
    }
    assert!(profiles::check_support(AssistantProvider::Kimi, Some("1.42.0"), true).is_err());
}

#[test]
fn minimax_separate_authentication_is_explained_before_starting() {
    let mut settings = configured(AssistantProvider::MiniMax, &[]);
    let notice = configuration_notice(AssistantProvider::MiniMax, &settings).unwrap_or_default();
    assert!(notice.contains("独立登录") && notice.contains("不会读取或复制"));
    assert!(notice.contains("允许原生工具") && notice.contains("Agent 自身权限"));
    settings.providers[0].allow_native_tools = true;
    assert!(configuration_notice(AssistantProvider::MiniMax, &settings).is_none());
    assert!(configuration_notice(AssistantProvider::Dsh, &settings).is_none());
}

fn minimax_launch_fixture() -> (tempfile::TempDir, AcpLaunch) {
    let temp = tempfile::tempdir().expect("login fixture");
    let cwd = temp.path().join("workspace ' fixture");
    let program = temp.path().join("program ' fixture").join("mcode.exe");
    let home = cwd.join(".ncd-agent/minimax ' fixture");
    let launch = AcpLaunch {
        executable: program,
        cwd: cwd.clone(),
        args: vec![
            "acp".into(),
            "--append-system-prompt-file".into(),
            cwd.join("persona.md").to_string_lossy().into_owned(),
        ],
        env: [(
            "MINIMAX_DATA_DIR".into(),
            home.to_string_lossy().into_owned(),
        )]
        .into_iter()
        .collect(),
    };
    (temp, launch)
}

#[test]
fn login_command_uses_literal_powershell_arguments_and_fixed_oauth_options() {
    let (_temp, launch) = minimax_launch_fixture();
    let command = render_minimax_login(&launch).expect("login command");
    assert!(command.contains(
        &powershell_literal(launch.executable.to_str().expect("program path")).expect("literal")
    ));
    assert!(
        command.contains(
            &powershell_literal(launch.env.get("MINIMAX_DATA_DIR").expect("home path"))
                .expect("literal")
        )
    );
    assert!(command.contains("'acp' 'login' '--region' 'cn' '--no-browser'"));
    assert!(!command.contains("--append-system-prompt-file") && !command.contains("persona.md"));
    assert!(!command.contains("Bearer") && !command.contains("Authorization"));
    assert_eq!(
        powershell_literal("x'; $(fixture); `fixture` #").expect("quoted text"),
        "'x''; $(fixture); `fixture` #'"
    );
    assert!(command.contains("finally") && command.contains("$ncdPreviousMiniMaxHome"));
}

#[test]
fn login_command_preserves_only_the_verified_npm_script_prefix() {
    let (_temp, mut launch) = minimax_launch_fixture();
    let script = launch
        .cwd
        .join("node_modules/@minimax-ai/code/bin/mcode.js")
        .to_string_lossy()
        .into_owned();
    launch.executable = launch.cwd.join("node.exe");
    launch.args.insert(0, script.clone());
    let command = render_minimax_login(&launch).expect("npm login command");
    assert!(command.contains(&powershell_literal(&script).expect("script literal")));
    launch.args.insert(0, "--require".into());
    assert!(render_minimax_login(&launch).is_err());
}

#[test]
fn login_command_rejects_unknown_environment_and_control_characters() {
    let (_temp, mut launch) = minimax_launch_fixture();
    launch
        .env
        .insert("MCP_TOKEN".into(), "fixture-secret".into());
    let error = render_minimax_login(&launch)
        .err()
        .expect("unknown environment rejected");
    assert!(!error.contains("fixture-secret"));
    launch.env.remove("MCP_TOKEN");
    launch.env.insert(
        "MINIMAX_DATA_DIR".into(),
        format!("{}\nfixture", launch.cwd.display()),
    );
    assert!(render_minimax_login(&launch).is_err());
    for text in [
        "fixture\ncommand",
        "fixture\rcommand",
        "fixture\0command",
        "fixture\tcommand",
    ] {
        assert!(powershell_literal(text).is_err());
    }
}

#[tokio::test]
async fn login_command_for_other_providers_does_not_discover_or_start_an_agent() {
    for provider in PROVIDERS
        .into_iter()
        .filter(|provider| *provider != AssistantProvider::MiniMax)
    {
        assert!(
            login_command(
                provider,
                &AssistantSettings::default(),
                Path::new("missing-relative-directory")
            )
            .await
            .expect("unsupported login")
            .is_none()
        );
    }
}

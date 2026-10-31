//! 显式运行：预先导入含 /bin/sh、sleep、cat 的镜像，设置 NCD_PODMAN_TEST_IMAGE。
//! 使用当前用户的 Podman 和 Compose；所有测试容器、卷均由本测试创建并清理。
#![cfg(target_os = "linux")]

use ncd_deploy::docker::{
    DockerCli,
    engine::{self, ContainerEngine},
};
use ncd_domain::{ContainerAction, ContainerState};
use ncd_host::{Host, local::LocalLinuxHost};
use ncd_test_support::TempWorkspace;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
#[ignore = "需要可用的 Podman / Compose 和 NCD_PODMAN_TEST_IMAGE 指定的已导入测试镜像"]
async fn podman_compose_lifecycle_and_resource_reports() -> TestResult {
    let image = std::env::var("NCD_PODMAN_TEST_IMAGE")?;
    let host = LocalLinuxHost::new();
    let mut preferences = engine::preferences();
    preferences.insert(host.id().into(), ContainerEngine::Podman);
    engine::replace_preferences(preferences);
    let cli = DockerCli::new(&host);
    cli.ensure_ready().await?;
    cli.ensure_image_compatible(&image).await?;
    let workspace = TempWorkspace::new()?;
    workspace.write_file("input/marker", b"podman-bind-mount-ok")?;
    let name = format!("ncd-podman-smoke-{}", std::process::id());
    let yaml = format!(
        "services:\n  smoke:\n    image: {image}\n    container_name: {name}\n    network_mode: none\n    entrypoint: [/bin/sh, -c, 'echo podman-smoke-ready; sleep 60']\n    volumes:\n      - ./input:/input:ro\n      - smoke-data:/data\nvolumes:\n  smoke-data:\n"
    );
    workspace.write_file(
        "docker-compose.yml",
        engine::prepare_compose(ContainerEngine::Podman, &yaml)?,
    )?;
    let project = workspace.path().to_string_lossy().into_owned();
    let result: TestResult = async {
        cli.compose_up(&project).await?;
        let rows = cli.list_containers().await?;
        let container = rows
            .iter()
            .find(|row| row.name == name)
            .ok_or("container missing after Compose up")?;
        assert_eq!(container.state, ContainerState::Running);
        assert!(cli.image_exists(&image).await?);
        assert!(!cli.list_images().await?.is_empty());
        let details = cli.container_details(&name).await?;
        assert_eq!(details.name, name);
        assert_eq!(details.state, ContainerState::Running);
        cli.list_networks().await?;
        assert!(!cli.list_volumes().await?.is_empty());
        let content = cli.exec_read_file(&name, "/input/marker").await?;
        assert_eq!(content.trim(), "podman-bind-mount-ok");
        if std::env::var_os("NCD_PODMAN_SMOKE_NO_STATS").is_none() {
            cli.container_stats(&name).await?;
        }
        cli.container_action(ContainerAction::Stop, &name, false)
            .await?;
        cli.container_action(ContainerAction::Start, &name, false)
            .await?;
        assert_eq!(
            cli.container_details(&name).await?.state,
            ContainerState::Running
        );
        Ok(())
    }
    .await;
    let cleanup = cli.compose_down(&project, true).await;
    result?;
    cleanup?;
    assert!(
        !cli.list_containers()
            .await?
            .iter()
            .any(|row| row.name == name)
    );
    Ok(())
}

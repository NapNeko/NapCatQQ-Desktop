//! Offline fixture smoke test. Only creates containers/volumes with unique test names.

use super::*;
use ncd_host::local::LocalLinuxHost;
use ncd_test_support::TempWorkspace;

struct Cleanup {
    containers: Vec<String>,
    project: String,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for container in &self.containers {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-fv", "--", container])
                .output();
        }
        let _ = std::process::Command::new("docker")
            .args(["compose", "down", "-v", "--remove-orphans"])
            .current_dir(&self.project)
            .output();
    }
}

/// Prepare an offline image whose name includes `snowluma` and which has Node:
/// Build `FROM node:24-bookworm-slim` plus `RUN mkdir -p /app/data` as
/// `ncd-snowluma-migration-fixture:local` (no network inside the build).
/// `NCD_DOCKER_MIGRATION_FIXTURE_IMAGE=ncd-snowluma-migration-fixture:local cargo test
/// -p ncd-deploy real_docker_migrates -- --ignored --test-threads=1`
#[tokio::test]
#[ignore = "requires local Docker/Compose and NCD_DOCKER_MIGRATION_FIXTURE_IMAGE"]
async fn real_docker_migrates_writable_layer_and_anonymous_volume_before_start() {
    let image = std::env::var("NCD_DOCKER_MIGRATION_FIXTURE_IMAGE").expect("fixture image");
    assert!(image.contains("snowluma"));
    for anonymous_volume in [false, true] {
        let workspace = TempWorkspace::new().unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let project = workspace
            .create_dir_all(format!("ncd-sl-test-{nonce}"))
            .unwrap();
        let project = project.to_string_lossy().into_owned();
        let legacy = format!("ncd-sl-test-legacy-{nonce}");
        let reader = format!("ncd-sl-test-reader-{nonce}");
        let _cleanup = Cleanup {
            containers: vec![legacy.clone(), reader.clone()],
            project: project.clone(),
        };
        let host = LocalLinuxHost::new();
        let cli = DockerCli::new(&host);
        cli.ensure_ready().await.unwrap();
        let compose = format!(
            "services:\n  snowluma:\n    image: {image}\n    environment:\n      NODE_OPTIONS: '--require=/must-not-run-preload.cjs'\n    volumes:\n      - data:/app/snowluma-data\nvolumes:\n  data:\n"
        );
        std::fs::write(format!("{project}/compose.yml"), compose).unwrap();
        let mut create = cli
            .command()
            .args(["create", "--name", &legacy, "--entrypoint", "node"]);
        if anonymous_volume {
            create = create.args(["--volume", "/app/data"]);
        }
        checked(&host, create.args([&image, "-e", "process.exit(0)"]))
            .await
            .unwrap();
        let source = workspace.create_dir_all("source/config").unwrap();
        std::fs::write(source.join("runtime.json"), "legacy-runtime").unwrap();
        std::fs::write(source.join("webui.json"), "legacy-webui").unwrap();
        std::fs::write(source.join("consent.json"), "legacy-consent").unwrap();
        std::fs::write(source.join("onebot_10001.json"), "legacy-account-one").unwrap();
        std::fs::write(source.join("onebot_20002.json"), "legacy-account-two").unwrap();
        std::fs::write(workspace.join("source/.hidden"), "legacy-hidden").unwrap();
        // Create /app/data without starting the protocol or relying on image VOLUME defaults.
        checked(
            &host,
            cli.command().args([
                "cp",
                &format!("{}/.", workspace.join("source").to_string_lossy()),
                &format!("{legacy}:/app/data"),
            ]),
        )
        .await
        .unwrap();
        checked(&host, cli.command().args(["start", "--attach", &legacy]))
            .await
            .unwrap();
        let container = cli
            .list_containers()
            .await
            .unwrap()
            .into_iter()
            .find(|container| container.name == legacy)
            .unwrap();

        // Seed a newer WebUI config before migration; this file must win.
        checked(&host, cli.compose_command().await.unwrap().args([
            "run", "--rm", "--no-deps", "--entrypoint", "node", "-e", "NODE_OPTIONS=", "snowluma", "-e",
            "const fs=require('fs');fs.mkdirSync('/app/snowluma-data/config',{recursive:true});fs.writeFileSync('/app/snowluma-data/config/webui.json','new-webui');",
        ]).working_dir(HostPath::from_posix(&project))).await.unwrap();
        migrate(&host, &cli, &project, &container).await.unwrap();
        migrate(&host, &cli, &project, &container).await.unwrap();
        assert!(
            std::path::Path::new(&format!(
                "{project}/snowluma-legacy/{}/config/runtime.json",
                container.id
            ))
            .exists()
        );
        assert!(
            cli.list_containers()
                .await
                .unwrap()
                .iter()
                .any(|item| item.id == container.id)
        );
        // Remove the old source and prove that the persisted destination is self-contained.
        checked(&host, cli.command().args(["rm", "-v", "--", &legacy]))
            .await
            .unwrap();
        checked(&host, cli.compose_command().await.unwrap().args([
            "run", "--rm", "--no-deps", "--name", &reader, "--entrypoint", "node", "-e", "NODE_OPTIONS=", "snowluma", "-e",
            "const fs=require('fs'),a=require('assert/strict'),p='/app/snowluma-data/';for(const [f,v] of Object.entries({'config/runtime.json':'legacy-runtime','config/webui.json':'new-webui','config/consent.json':'legacy-consent','config/onebot_10001.json':'legacy-account-one','config/onebot_20002.json':'legacy-account-two','.hidden':'legacy-hidden'}))a.equal(fs.readFileSync(p+f,'utf8'),v);",
        ]).working_dir(HostPath::from_posix(&project))).await.unwrap();
    }
}

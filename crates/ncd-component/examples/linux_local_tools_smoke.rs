//! Run the production Linux tool installers in an isolated managed directory.
//!
//! cargo run -p ncd-component --example linux_local_tools_smoke -- nodejs uv redis git
//! NCD_SMOKE_ROOT defaults to /tmp/ncd-linux-validation.

#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(target_os = "linux")]
mod support;

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ncd_component::{
        ActionCtx, Component, GitComponent, MINGIT_DEFAULT_VERSION, NoVncComponent,
        NodeJsComponent, RedisComponent, UV_DEFAULT_VERSION, UvComponent,
    };
    use ncd_host::{HostPath, local::LocalLinuxHost};

    support::configure_network()?;
    let root = std::env::var_os("NCD_SMOKE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/tmp/ncd-linux-validation".into());
    tokio::fs::create_dir_all(root.join("downloads")).await?;
    let host = LocalLinuxHost::new().with_managed_root(&root);
    let path = |name: &str| HostPath::from_posix(root.join(name).to_string_lossy().into_owned());
    let selected: Vec<String> = std::env::args().skip(1).collect();
    let node_version = std::env::var("NCD_SMOKE_NODE_VERSION").unwrap_or_else(|_| "22.13.0".into());
    let components: Vec<Box<dyn Component>> = vec![
        Box::new(
            NodeJsComponent::new(node_version, path("components/NodeJs"))
                .with_tmp_dir(path("staging")),
        ),
        Box::new(
            UvComponent::new(UV_DEFAULT_VERSION, path("components/Uv"))
                .with_tmp_dir(path("staging")),
        ),
        Box::new(RedisComponent::new(path("components/Redis")).with_tmp_dir(path("staging"))),
        Box::new(GitComponent::new(
            MINGIT_DEFAULT_VERSION,
            path("components/Git"),
        )),
        Box::new(NoVncComponent::new()),
    ];
    let mut results = Vec::new();
    for component in components {
        let id = component.id().as_str();
        if !selected.is_empty() && !selected.iter().any(|name| name == id) {
            continue;
        }
        component.check_target(&host)?;
        let (ctx, mut events) = ActionCtx::new();
        let mut ctx = ctx.with_download_dir(root.join("downloads"));
        let logger = tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                println!(
                    "[{id}] {}",
                    serde_json::to_string(&event).unwrap_or_default()
                );
            }
        });
        let installed = component.install(&host, &mut ctx).await;
        drop(ctx);
        logger.await?;
        let result = match installed {
            Ok(()) => {
                let detected = component.detect(&host).await;
                let verified = component.verify(&host).await;
                match (detected, verified) {
                    (Ok(Some(version)), Ok(report)) if report.ok => serde_json::json!({
                        "component": id, "ok": true, "version": version.version,
                        "source": version.source, "verify": report,
                    }),
                    (detected, verified) => serde_json::json!({
                        "component": id, "ok": false, "detect": format!("{detected:?}"),
                        "verify": format!("{verified:?}"),
                    }),
                }
            }
            Err(error) => {
                serde_json::json!({"component": id, "ok": false, "error": error.to_string()})
            }
        };
        println!("RESULT {result}");
        tokio::fs::write(
            root.join(format!("{id}-result.json")),
            serde_json::to_vec_pretty(&result)?,
        )
        .await?;
        results.push(result);
    }
    if results.is_empty() || results.iter().any(|result| result["ok"] != true) {
        return Err("One or more real tool installations failed; see the result files".into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This smoke installer requires Linux");
    std::process::exit(1);
}

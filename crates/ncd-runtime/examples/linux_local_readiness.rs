//! Read-only production install/start gates against real Linux installations.
//!
//! cargo run -p ncd-runtime --example linux_local_readiness -- /tmp/ncd-linux-validation napcat
//! The root uses the Desktop data layout: components/{QQ,NodeJs,Uv,...}, apps/<framework>.
//! For SnowLuma, pass its actual data root and NCD_SMOKE_SNOWLUMA_PACKAGE=lite|full.

#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    use std::sync::Arc;

    use ncd_appframework::AppFrameworkRegistry;
    use ncd_component::{ComponentId, RequirementPhase};
    use ncd_domain::SnowLumaLinuxPackage;
    use ncd_host::{HostPath, Locality, Os, local::local_host_for_data_root};
    use ncd_runtime::components::{AppComponentHint, ComponentBuildInputs, RemoteLayout};

    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("Specify an absolute Desktop data root")?);
    if !root.is_absolute() || !root.is_dir() {
        return Err("The validation root must be an existing absolute directory".into());
    }
    let selected: Vec<String> = args.collect();
    if selected.is_empty() {
        return Err("Specify napcat, snowluma, or registered application framework IDs".into());
    }
    let host = local_host_for_data_root(&root);
    assert_eq!((host.os(), host.locality()), (Os::Linux, Locality::Local));
    let registry = Arc::new(AppFrameworkRegistry::with_builtin());
    let snowluma_dir = root.join("components/SnowLuma");
    let local_snowluma_version = tokio::fs::read(PathBuf::from(&snowluma_dir).join("package.json"))
        .await
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["version"].as_str().map(str::to_string));
    let mut inputs = ComponentBuildInputs {
        data_root: root.clone(),
        remote_windows_root: None,
        remote_home: None,
        layout: RemoteLayout::Rootless,
        snapshot: None,
        local_snowluma_version,
        desktop_product_version: String::new(),
        selected: None,
        snowluma_linux_package: Some(
            if std::env::var("NCD_SMOKE_SNOWLUMA_PACKAGE").as_deref() == Ok("lite") {
                SnowLumaLinuxPackage::Lite
            } else {
                SnowLumaLinuxPackage::Full
            },
        ),
        snowluma_node_path: std::env::var("NCD_SMOKE_NODE").ok(),
        app_component: None,
        registry: Arc::clone(&registry),
    };
    let manifests = registry.manifests();
    let mut results = Vec::new();
    for name in &selected {
        let id = ComponentId::parse(name).ok_or("Unknown component ID")?;
        inputs.app_component = if id.is_app_framework() {
            let (index, manifest) = manifests
                .iter()
                .enumerate()
                .find(|(_, manifest)| manifest.component_id == *name)
                .ok_or("Unregistered application framework")?;
            Some(AppComponentHint {
                instance_id: format!("linux-smoke-{}", manifest.id.as_str()),
                install_dir: HostPath::from_posix(
                    root.join("apps")
                        .join(manifest.id.as_str())
                        .to_string_lossy(),
                ),
                port: 18700 + index as u16 * 10,
                npm_registry: None,
                pypi_index: None,
                install_version: None,
                install_renderer: false,
                adopt_existing: false,
                webui_username: None,
                webui_password: None,
            })
        } else {
            None
        };
        let install = inputs
            .resolve(id, host.as_ref(), RequirementPhase::Install)
            .await;
        let runtime = inputs.readiness(id, host.as_ref()).await;
        let result = match (install, runtime) {
            (Ok(install), Ok(runtime)) => serde_json::json!({
                "component": name,
                "host_id": host.id(),
                "install_ready": install.ready(),
                "runtime_ready": runtime.ready(),
                "install": install,
                "runtime": runtime,
            }),
            (install, runtime) => serde_json::json!({
                "component": name,
                "install_error": install.err(),
                "runtime_error": runtime.err(),
            }),
        };
        println!("RESULT {}", serde_json::to_string(&result)?);
        results.push(result);
    }
    let output = root.join(format!("readiness-results-{}.json", selected.join("-")));
    tokio::fs::write(&output, serde_json::to_vec_pretty(&results)?).await?;
    if results
        .iter()
        .any(|result| result["install_ready"] != true || result["runtime_ready"] != true)
    {
        return Err(format!("A production gate is blocked; see {}", output.display()).into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This read-only check requires Linux");
    std::process::exit(1);
}

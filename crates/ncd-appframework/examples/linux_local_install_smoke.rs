//! 手动执行真实 Linux 本机安装，不在 CI 中运行。
//!
//! cargo run -p ncd-appframework --example linux_local_install_smoke -- nonebot2 karin
//! 默认安装至 /tmp/ncd-linux-validation/apps，可用 NCD_SMOKE_ROOT 覆盖。
//! NCD_SMOKE_NODE / UV / GIT / REDIS 可指定已安装的托管运行时二进制。

#![allow(clippy::print_stdout, clippy::print_stderr)]

#[cfg(target_os = "linux")]
#[path = "../../ncd-component/examples/support/mod.rs"]
mod support;

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ncd_appframework::{AppComponentSpec, AppFrameworkRegistry};
    use ncd_component::{ActionCtx, DetectOutcome};
    use ncd_host::{Host, HostPath, Locality, Os, local::LocalLinuxHost};

    support::configure_network()?;
    let root = std::env::var_os("NCD_SMOKE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/ncd-linux-validation/apps"));
    tokio::fs::create_dir_all(&root).await?;
    let host = LocalLinuxHost::new().with_managed_root(&root);
    assert_eq!(host.os(), Os::Linux);
    assert_eq!(host.locality(), Locality::Local);
    let registry = AppFrameworkRegistry::with_builtin();
    let selected: Vec<String> = std::env::args().skip(1).collect();
    let mut results = Vec::new();
    for (index, manifest) in registry.manifests().into_iter().enumerate() {
        let id = manifest.id.as_str();
        if !selected.is_empty() && !selected.iter().any(|s| s == id) {
            continue;
        }
        let runtime_path = |name: &str| {
            std::env::var(format!("NCD_SMOKE_{name}"))
                .ok()
                .map(HostPath::from_posix)
        };
        let spec = AppComponentSpec {
            install_dir: HostPath::from_posix(root.join(id).to_string_lossy().into_owned()),
            port: 18700 + index as u16 * 10,
            node_bin: runtime_path("NODE"),
            uv_bin: runtime_path("UV"),
            git_bin: runtime_path("GIT"),
            redis_bin: runtime_path("REDIS"),
            npm_registry: std::env::var("NCD_SMOKE_NPM_REGISTRY").ok(),
            pypi_index: std::env::var("NCD_SMOKE_PYPI_INDEX").ok(),
            install_version: None,
            install_renderer: false,
            adopt_existing: false,
            instance_id: format!("linux-smoke-{id}"),
            webui_username: None,
            webui_password: None,
        };
        let adapter = registry.get(&manifest.id)?;
        let component = adapter.component(&spec);
        component.check_target(&host)?;
        let (mut ctx, mut events) = ActionCtx::new();
        let log_id = id.to_string();
        let logger = tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                println!(
                    "[{log_id}] {}",
                    serde_json::to_string(&event).unwrap_or_default()
                );
            }
        });
        println!("INSTALL {id} linux/local {}", spec.install_dir.as_posix());
        let installed = component.install(&host, &mut ctx).await;
        drop(ctx);
        logger.await?;
        let result = match installed {
            Err(error) => {
                serde_json::json!({ "framework": id, "ok": false, "error": error.to_string() })
            }
            Ok(()) => {
                let detected = component.detect_outcome(&host).await;
                let verified = component.verify(&host).await;
                match (detected, verified) {
                    (Ok(DetectOutcome::Installed(version)), Ok(report)) if report.ok => {
                        serde_json::json!({ "framework": id, "ok": true, "version": version.version, "source": version.source, "verify": report })
                    }
                    (detected, verified) => {
                        serde_json::json!({ "framework": id, "ok": false, "detect": format!("{detected:?}"), "verify": format!("{verified:?}") })
                    }
                }
            }
        };
        println!("RESULT {}", serde_json::to_string(&result)?);
        results.push(result);
    }
    if results.is_empty() {
        return Err("未选中任何内置框架".into());
    }
    let suffix = if selected.is_empty() {
        "all".to_string()
    } else {
        selected.join("-")
    };
    let output = root.join(format!("install-results-{suffix}.json"));
    tokio::fs::write(&output, serde_json::to_vec_pretty(&results)?).await?;
    if results.iter().any(|result| result["ok"] != true) {
        return Err(format!("部分真实安装失败，结果见 {}", output.display()).into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("此冒烟入口仅适用于 Linux 本机");
    std::process::exit(1);
}

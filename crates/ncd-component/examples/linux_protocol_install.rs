//! Explicit opt-in: cargo run -p ncd-component --example linux_protocol_install -- qq|napcat|snowluma-lite|snowluma-full [absolute-root]
//! Downloads and installs real releases beneath the supplied root; QQ may install system libraries.

#[cfg(target_os = "linux")]
mod support;

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ncd_component::{ActionCtx, Component, NapCatComponent, QQComponent, SnowLumaComponent};
    use ncd_domain::SnowLumaLinuxPackage;
    use ncd_host::{Host, HostPath, Locality, Os, local::LocalLinuxHost};
    use std::path::PathBuf;

    support::configure_network()?;

    let mut arguments = std::env::args().skip(1);
    let action = arguments
        .next()
        .ok_or("Specify qq, napcat, snowluma-lite, or snowluma-full")?;
    let root = PathBuf::from(
        arguments
            .next()
            .ok_or("Specify an absolute validation root")?,
    );
    if !root.is_absolute() || root.parent().is_none() {
        return Err(
            "Validation root must be an absolute directory below the filesystem root".into(),
        );
    }
    std::fs::create_dir_all(&root)?;
    let host = LocalLinuxHost::new().with_managed_root(&root);
    assert_eq!((host.os(), host.locality()), (Os::Linux, Locality::Local));
    let base = HostPath::from_posix(root.join("components").to_string_lossy());
    let temporary = HostPath::from_posix(root.join("protocol-tmp").to_string_lossy());
    let component: Box<dyn Component> = match action.as_str() {
        "qq" => Box::new(QQComponent::default_v3_2_25(base).with_tmp_dir(temporary)),
        "napcat" => Box::new(NapCatComponent::new(base).with_tmp_dir(temporary)),
        "snowluma-lite" | "snowluma-full" => {
            let release: serde_json::Value = ncd_network::shared_client()
                .get("https://api.github.com/repos/SnowLuma/SnowLuma/releases/latest")
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let tag = release
                .get("tag_name")
                .and_then(serde_json::Value::as_str)
                .ok_or("SnowLuma release missing tag_name")?;
            let package = if action == "snowluma-full" {
                SnowLumaLinuxPackage::Full
            } else {
                SnowLumaLinuxPackage::Lite
            };
            let suffix = if package == SnowLumaLinuxPackage::Lite {
                "-lite"
            } else {
                ""
            };
            let arch = match host.arch() {
                ncd_host::Arch::X86_64 => "x64",
                ncd_host::Arch::Aarch64 => "arm64",
                _ => return Err("Unsupported Linux architecture".into()),
            };
            let name = format!("SnowLuma-{tag}-linux-{arch}{suffix}.tar.gz");
            let asset = release
                .get("assets")
                .and_then(serde_json::Value::as_array)
                .and_then(|assets| {
                    assets.iter().find(|asset| {
                        asset.get("name").and_then(serde_json::Value::as_str) == Some(&name)
                    })
                })
                .ok_or("SnowLuma release missing requested asset")?;
            let url = asset
                .get("browser_download_url")
                .and_then(serde_json::Value::as_str)
                .ok_or("SnowLuma release asset missing download URL")?;
            let workspace =
                HostPath::from_posix(root.join(&action).join("components").to_string_lossy());
            let install_dir = workspace.join("SnowLuma");
            let mut component = SnowLumaComponent::new(workspace, url)
                .with_snowluma_dir(install_dir)
                .with_package(package);
            if let Some(sha) = asset
                .get("digest")
                .and_then(serde_json::Value::as_str)
                .and_then(|digest| digest.strip_prefix("sha256:"))
            {
                component = component.with_sha256(sha);
            }
            Box::new(component)
        }
        _ => return Err("Unknown protocol component".into()),
    };
    let (ctx, mut progress) = ActionCtx::new();
    let mut ctx = ctx.with_download_dir(root.join("protocol-downloads"));
    let output = tokio::spawn(async move {
        while let Some(event) = progress.recv().await {
            println!("{}", serde_json::to_string(&event)?);
        }
        Ok::<(), serde_json::Error>(())
    });
    let result = component.install(&host, &mut ctx).await;
    drop(ctx);
    output.await??;
    result?;
    let detected = component.detect(&host).await?;
    let verified = component.verify(&host).await?;
    println!(
        "RESULT {}",
        serde_json::json!({"component": action, "os": "linux", "locality": "local", "detected": detected, "verify": verified})
    );
    if !verified.ok || detected.is_none() {
        return Err("Installed protocol component failed verification".into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This explicit installation check requires Linux");
    std::process::exit(1);
}

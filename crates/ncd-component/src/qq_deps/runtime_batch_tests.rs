use super::*;
use crate::qq_deps::qq_qqnt_dependencies_v3_2_25;

fn distro(family: DistroFamily) -> DistroInfo {
    DistroInfo {
        family,
        name: String::new(),
        version: String::new(),
    }
}

fn packet(detector: &QqDependencyDetector, family: DistroFamily) -> serde_json::Value {
    serde_json::json!({
        "v": 1,
        "packages": detector.manifest.dependencies.iter().map(|dependency| {
            let name = if family == DistroFamily::Debian {
                &dependency.debian_package
            } else {
                &dependency.rhel_package
            };
            serde_json::json!({"base": name, "name": name, "installed": true})
        }).collect::<Vec<_>>()
    })
}

fn output(packet: &serde_json::Value) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: packet.to_string(),
        stderr: String::new(),
    }
}

#[test]
fn runtime_batch_uses_one_unprivileged_command_with_catalog_names_as_arguments() {
    let detector = QqDependencyDetector::new(qq_qqnt_dependencies_v3_2_25());
    let command = detector
        .runtime_batch_command(&distro(DistroFamily::Debian))
        .unwrap();
    assert_eq!(command.program, "sh");
    assert_eq!(command.args[0], "-c");
    assert_eq!(command.args[1], SCRIPT);
    assert_eq!(command.args[3], "debian");
    assert_eq!(
        command.args.len(),
        4 + detector.manifest.dependencies.len() * 2
    );
    assert!(!command.elevated);
    assert_eq!(command.timeout, Some(PROBE_BUDGET));
    assert_eq!(&command.args[4..8], ["libnss3", "0", "libgbm1", "0"]);
}

#[test]
fn runtime_batch_preserves_selected_t64_alias_and_missing_package_install_hint() {
    let detector = QqDependencyDetector::new(qq_qqnt_dependencies_v3_2_25());
    let distro = distro(DistroFamily::Debian);
    let mut reply = packet(&detector, DistroFamily::Debian);
    reply["packages"][2]["name"] = "libglib2.0-0t64".into();
    reply["packages"][9]["installed"] = false.into();
    let (satisfied, missing) = detector
        .parse_runtime_batch(&distro, output(&reply))
        .unwrap();
    assert_eq!(satisfied.len(), 18);
    assert!(
        satisfied
            .iter()
            .any(|package| package.name == "libglib2.0-0t64")
    );
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].name, "curl");
    assert_eq!(missing[0].detection_method, DetectionMethod::PackageManager);
    assert_eq!(
        detector.build_install_command(&distro, &missing).as_deref(),
        Some("sudo apt-get install -y curl")
    );
}

#[test]
fn runtime_batch_maps_rpm_names_without_debian_aliases() {
    let detector = QqDependencyDetector::new(qq_qqnt_dependencies_v3_2_25());
    let mut reply = packet(&detector, DistroFamily::Rhel);
    reply["packages"][0]["installed"] = false.into();
    let (satisfied, missing) = detector
        .parse_runtime_batch(&distro(DistroFamily::Rhel), output(&reply))
        .unwrap();
    assert_eq!(satisfied.len(), 18);
    assert_eq!(missing[0].name, "nss");
}

#[test]
fn runtime_batch_rejects_partial_or_untrusted_results_instead_of_reporting_missing() {
    let detector = QqDependencyDetector::new(qq_qqnt_dependencies_v3_2_25());
    let distro = distro(DistroFamily::Debian);
    for invalid in [
        "version",
        "truncated",
        "duplicate",
        "alias",
        "boolean",
        "extra",
    ] {
        let mut reply = packet(&detector, DistroFamily::Debian);
        match invalid {
            "version" => reply["v"] = 2.into(),
            "truncated" => {
                reply["packages"].as_array_mut().unwrap().pop();
            }
            "duplicate" => reply["packages"][1] = reply["packages"][0].clone(),
            "alias" => reply["packages"][0]["name"] = "libnss3t64".into(),
            "boolean" => reply["packages"][0]["installed"] = "false".into(),
            _ => reply["packages"][0]["error"] = "unavailable".into(),
        }
        assert!(
            detector
                .parse_runtime_batch(&distro, output(&reply))
                .is_err(),
            "case={invalid}"
        );
    }
    let mut failed = output(&packet(&detector, DistroFamily::Debian));
    failed.exit_code = Some(2);
    assert!(detector.parse_runtime_batch(&distro, failed).is_err());
}

#[test]
fn runtime_batch_rejects_non_catalog_shell_input() {
    let mut manifest = qq_qqnt_dependencies_v3_2_25();
    manifest.dependencies[0].debian_package = "$(touch marker)".into();
    let detector = QqDependencyDetector::new(manifest);
    assert!(
        detector
            .runtime_batch_command(&distro(DistroFamily::Debian))
            .is_err()
    );
}

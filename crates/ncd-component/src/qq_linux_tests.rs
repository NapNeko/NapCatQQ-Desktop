use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{ArchiveKind, DirEntry, HostProcess};
use std::sync::Mutex;

struct LinuxHost {
    uid: &'static str,
    sudo_error: Option<&'static str>,
    has_password: bool,
    package_output: CommandOutput,
    binaries: Vec<&'static str>,
    commands: Mutex<Vec<HostCommand>>,
}

impl LinuxHost {
    fn user() -> Self {
        Self {
            uid: "1000",
            sudo_error: None,
            has_password: false,
            package_output: output(0, "", ""),
            binaries: vec!["dpkg", "dpkg-deb"],
            commands: Mutex::new(Vec::new()),
        }
    }

    fn commands(&self) -> Vec<HostCommand> {
        self.commands.lock().unwrap().clone()
    }
}

fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

fn component(base: &str) -> QQComponent {
    QQComponent::default_v3_2_25(HostPath::from_posix(base))
}

fn package() -> HostPath {
    HostPath::from_posix("/tmp/QQ package.deb")
}

#[async_trait]
impl Host for LinuxHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        Locality::Remote
    }
    fn id(&self) -> &str {
        "remote:linux-qq-fixture"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        self.commands.lock().unwrap().push(command.clone());
        match command.program.as_str() {
            "id" => {
                assert_eq!(command.args, ["-u"]);
                Ok(output(0, self.uid, ""))
            }
            "true" => {
                assert!(command.elevated);
                assert!(command.stdin.is_none());
                Ok(match self.sudo_error {
                    Some(stderr) => output(1, "", stderr),
                    None => output(0, "", ""),
                })
            }
            "sudo" => Ok(match self.sudo_error {
                Some(stderr) => output(1, "", stderr),
                None => output(0, "", ""),
            }),
            "sh" | "dpkg-deb" | "rpm" => Ok(self.package_output.clone()),
            other => panic!("unexpected command: {other}"),
        }
    }
    async fn command_exists(&self, binary: &str) -> bool {
        self.binaries.contains(&binary)
    }
    async fn has_elevation_password(&self) -> bool {
        self.has_password
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        panic!("installation preflight must not read files")
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        panic!("installation preflight must not write files")
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        panic!("installation preflight must not list directories")
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("installation preflight must not create directories")
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("installation preflight must not remove files")
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("installation preflight must not remove directories")
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        panic!("installation preflight must not probe files")
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        panic!("installation preflight must not upload")
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        panic!("installation preflight must not download")
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        panic!("QQ packages use their native tools")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("installation must not leave a background process")
    }
}

#[tokio::test]
async fn system_root_installs_with_dpkg_without_requiring_sudo() {
    let host = LinuxHost {
        uid: "0",
        sudo_error: Some("sudo is not installed"),
        ..LinuxHost::user()
    };
    let qq = component("/");
    let access = qq.prepare_linux_install(&host).await.unwrap();
    qq.install_linux_package(&host, PackageFormat::Deb, &package(), access)
        .await
        .unwrap();

    let commands = host.commands();
    assert_eq!(access, LinuxInstallAccess::SystemRoot);
    assert_eq!(commands.len(), 2);
    assert!(!commands[1].elevated);
    assert!(commands[1].args[1].contains("dpkg --install '/tmp/QQ package.deb'"));
    assert!(commands[1].args[1].contains("wait_ncd_dpkg_lock"));
    assert!(!commands[1].args[1].contains("dpkg-deb"));
}

#[tokio::test]
async fn system_passwordless_sudo_is_checked_and_used_for_package_install() {
    let host = LinuxHost::user();
    let qq = component("/");
    let access = qq.prepare_linux_install(&host).await.unwrap();
    qq.install_linux_package(&host, PackageFormat::Deb, &package(), access)
        .await
        .unwrap();

    let commands = host.commands();
    assert_eq!(access, LinuxInstallAccess::SystemSudo);
    assert_eq!(commands.len(), 3);
    assert_eq!(commands[1].program, "true");
    assert!(commands[2].elevated);
    assert!(commands.iter().all(|command| command.stdin.is_none()));
}

#[tokio::test]
async fn missing_sudo_password_fails_before_dependencies_or_download() {
    let host = LinuxHost {
        sudo_error: Some("sudo: a password is required"),
        ..LinuxHost::user()
    };
    let (mut ctx, _events) = ActionCtx::new();
    let error = component("/")
        .install_linux(&host, &mut ctx)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("elevation_required"));
    assert!(error.contains("sudo: a password is required"));
    assert_eq!(host.commands().len(), 2);
}

#[tokio::test]
async fn incorrect_saved_sudo_password_preserves_authentication_error() {
    let host = LinuxHost {
        sudo_error: Some("sudo: 1 incorrect password attempt"),
        has_password: true,
        ..LinuxHost::user()
    };
    let error = component("/")
        .prepare_linux_install(&host)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("elevation_required"));
    assert!(error.contains("1 incorrect password attempt"));
    assert_eq!(host.commands().len(), 2);
}

#[tokio::test]
async fn saved_password_is_validated_without_putting_credentials_in_commands() {
    let host = LinuxHost {
        has_password: true,
        ..LinuxHost::user()
    };
    let qq = component("/");
    let access = qq.prepare_linux_install(&host).await.unwrap();
    qq.install_linux_package(&host, PackageFormat::Deb, &package(), access)
        .await
        .unwrap();

    let commands = host.commands();
    assert_eq!(commands[1].program, "true");
    assert!(commands[1].elevated && commands[2].elevated);
    assert!(commands.iter().all(|command| command.stdin.is_none()));
}

#[tokio::test]
async fn rootless_deb_extracts_under_the_selected_root_without_sudo_probes() {
    let host = LinuxHost {
        sudo_error: Some("sudo: a password is required"),
        ..LinuxHost::user()
    };
    let qq = component("/home/user/Napcat");
    let access = qq.prepare_linux_install(&host).await.unwrap();
    qq.install_linux_package(&host, PackageFormat::Deb, &package(), access)
        .await
        .unwrap();

    let commands = host.commands();
    assert_eq!(access, LinuxInstallAccess::Rootless);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "dpkg-deb");
    assert_eq!(
        commands[0].args,
        ["-x", "/tmp/QQ package.deb", "/home/user/Napcat"]
    );
    assert!(!commands[0].elevated);
}

#[tokio::test]
async fn system_rpm_uses_package_upgrade_without_requiring_extraction_tools() {
    let host = LinuxHost {
        binaries: vec!["rpm"],
        ..LinuxHost::user()
    };
    let qq = component("/");
    let access = qq.prepare_linux_install(&host).await.unwrap();
    let format = qq.linux_package_format(&host, access).await.unwrap();
    qq.install_linux_package(&host, format, &package(), access)
        .await
        .unwrap();

    let commands = host.commands();
    let install = commands.last().unwrap();
    assert_eq!(format, PackageFormat::Rpm);
    assert_eq!(install.program, "rpm");
    assert_eq!(
        install.args,
        ["--upgrade", "--replacepkgs", "/tmp/QQ package.deb"]
    );
    assert!(install.elevated);
}

#[tokio::test]
async fn rootless_rpm_keeps_extraction_unprivileged() {
    let host = LinuxHost::user();
    let qq = component("/home/user/QQ root");
    let access = qq.prepare_linux_install(&host).await.unwrap();
    qq.install_linux_package(&host, PackageFormat::Rpm, &package(), access)
        .await
        .unwrap();

    let commands = host.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "sh");
    assert_eq!(
        commands[0].args[1],
        "rpm2cpio '/tmp/QQ package.deb' | (cd '/home/user/QQ root' && cpio -idm)"
    );
    assert!(!commands[0].elevated);
}

#[tokio::test]
async fn package_database_lock_failure_keeps_details_without_prompting_for_password() {
    let host = LinuxHost {
        package_output: output(
            2,
            "dpkg package phase",
            "dpkg: error: database lock is held",
        ),
        ..LinuxHost::user()
    };
    let error = component("/")
        .install_linux_package(
            &host,
            PackageFormat::Deb,
            &package(),
            LinuxInstallAccess::SystemSudo,
        )
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("database lock is held"));
    assert!(error.contains("dpkg package phase"));
    assert!(!error.contains("elevation_required"));
}

#[tokio::test]
async fn expired_sudo_authentication_at_install_remains_retryable() {
    let host = LinuxHost {
        package_output: output(1, "", "sudo: a password is required"),
        ..LinuxHost::user()
    };
    let error = component("/")
        .install_linux_package(
            &host,
            PackageFormat::Deb,
            &package(),
            LinuxInstallAccess::SystemSudo,
        )
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("elevation_required"));
    assert!(error.contains("sudo: a password is required"));
}

#[tokio::test]
async fn sudo_command_lookup_failure_does_not_request_another_password() {
    let host = LinuxHost {
        package_output: output(1, "", "sudo: sh: command not found"),
        ..LinuxHost::user()
    };
    let error = component("/")
        .install_linux_package(
            &host,
            PackageFormat::Deb,
            &package(),
            LinuxInstallAccess::SystemSudo,
        )
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("sudo: sh: command not found"));
    assert!(!error.contains("elevation_required"));
}

#[test]
fn sudo_policy_denial_is_classified_as_an_elevation_failure() {
    assert!(is_sudo_authentication_failure(
        "sudo: user is not allowed to execute '/usr/bin/sh' as root on host"
    ));
}

#[test]
fn rootless_host_elevation_error_is_not_reclassified_for_sudo_retry() {
    let error = map_elevation_error(
        HostError::ElevationFailed {
            locality: "remote",
            reason: "unexpected host failure".into(),
        },
        LinuxInstallAccess::Rootless,
    );

    assert!(matches!(
        error,
        ActionError::Host(HostError::ElevationFailed { .. })
    ));
}

#[tokio::test]
async fn novnc_requests_password_before_installing_graphics_packages() {
    let host = LinuxHost {
        sudo_error: Some("sudo: a password is required"),
        ..LinuxHost::user()
    };
    let (mut ctx, _) = ActionCtx::new();
    let error = crate::novnc::NoVncComponent::new()
        .install(&host, &mut ctx)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("elevation_required"));
    assert!(error.contains("noVNC"));
    assert_eq!(host.commands().len(), 2);
    assert!(host.commands().iter().all(|command| !command.elevated));
}

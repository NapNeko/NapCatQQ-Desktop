use super::*;
use std::fs::{File, FileTimes};

fn fixture() -> (tempfile::TempDir, DownloadCacheService) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("data");
    fs::create_dir(&root).unwrap();
    let mut service = DownloadCacheService::new(&root);
    service.system_temp = dir.path().join("system-temp");
    fs::create_dir(&service.system_temp).unwrap();
    (dir, service)
}
fn write(path: &Path, age: Duration) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"cached bytes").unwrap();
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::now() - age))
        .unwrap();
}

#[tokio::test]
async fn cleaning_is_opt_in_and_preserves_backups_logs_and_unknown_files() {
    let (_dir, service) = fixture();
    let qq = service.paths.installer_cache_dir().join("qq/QQNT-9.9.exe");
    let vc = service
        .paths
        .root()
        .join("runtime/cache/vcredist/VC_redist.x64.exe.part");
    let update = service
        .paths
        .root()
        .join("runtime/tmp/NapCatQQ-Desktop-x64.msi");
    let protected = [
        service
            .paths
            .root()
            .join("tmp/migration-backup/important.json"),
        service
            .paths
            .root()
            .join("runtime/tmp/msi_install_3.4.4.log"),
        service.paths.installer_cache_dir().join("qq/notes.txt"),
        service.paths.root().join("output/download.zip"),
    ];
    for path in [&qq, &vc, &update].into_iter().chain(protected.iter()) {
        write(path, DAY * 10);
    }
    let result = service
        .clean(vec![CacheCategory::Installers, CacheCategory::Installers])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 2);
    assert_eq!(result.deleted_bytes, 24);
    assert!(!qq.exists());
    assert!(!vc.exists());
    assert!(update.exists());
    assert!(protected.iter().all(|path| path.exists()));
}

#[tokio::test]
async fn update_packages_have_a_grace_period_and_logs_are_preserved() {
    let (_dir, service) = fixture();
    let old = service
        .paths
        .root()
        .join("runtime/tmp/NapCatQQ-Desktop-x64.msi.part");
    let recent = service
        .paths
        .update_cache_dir()
        .join("NapCatQQ-Desktop-x64.msi");
    let log = service
        .paths
        .update_cache_dir()
        .join("msi_install_3.4.4.log");
    write(&old, DAY * 2);
    write(&recent, Duration::ZERO);
    write(&log, DAY * 10);
    let scan = service.scan().await.unwrap();
    assert_eq!(scan.categories[1].files, 1);
    assert_eq!(scan.categories[1].skipped, 1);
    let result = service
        .clean(vec![CacheCategory::DesktopUpdates])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 1);
    assert_eq!(result.skipped_files, 1);
    assert!(!old.exists());
    assert!(recent.exists());
    assert!(log.exists());
}

#[tokio::test]
async fn historical_temp_files_require_known_names_age_and_dead_owner() {
    let (_dir, service) = fixture();
    let old = service
        .system_temp
        .join("ncd-nodejs-22.13.0-4294967295.zip.chunk-2");
    let recent = service.system_temp.join("ncd-uv-0.9-4294967295.zip.part");
    let alive = service
        .system_temp
        .join(format!("ncd-napcat-{}-123.zip", std::process::id()));
    let unknown = service.system_temp.join("ncd-config-import-123.zip");
    write(&old, DAY * 10);
    write(&recent, Duration::ZERO);
    write(&alive, DAY * 10);
    write(&unknown, DAY * 10);
    let managed = service
        .paths
        .download_cache_dir()
        .join("staging/package/file.part");
    write(&managed, Duration::ZERO);
    let result = service
        .clean(vec![CacheCategory::DownloadResidues])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 2);
    assert_eq!(result.skipped_files, 2);
    assert!(!old.exists());
    assert!(!managed.exists());
    assert!(recent.exists());
    assert!(alive.exists());
    assert!(unknown.exists());
}

#[tokio::test]
async fn active_install_prevents_cleanup_until_upload_and_extract_finish() {
    let (_dir, service) = fixture();
    let file = service.paths.download_cache_dir().join("package.zip");
    write(&file, Duration::ZERO);
    let lease = service.lease().await.unwrap();
    assert!(service.scan().await.unwrap().busy);
    assert!(
        service
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .is_err()
    );
    assert!(service.maintenance().is_err());
    assert!(file.exists());
    drop(lease);
    assert_eq!(
        service
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .unwrap()
            .deleted_files,
        1
    );
}

#[tokio::test]
async fn maintenance_blocks_new_install_leases_and_concurrent_cleanups() {
    let (_dir, service) = fixture();
    let guard = service.maintenance().unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), service.lease())
            .await
            .is_err()
    );
    assert!(service.scan().await.is_err());
    assert!(
        service
            .clean(vec![CacheCategory::Installers])
            .await
            .is_err()
    );
    drop(guard);
    assert!(service.lease().await.is_ok());
}

#[tokio::test]
async fn retired_root_requires_restart_before_install_or_cleanup() {
    let (_dir, service) = fixture();
    write(
        &service
            .paths
            .root()
            .join(ncd_domain::DataRootRetiredMarker::FILE_NAME),
        Duration::ZERO,
    );
    assert!(service.lease().await.is_err());
    assert!(service.scan().await.is_err());
    assert!(
        service
            .clean(vec![CacheCategory::Installers])
            .await
            .is_err()
    );
}

#[test]
fn changed_file_is_not_deleted_using_an_old_scan() {
    let (_dir, service) = fixture();
    let file = service.paths.download_cache_dir().join("package.zip");
    write(&file, Duration::ZERO);
    let old = fs::metadata(&file).unwrap();
    fs::write(&file, b"a replacement with different size").unwrap();
    assert!(remove_plain_file(&file, old.len(), old.modified().ok()).is_err());
    assert!(file.exists());
}

#[test]
fn legacy_matching_does_not_accept_other_ncd_artifacts() {
    for name in [
        "ncd-napcat-win-123-456.zip.part",
        "ncd-uv-0.9-123.zip.chunk-0",
        "ncd-watch-123-v0.2.8",
        "ncd-maibot-core-123-456.tar.gz",
    ] {
        assert_eq!(legacy_pid(name), Some(123), "{name}");
    }
    for name in [
        "ncd-config-import-123.zip",
        "ncd-desktop-relaunch-123.ps1",
        "download.part",
        "ncd-uv-0.9-123.zip.chunk-junk",
        "ncd-napcat-0-456.zip",
        "ncd-watch-123-v0.2.8.log",
    ] {
        assert_eq!(legacy_pid(name), None, "{name}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn permission_denied_keeps_the_file_and_continues_cleaning_other_files() {
    use std::os::unix::fs::PermissionsExt;
    struct Restore {
        path: PathBuf,
        permissions: fs::Permissions,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.path, self.permissions.clone());
        }
    }
    let (_dir, service) = fixture();
    let denied = service
        .paths
        .download_cache_dir()
        .join("protected/package.zip");
    let allowed = service.paths.download_cache_dir().join("allowed.zip");
    write(&denied, Duration::ZERO);
    write(&allowed, Duration::ZERO);
    let directory = denied.parent().unwrap();
    let restore = Restore {
        path: directory.to_path_buf(),
        permissions: fs::metadata(directory).unwrap().permissions(),
    };
    fs::set_permissions(directory, fs::Permissions::from_mode(0o500)).unwrap();
    let result = service
        .clean(vec![CacheCategory::DownloadResidues])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 1);
    assert_eq!(result.failed_files, 1);
    assert!(
        result
            .warnings
            .iter()
            .any(|message| message.contains("权限不足"))
    );
    assert!(denied.exists());
    assert!(!allowed.exists());
    assert_eq!(
        fs::metadata(directory).unwrap().permissions().mode() & 0o777,
        0o500
    );
    drop(restore);
}

#[cfg(unix)]
#[test]
fn lease_directory_creation_rejects_links_before_creating_children() {
    let (_dir, service) = fixture();
    let link = service.paths.tmp_dir();
    std::os::unix::fs::symlink(&service.system_temp, &link).unwrap();
    assert!(super::super::cache_fs::PlainDirectories::create(&link.join("nested")).is_err());
    assert!(!service.system_temp.join("nested").exists());
    let ordinary = service.paths.root().join("ordinary/nested");
    assert!(super::super::cache_fs::PlainDirectories::create(&ordinary).is_ok());
    assert!(ordinary.is_dir());
}

#[cfg(unix)]
#[tokio::test]
async fn linked_children_and_linked_ancestors_are_never_cleaned() {
    use std::os::unix::fs::symlink;
    let (_dir, service) = fixture();
    let external = service.system_temp.join("important.bin");
    write(&external, DAY * 10);
    let cache = service.paths.download_cache_dir();
    fs::create_dir_all(&cache).unwrap();
    symlink(&external, cache.join("linked.zip")).unwrap();
    symlink(&service.system_temp, cache.join("linked-dir")).unwrap();
    let result = service
        .clean(vec![CacheCategory::DownloadResidues])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 0);
    assert_eq!(result.skipped_files, 2);
    fs::remove_file(cache.join("linked.zip")).unwrap();
    fs::remove_file(cache.join("linked-dir")).unwrap();
    fs::remove_dir(&cache).unwrap();
    symlink(&service.system_temp, &cache).unwrap();
    assert_eq!(
        service
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .unwrap()
            .deleted_files,
        0
    );
    assert!(external.exists());
}

#[cfg(windows)]
#[tokio::test]
async fn windows_independent_instances_share_the_same_cache_lease() {
    let (_dir, service) = fixture();
    let mut second = DownloadCacheService::new(service.paths.root());
    second.system_temp = service.system_temp.clone();
    let path = service.paths.download_cache_dir().join("package.zip");
    write(&path, Duration::ZERO);
    let install = service.lease().await.unwrap();
    assert!(second.scan().await.unwrap().busy);
    assert!(
        second
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .is_err()
    );
    assert!(path.exists());
    drop(install);
    let maintenance = second.maintenance().unwrap();
    assert!(service.lease().await.is_err());
    drop(maintenance);
    assert_eq!(
        second
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .unwrap()
            .deleted_files,
        1
    );
}

#[cfg(windows)]
#[tokio::test]
async fn windows_open_file_is_skipped_and_can_be_cleaned_after_close() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_dir, service) = fixture();
    let path = service.paths.download_cache_dir().join("busy.zip");
    write(&path, Duration::ZERO);
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .open(&path)
        .unwrap();
    let result = service
        .clean(vec![CacheCategory::DownloadResidues])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 0);
    assert_eq!(result.skipped_files, 1);
    drop(file);
    assert_eq!(
        service
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .unwrap()
            .deleted_files,
        1
    );
}

#[cfg(windows)]
struct RestorePermissions {
    path: PathBuf,
    original: fs::Permissions,
}
#[cfg(windows)]
impl RestorePermissions {
    fn new(path: PathBuf) -> Self {
        let original = fs::metadata(&path).unwrap().permissions();
        Self { path, original }
    }
}
#[cfg(windows)]
impl Drop for RestorePermissions {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, self.original.clone());
    }
}

#[cfg(windows)]
#[tokio::test]
async fn windows_readonly_lease_does_not_require_write_permission() {
    let (_dir, service) = fixture();
    drop(service.lease().await.unwrap());
    let path = service.paths.tmp_dir().join(".ncd-download-cache.lock");
    let restore = RestorePermissions::new(path.clone());
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    assert!(service.maintenance().is_ok());
    assert!(service.lease().await.is_ok());
    assert!(fs::metadata(&path).unwrap().permissions().readonly());
    drop(restore);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_readonly_cache_is_retained_without_changing_attributes() {
    let (_dir, service) = fixture();
    let protected = service.paths.download_cache_dir().join("readonly.zip");
    let allowed = service.paths.download_cache_dir().join("allowed.zip");
    write(&protected, Duration::ZERO);
    write(&allowed, Duration::ZERO);
    let restore = RestorePermissions::new(protected.clone());
    let mut permissions = fs::metadata(&protected).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&protected, permissions).unwrap();
    let result = service
        .clean(vec![CacheCategory::DownloadResidues])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 1);
    assert_eq!(result.failed_files, 1);
    assert!(protected.exists());
    assert!(!allowed.exists());
    assert!(fs::metadata(&protected).unwrap().permissions().readonly());
    drop(restore);
}

#[cfg(windows)]
#[test]
fn windows_directory_lease_prevents_rename_until_released() {
    let (_dir, service) = fixture();
    let directory = service.paths.root().join("pinned");
    let renamed = service.paths.root().join("renamed");
    fs::create_dir(&directory).unwrap();
    let guard = super::super::cache_fs::PlainDirectories::open(&directory).unwrap();
    assert!(fs::rename(&directory, &renamed).is_err());
    drop(guard);
    fs::rename(&directory, &renamed).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_directory_guard_rejects_a_file_handle_from_another_directory() {
    let (_dir, service) = fixture();
    let inside = service.paths.root().join("inside.zip");
    let outside = service.system_temp.join("outside.zip");
    write(&inside, Duration::ZERO);
    write(&outside, Duration::ZERO);
    let guard = super::super::cache_fs::PlainDirectories::open(service.paths.root()).unwrap();
    assert!(guard.validate_child(&File::open(&inside).unwrap()).is_ok());
    assert!(
        guard
            .validate_child(&File::open(&outside).unwrap())
            .is_err()
    );
    assert!(inside.exists());
    assert!(outside.exists());
}

#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[cfg(windows)]
#[tokio::test]
async fn windows_linked_lease_directory_does_not_create_files_in_target() {
    let (_dir, service) = fixture();
    let link = service.paths.tmp_dir();
    junction(&link, &service.system_temp);
    assert!(service.lease().await.is_err());
    assert!(service.scan().await.is_err());
    assert!(
        service
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .is_err()
    );
    assert!(
        !service
            .system_temp
            .join(".ncd-download-cache.lock")
            .exists()
    );
    fs::remove_dir(link).unwrap();
}

#[cfg(windows)]
#[tokio::test]
async fn windows_denied_delete_acl_is_preserved_and_other_files_are_cleaned() {
    struct RestoreAcl {
        paths: Vec<PathBuf>,
        sid: String,
    }
    impl Drop for RestoreAcl {
        fn drop(&mut self) {
            for path in &self.paths {
                let _ = std::process::Command::new("icacls")
                    .arg(path)
                    .args(["/remove:d", &format!("*{}", self.sid)])
                    .output();
            }
        }
    }
    let (_dir, service) = fixture();
    let protected = service
        .paths
        .download_cache_dir()
        .join("protected/package.zip");
    let allowed = service.paths.download_cache_dir().join("allowed.zip");
    write(&protected, Duration::ZERO);
    write(&allowed, Duration::ZERO);
    let whoami = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .unwrap();
    assert!(whoami.status.success());
    let text = String::from_utf8_lossy(&whoami.stdout);
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .unwrap()
        .trim_matches('"')
        .to_string();
    assert!(sid.starts_with("S-1-"));
    let parent = protected.parent().unwrap().to_path_buf();
    let restore = RestoreAcl {
        paths: vec![parent.clone(), protected.clone()],
        sid: sid.clone(),
    };
    // 只改测试临时路径；同时拒绝文件 DELETE 与父目录 DELETE_CHILD。
    for (path, rights) in [(&protected, "DE"), (&parent, "DC")] {
        let output = std::process::Command::new("icacls")
            .arg(path)
            .args(["/deny", &format!("*{sid}:({rights})")])
            .output()
            .unwrap();
        assert!(output.status.success());
    }
    let read_acl = |path: &Path| {
        let output = std::process::Command::new("icacls")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    let original_acls = [read_acl(&protected), read_acl(&parent)];
    let result = service
        .clean(vec![CacheCategory::DownloadResidues])
        .await
        .unwrap();
    assert_eq!(result.deleted_files, 1);
    assert_eq!(result.failed_files, 1);
    assert!(protected.exists());
    assert!(!allowed.exists());
    assert_eq!(original_acls, [read_acl(&protected), read_acl(&parent)]);
    drop(restore);
}

#[cfg(windows)]
#[tokio::test]
async fn windows_junction_is_skipped_without_entering_its_target() {
    let (_dir, service) = fixture();
    let external = service.system_temp.join("important.bin");
    write(&external, DAY * 10);
    let cache = service.paths.download_cache_dir();
    fs::create_dir_all(&cache).unwrap();
    let link = cache.join("junction");
    junction(&link, &service.system_temp);
    assert_eq!(
        service
            .clean(vec![CacheCategory::DownloadResidues])
            .await
            .unwrap()
            .deleted_files,
        0
    );
    assert!(external.exists());
    fs::remove_dir(link).unwrap();
}

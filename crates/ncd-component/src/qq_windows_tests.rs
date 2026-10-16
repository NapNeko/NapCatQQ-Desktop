use super::*;

const VERSION: &str = "9.9.36-53644";

fn existing_install(root: &Path) -> PathBuf {
    let installed = root.join(MANAGED_VERSIONS_DIR).join(VERSION);
    std::fs::create_dir_all(&installed).unwrap();
    std::fs::write(installed.join("QQ.exe"), b"old QQ").unwrap();
    std::fs::write(root.join(MANAGED_CURRENT_FILE), format!("{VERSION}\n")).unwrap();
    installed
}

fn extracted_files(root: &Path) -> PathBuf {
    let files = root.join("staging").join("Files");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("QQ.exe"), b"new QQ").unwrap();
    files
}

#[tokio::test]
async fn same_version_reinstall_publishes_the_new_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let installed = existing_install(tmp.path());
    let files = extracted_files(tmp.path());
    let (ctx, _rx) = ActionCtx::new();
    commit_managed_qq(&files, tmp.path(), VERSION, &ctx)
        .await
        .unwrap();
    assert_eq!(managed_qq_current_dir(tmp.path()), Some(installed.clone()));
    assert_eq!(std::fs::read(installed.join("QQ.exe")).unwrap(), b"new QQ");
    assert!(!tmp.path().join("current.next").exists());
}

#[tokio::test]
async fn failed_placement_restores_the_previous_copy_and_pointer() {
    let tmp = tempfile::tempdir().unwrap();
    let installed = existing_install(tmp.path());
    let (ctx, _rx) = ActionCtx::new();
    let result = commit_managed_qq(&tmp.path().join("missing"), tmp.path(), VERSION, &ctx).await;
    assert!(result.is_err());
    assert_eq!(managed_qq_current_dir(tmp.path()), Some(installed.clone()));
    assert_eq!(std::fs::read(installed.join("QQ.exe")).unwrap(), b"old QQ");
}

#[tokio::test]
async fn failed_pointer_write_does_not_touch_the_existing_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let installed = existing_install(tmp.path());
    let files = extracted_files(tmp.path());
    std::fs::create_dir(tmp.path().join("current.next")).unwrap();
    let (ctx, _rx) = ActionCtx::new();
    let result = commit_managed_qq(&files, tmp.path(), VERSION, &ctx).await;
    assert!(result.is_err());
    assert_eq!(managed_qq_current_dir(tmp.path()), Some(installed.clone()));
    assert_eq!(std::fs::read(installed.join("QQ.exe")).unwrap(), b"old QQ");
}

#[tokio::test]
async fn failed_pointer_switch_rolls_back_the_replacement() {
    let tmp = tempfile::tempdir().unwrap();
    let installed = existing_install(tmp.path());
    let files = extracted_files(tmp.path());
    std::fs::remove_file(tmp.path().join(MANAGED_CURRENT_FILE)).unwrap();
    std::fs::create_dir(tmp.path().join(MANAGED_CURRENT_FILE)).unwrap();
    let (ctx, _rx) = ActionCtx::new();
    let result = commit_managed_qq(&files, tmp.path(), VERSION, &ctx).await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(installed.join("QQ.exe")).unwrap(), b"old QQ");
    assert_eq!(std::fs::read(files.join("QQ.exe")).unwrap(), b"new QQ");
}

#[tokio::test]
async fn cancelled_install_does_not_replace_the_current_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let installed = existing_install(tmp.path());
    let files = extracted_files(tmp.path());
    let (ctx, _rx) = ActionCtx::new();
    ctx.cancel();
    let result = commit_managed_qq(&files, tmp.path(), VERSION, &ctx).await;
    assert!(matches!(result, Err(ActionError::Cancelled)));
    assert_eq!(std::fs::read(installed.join("QQ.exe")).unwrap(), b"old QQ");
}

#[test]
fn current_pointer_rejects_parent_directory_segments() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join(MANAGED_VERSIONS_DIR)).unwrap();
    std::fs::write(tmp.path().join("QQ.exe"), b"outside versions").unwrap();
    std::fs::write(tmp.path().join(MANAGED_CURRENT_FILE), "..").unwrap();
    assert!(managed_qq_current_dir(tmp.path()).is_none());
}

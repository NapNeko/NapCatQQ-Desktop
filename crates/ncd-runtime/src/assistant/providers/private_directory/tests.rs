use super::*;

#[tokio::test]
async fn only_the_designated_session_home_can_have_its_permissions_changed() {
    let temp = tempfile::tempdir().expect("private home fixture");
    let workspace = std::fs::canonicalize(temp.path()).expect("canonical workspace");
    let sibling = workspace.join("another-profile");
    tokio::fs::create_dir(&sibling)
        .await
        .expect("sibling fixture");
    assert!(secure_minimax_home(&workspace, &workspace).await.is_err());
    assert!(secure_minimax_home(&workspace, &sibling).await.is_err());
    let other = tempfile::tempdir().expect("outside fixture");
    assert!(secure_minimax_home(&workspace, other.path()).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn posix_home_is_private_and_parent_permissions_are_unchanged() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().expect("private home fixture");
    let workspace = std::fs::canonicalize(temp.path()).expect("workspace");
    let home = workspace.join(".ncd-agent/minimax");
    tokio::fs::create_dir_all(&home)
        .await
        .expect("profile fixture");
    tokio::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755))
        .await
        .expect("initial mode");
    let parent = home.parent().expect("parent directory");
    let parent_mode = tokio::fs::metadata(parent)
        .await
        .expect("parent metadata")
        .permissions()
        .mode();
    secure_minimax_home(&workspace, &home)
        .await
        .expect("private directory");
    assert_eq!(
        tokio::fs::metadata(&home)
            .await
            .expect("home metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        tokio::fs::metadata(parent)
            .await
            .expect("parent metadata")
            .permissions()
            .mode(),
        parent_mode
    );
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_home_or_parent_cannot_redirect_the_permission_change() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temp = tempfile::tempdir().expect("workspace fixture");
    let outside = tempfile::tempdir().expect("outside fixture");
    let workspace = std::fs::canonicalize(temp.path()).expect("workspace");
    tokio::fs::create_dir(workspace.join(".ncd-agent"))
        .await
        .expect("profile root");
    let home = workspace.join(".ncd-agent/minimax");
    let outside_mode = std::fs::metadata(outside.path())
        .expect("outside metadata")
        .permissions()
        .mode();
    symlink(outside.path(), &home).expect("home symlink");
    assert!(secure_minimax_home(&workspace, &home).await.is_err());
    assert_eq!(
        std::fs::metadata(outside.path())
            .expect("outside metadata")
            .permissions()
            .mode(),
        outside_mode
    );
}

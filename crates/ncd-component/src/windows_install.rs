//! Windows 远端组件的可恢复目录切换；断线后的重试先还原未完成的事务。
use crate::ActionError;
use ncd_host::{Host, HostPath, Os};

pub(crate) fn require_x64(host: &dyn Host, component: &str) -> Result<(), ActionError> {
    if host.os() == Os::Windows
        && host.locality() == ncd_host::Locality::Remote
        && host.arch() != ncd_host::Arch::X86_64
    {
        return Err(ActionError::other(format!(
            "{component} 的远端 Windows 发行包目前仅支持 x64"
        )));
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Transaction {
    target: String,
    staged: String,
    backup: String,
    preserve: Vec<String>,
    had_previous: bool,
}

pub(crate) fn nonce() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn marker(target: &HostPath) -> Result<HostPath, ActionError> {
    let parent = target
        .parent()
        .ok_or_else(|| ActionError::other("不能替换 Windows 盘根"))?;
    let name = target
        .file_name()
        .ok_or_else(|| ActionError::other("组件目录名为空"))?;
    Ok(parent.join(format!(".ncd-{name}-transaction.json")))
}

fn validate(transaction: &Transaction, target: &HostPath) -> Result<(), ActionError> {
    let parent = target
        .parent()
        .ok_or_else(|| ActionError::other("组件目录无父目录"))?;
    let prefix = format!("{}/", parent.as_posix().to_ascii_lowercase());
    let target_name = target
        .file_name()
        .ok_or_else(|| ActionError::other("组件目录名为空"))?
        .to_ascii_lowercase();
    if !transaction
        .target
        .eq_ignore_ascii_case(&target.render_for(Os::Windows))
        || transaction
            .preserve
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(['/', '\\', ':']))
    {
        return Err(ActionError::other("组件安装事务记录无效，保留现场等待恢复"));
    }
    for (is_backup, path) in [(false, &transaction.staged), (true, &transaction.backup)] {
        let path = HostPath::from_windows(path);
        let lower = path.as_posix().to_ascii_lowercase();
        let relative = lower.strip_prefix(&prefix).unwrap_or("");
        let first = relative.split('/').next().unwrap_or("");
        let owned = if is_backup {
            first.starts_with(&format!(".ncd-{target_name}-backup-")) && !relative.contains('/')
        } else {
            first.starts_with(".ncd-") || first.starts_with(&format!("{target_name}."))
        };
        if !path.is_absolute()
            || !lower.starts_with(&prefix)
            || !owned
            || path.as_posix().eq_ignore_ascii_case(target.as_posix())
            || path.as_posix().split('/').any(|p| p == "..")
        {
            return Err(ActionError::other("组件安装事务路径越界，保留现场等待恢复"));
        }
    }
    Ok(())
}

async fn restore_preserved(host: &dyn Host, transaction: &Transaction) -> Result<(), ActionError> {
    let staged = HostPath::from_windows(&transaction.staged);
    let backup = HostPath::from_windows(&transaction.backup);
    for name in &transaction.preserve {
        let old = backup.join(name);
        let moved = staged.join(name);
        if !host.exists(&old).await? && host.exists(&moved).await? {
            host.rename(&moved, &old).await?;
        }
    }
    Ok(())
}

pub(crate) async fn recover(host: &dyn Host, target: &HostPath) -> Result<(), ActionError> {
    let journal = marker(target)?;
    if !host.exists(&journal).await? {
        return Ok(());
    }
    let bytes = host.read_file(&journal).await?;
    let transaction: Transaction = serde_json::from_slice(&bytes)
        .map_err(|e| ActionError::other(format!("组件安装事务损坏：{e}")))?;
    validate(&transaction, target)?;
    let staged = HostPath::from_windows(&transaction.staged);
    let backup = HostPath::from_windows(&transaction.backup);
    let has_target = host.exists(target).await?;
    let has_staged = host.exists(&staged).await?;
    let has_backup = host.exists(&backup).await?;
    match (has_target, has_staged, has_backup) {
        // staged 消失代表最后一次目录切换已完成；只清理已退役的备份。
        (true, false, true) => host.remove_dir_all(&backup).await?,
        (false, _, true) => {
            restore_preserved(host, &transaction).await?;
            host.rename(&backup, target).await?;
        }
        (true, true, false) => {}
        (false, true, false) if !transaction.had_previous => {}
        (true, false, false) => {}
        _ => {
            return Err(ActionError::other(format!(
                "组件目录切换状态不明，已保留备份：{}",
                transaction.backup
            )));
        }
    }
    host.remove_file(&journal).await?;
    Ok(())
}

pub(crate) async fn replace(
    host: &dyn Host,
    target: &HostPath,
    staged: &HostPath,
    preserve: &[&str],
) -> Result<(), ActionError> {
    recover(host, target).await?;
    let parent = target
        .parent()
        .ok_or_else(|| ActionError::other("组件目录无父目录"))?;
    let backup = parent.join(format!(
        ".ncd-{}-backup-{}",
        target.file_name().unwrap_or("component"),
        nonce()
    ));
    let transaction = Transaction {
        target: target.render_for(Os::Windows),
        staged: staged.render_for(Os::Windows),
        backup: backup.render_for(Os::Windows),
        preserve: preserve.iter().map(|p| p.to_string()).collect(),
        had_previous: host.exists(target).await?,
    };
    validate(&transaction, target)?;
    let journal = marker(target)?;
    let bytes = serde_json::to_vec(&transaction).map_err(|e| ActionError::other(e.to_string()))?;
    host.write_file(&journal, &bytes).await?;
    let result = async {
        if transaction.had_previous {
            host.rename(target, &backup).await?;
        }
        for name in preserve {
            let old = backup.join(name);
            if transaction.had_previous && host.exists(&old).await? {
                let incoming = staged.join(name);
                if host.exists(&incoming).await? {
                    host.remove_dir_all(&incoming).await?;
                }
                host.rename(&old, &incoming).await?;
            }
        }
        host.rename(staged, target).await
    }
    .await;
    if let Err(error) = result {
        if let Err(restore) = recover(host, target).await {
            return Err(ActionError::other(format!(
                "安装切换失败：{error}；恢复失败：{restore}"
            )));
        }
        return Err(error.into());
    }
    // 备份清理失败时留下事务；下一次重试会再次确认目录状态后清理。
    recover(host, target).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupted_journal_cannot_move_a_different_component_or_escape_parent() {
        let target = HostPath::from_windows(r"C:\remote\components\NodeJs");
        let mut transaction = Transaction {
            target: target.render_for(Os::Windows),
            staged: r"C:\remote\components\.ncd-node-stage\payload".into(),
            backup: r"C:\remote\components\.ncd-nodejs-backup-1".into(),
            preserve: vec![],
            had_previous: true,
        };
        assert!(validate(&transaction, &target).is_ok());
        transaction.staged = r"C:\remote\components\..\..\Windows".into();
        assert!(validate(&transaction, &target).is_err());
        transaction.staged = r"C:\other\data".into();
        assert!(validate(&transaction, &target).is_err());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn reconnect_before_publish_restores_old_tree_and_preserved_user_data() {
        let temp = tempfile::tempdir().unwrap();
        let host = ncd_host::local::LocalWindowsHost::new();
        let parent = HostPath::from_windows(temp.path().to_string_lossy().as_ref());
        let target = parent.join("SnowLuma");
        let backup = parent.join(".ncd-SnowLuma-backup-test");
        let staged = parent.join(".ncd-snowluma-stage-test/payload");
        host.create_dir_all(&backup).await.unwrap();
        host.create_dir_all(&staged.join("config")).await.unwrap();
        host.write_file(&backup.join("index.mjs"), b"old release")
            .await
            .unwrap();
        host.write_file(&staged.join("index.mjs"), b"new release")
            .await
            .unwrap();
        host.write_file(&staged.join("config/owner.json"), b"user settings")
            .await
            .unwrap();
        let transaction = Transaction {
            target: target.render_for(Os::Windows),
            staged: staged.render_for(Os::Windows),
            backup: backup.render_for(Os::Windows),
            preserve: vec!["config".into()],
            had_previous: true,
        };
        host.write_file(
            &marker(&target).unwrap(),
            &serde_json::to_vec(&transaction).unwrap(),
        )
        .await
        .unwrap();
        recover(&host, &target).await.unwrap();
        assert_eq!(
            host.read_file(&target.join("index.mjs"))
                .await
                .unwrap()
                .as_ref(),
            b"old release"
        );
        assert_eq!(
            host.read_file(&target.join("config/owner.json"))
                .await
                .unwrap()
                .as_ref(),
            b"user settings"
        );
        assert!(!host.exists(&marker(&target).unwrap()).await.unwrap());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn publish_preserves_user_config_and_reconnect_does_not_revert_completed_install() {
        let temp = tempfile::tempdir().unwrap();
        let host = ncd_host::local::LocalWindowsHost::new();
        let parent = HostPath::from_windows(temp.path().to_string_lossy().as_ref());
        let target = parent.join("NapCatQQ");
        let staged = parent.join(".ncd-napcat-stage-test/payload");
        host.create_dir_all(&target.join("config")).await.unwrap();
        host.create_dir_all(&staged.join("config")).await.unwrap();
        host.write_file(&target.join("config/owner.json"), b"user config")
            .await
            .unwrap();
        host.write_file(&target.join("napcat.mjs"), b"old")
            .await
            .unwrap();
        host.write_file(&staged.join("config/owner.json"), b"bundled default")
            .await
            .unwrap();
        host.write_file(&staged.join("napcat.mjs"), b"new")
            .await
            .unwrap();
        replace(&host, &target, &staged, &["config"]).await.unwrap();
        recover(&host, &target).await.unwrap();
        assert_eq!(
            host.read_file(&target.join("config/owner.json"))
                .await
                .unwrap()
                .as_ref(),
            b"user config"
        );
        assert_eq!(
            host.read_file(&target.join("napcat.mjs"))
                .await
                .unwrap()
                .as_ref(),
            b"new"
        );
    }
}

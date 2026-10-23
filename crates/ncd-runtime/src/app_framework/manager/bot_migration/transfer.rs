use super::*;
use tokio::io::AsyncReadExt;

#[derive(Debug, serde::Deserialize)]
pub(super) struct TransferReport {
    pub bytes: u64,
    pub sha256: String,
}

pub(super) async fn run(
    host: &dyn Host,
    action: &str,
    root: &str,
    args: &[&str],
) -> Result<TransferReport, String> {
    let command = HostCommand::new("python3")
        .arg("-c")
        .arg(include_str!("../migration_transfer.py"))
        .arg(action)
        .arg(root)
        .args(args.iter().copied())
        .timeout(Duration::from_secs(3600));
    let output = host
        .run_to_string(command)
        .await
        .map_err(|e| e.to_string())?;
    if !output.success() {
        return Err(format!("迁移文件检查失败：{}", output.stderr.trim()));
    }
    serde_json::from_str(output.stdout.trim()).map_err(|e| format!("无法读取迁移校验结果：{e}"))
}

pub(super) async fn local_digest(path: &Path) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}

impl AppManager {
    pub(super) async fn migration_copy_app(
        &self,
        journal: &MigrationJournal,
        movement: &AppMove,
    ) -> Result<(), String> {
        let source = self
            .resolve_host(&movement.source.host_id)
            .await
            .map_err(|e| e.to_string())?;
        let target = self
            .resolve_host(&movement.target.host_id)
            .await
            .map_err(|e| e.to_string())?;
        let components = self.component_executor().map_err(|e| e.to_string())?;
        let (_, source_paths) = components
            .inventory()
            .host_probe(&movement.source.host_id, source.as_ref())
            .await?;
        let (_, target_paths) = components
            .inventory()
            .host_probe(&movement.target.host_id, target.as_ref())
            .await?;
        let source_home = source_paths.ok_or("无法读取源机目录")?.home;
        let target_home = target_paths.ok_or("无法读取目标机目录")?.home;
        let relative = format!(
            "ncd/migrations/{}/{}.tar.gz",
            journal.job.id, movement.source.id
        );
        let source_archive = HostPath::from_posix(&source_home).join(&relative);
        let target_archive = HostPath::from_posix(&target_home).join(&relative);
        let packed = run(
            source.as_ref(),
            "pack",
            &movement.source.install_dir,
            &[source_archive.as_posix()],
        )
        .await?;
        let local_dir = self.data_root.join("migrations").join(&journal.job.id);
        tokio::fs::create_dir_all(&local_dir)
            .await
            .map_err(|e| e.to_string())?;
        let local = local_dir.join(format!("{}.tar.gz", movement.source.id));
        source
            .download(&source_archive, &local)
            .await
            .map_err(|e| e.to_string())?;
        if tokio::fs::metadata(&local)
            .await
            .map_err(|e| e.to_string())?
            .len()
            != packed.bytes
            || local_digest(&local).await? != packed.sha256
        {
            return Err("下载快照校验失败，旧目录仍完整保留".into());
        }
        let managed = HostPath::from_posix(&target_home).join("ncd");
        let archive_dir = target_archive.parent().ok_or("快照目录无效")?;
        // .env 与数据库不能经由 SFTP 默认权限暴露给同机其他用户。
        run(
            target.as_ref(),
            "stage",
            archive_dir.as_posix(),
            &[managed.as_posix()],
        )
        .await?;
        target
            .upload(&local, &target_archive)
            .await
            .map_err(|e| e.to_string())?;
        run(
            target.as_ref(),
            "restore",
            &movement.target.install_dir,
            &[
                target_archive.as_posix(),
                &packed.sha256,
                &movement.source.install_dir,
                managed.as_posix(),
            ],
        )
        .await?;
        Ok(())
    }
}

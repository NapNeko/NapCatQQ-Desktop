//! Windows 远端 QQ 的受管安装树；控制端只保存下载缓存。
use super::*;

impl QQComponent {
    async fn remote_current_dir(&self, host: &dyn Host) -> Result<Option<HostPath>, ActionError> {
        let root = &self.install_base_dir;
        let pointer = root.join(MANAGED_CURRENT_FILE);
        if !host.exists(&pointer).await? {
            return Ok(None);
        }
        let bytes = host.read_file(&pointer).await?;
        let value = String::from_utf8_lossy(&bytes);
        let version = value.trim();
        if version.is_empty() || sanitize_qq_version_for_filename(version) != version {
            return Err(ActionError::other("远端 QQ 版本指针无效"));
        }
        let dir = root.join(MANAGED_VERSIONS_DIR).join(version);
        Ok(host.exists(&dir.join("QQ.exe")).await?.then_some(dir))
    }

    pub(super) async fn detect_remote_windows(
        &self,
        host: &dyn Host,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        if let Some(dir) = self.remote_current_dir(host).await? {
            let bytes = host.read_file(&dir.join("versions/config.json")).await?;
            return Ok(Some(DetectedVersion {
                version: parse_json_string_field(&bytes, "curVersion")
                    .unwrap_or_else(|| "unknown".into()),
                source: dir.render_for(Os::Windows),
            }));
        }
        self.detect_windows_system(host).await
    }

    pub(super) async fn install_remote_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        if host.arch() != Arch::X86_64 {
            return Err(ActionError::other("远端 QQ 侧装目前需要 Windows x64"));
        }
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        let (version, url) = fetch_windows_qq_release().await?;
        let local_installer = ctx.download_path(format!(
            "QQNT-{}.exe",
            sanitize_qq_version_for_filename(&version)
        ))?;
        let local_tool = ctx.download_path("ncd-7zr.exe")?;
        let helper = DownloadHelper::new()?;
        helper
            .download_with_mirrors(&[url], &local_installer, None, ctx, 1)
            .await?;
        helper
            .download_with_mirrors_no_chunk(
                &ncd_network::build_mirror_urls(SEVENZR_URL, None),
                &local_tool,
                Some(SEVENZR_SHA256),
                ctx,
                1,
            )
            .await?;
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        let root = &self.install_base_dir;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let stage = root.join(format!("{EXTRACT_STAGING_PREFIX}{nonce}"));
        host.create_dir_all(&stage).await?;
        let installer = stage.join("QQNT.exe");
        let tool = stage.join("7zr.exe");
        host.upload(&local_installer, &installer).await?;
        host.upload(&local_tool, &tool).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        let result = self
            .extract_remote_windows(host, ctx, &stage, &installer, &tool)
            .await;
        // 只清理本次唯一暂存目录；其他安装任务和恢复副本不属于本次任务。
        if result.is_ok() {
            let _ = host.remove_dir_all(&stage).await;
            let _ = tokio::fs::remove_file(local_installer).await;
            let _ = tokio::fs::remove_file(local_tool).await;
            ctx.emit(ProgressKind::Finished { ok: true }).await;
        }
        result
    }

    async fn extract_remote_windows(
        &self,
        host: &dyn Host,
        ctx: &ActionCtx,
        stage: &HostPath,
        installer: &HostPath,
        tool: &HostPath,
    ) -> Result<(), ActionError> {
        let exe = tool.render_for(Os::Windows);
        let installer = installer.render_for(Os::Windows);
        let listing = run_sevenzip(
            host,
            Path::new(&exe),
            &[
                "l".into(),
                "-t#".into(),
                "-sccUTF-8".into(),
                installer.clone(),
            ],
            "qq_list_payload",
        )
        .await?;
        let payload = pick_embedded_7z_payload(&listing)
            .ok_or_else(|| ActionError::other("QQ 安装包缺少内嵌 7z"))?;
        let carve = stage.join("carve");
        let out = stage.join("out");
        run_sevenzip(
            host,
            Path::new(&exe),
            &[
                "e".into(),
                "-t#".into(),
                "-y".into(),
                format!("-o{}", carve.render_for(Os::Windows)),
                installer,
                payload.clone(),
            ],
            "qq_carve_payload",
        )
        .await?;
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        run_sevenzip(
            host,
            Path::new(&exe),
            &[
                "x".into(),
                "-y".into(),
                format!("-o{}", out.render_for(Os::Windows)),
                carve.join(payload).render_for(Os::Windows),
            ],
            "qq_extract",
        )
        .await?;
        let files = out.join("Files");
        if !host.exists(&files.join("QQ.exe")).await? {
            return Err(ActionError::other("QQ 解包缺少 Files/QQ.exe"));
        }
        let bytes = host.read_file(&files.join("versions/config.json")).await?;
        let version = parse_json_string_field(&bytes, "curVersion")
            .ok_or_else(|| ActionError::other("QQ 安装包版本缺失"))?;
        if sanitize_qq_version_for_filename(&version) != version {
            return Err(ActionError::other("QQ 安装包版本无效"));
        }
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        let versions = self.install_base_dir.join(MANAGED_VERSIONS_DIR);
        host.create_dir_all(&versions).await?;
        let dest = versions.join(&version);
        crate::windows_install::recover(host, &dest).await?;
        let incoming = versions.join(format!(
            ".ncd-qq-incoming-{}",
            crate::windows_install::nonce()
        ));
        host.rename(&files, &incoming).await?;
        crate::windows_install::replace(host, &dest, &incoming, &[]).await?;
        let pointer = self.install_base_dir.join(MANAGED_CURRENT_FILE);
        let next = self
            .install_base_dir
            .join(format!("current-{}.next", crate::windows_install::nonce()));
        host.write_file(&next, format!("{version}\n").as_bytes())
            .await?;
        host.rename(&next, &pointer).await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        Ok(())
    }

    pub(super) async fn uninstall_remote_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        let versions = self.install_base_dir.join(MANAGED_VERSIONS_DIR);
        if host.exists(&versions).await? {
            host.remove_dir_all(&versions).await?;
            let pointer = self.install_base_dir.join(MANAGED_CURRENT_FILE);
            if host.exists(&pointer).await? {
                host.remove_file(&pointer).await?;
            }
        } else if self.query_windows_install_root(host).await?.is_some() {
            return Err(ActionError::other(
                "远端系统 QQ 由用户安装，请在目标机 Windows 设置中卸载",
            ));
        }
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_host::{ArchiveKind, CommandOutput, DirEntry, HostProcess};
    use std::collections::HashMap;

    struct RemoteTree {
        files: HashMap<String, Vec<u8>>,
    }
    #[async_trait]
    impl Host for RemoteTree {
        fn os(&self) -> Os {
            Os::Windows
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            "remote:fixture"
        }
        fn shell(&self) -> &dyn HostShell {
            &ncd_host::shell::PowerShellShell
        }
        async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
            Ok(self.files.contains_key(path.as_posix()))
        }
        async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
            self.files
                .get(path.as_posix())
                .cloned()
                .map(Bytes::from)
                .ok_or_else(|| HostError::Unsupported {
                    operation: "missing-fixture-file",
                })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            panic!("probe must be read-only")
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            panic!("probe must not scan directories")
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            panic!("probe must be read-only")
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            panic!("probe must be read-only")
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            panic!("probe must be read-only")
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            panic!("probe must be read-only")
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            panic!("probe must not read controller files")
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            panic!("probe must be read-only")
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            panic!("managed probe must not spawn")
        }
        async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
            panic!("managed probe must not query registry")
        }
    }

    fn fixture(version: &str) -> (QQComponent, RemoteTree) {
        let root = HostPath::from_windows(r"Z:\target-only\components\QQ");
        let dir = root.join("versions").join("9.9.36-test");
        let files = [
            (
                root.join("current.txt").as_posix().to_string(),
                version.as_bytes().to_vec(),
            ),
            (dir.join("QQ.exe").as_posix().to_string(), Vec::new()),
            (
                dir.join("versions/config.json").as_posix().to_string(),
                br#"{"curVersion":"9.9.36-test"}"#.to_vec(),
            ),
        ]
        .into_iter()
        .collect();
        (QQComponent::default_v3_2_25(root), RemoteTree { files })
    }
    #[tokio::test]
    async fn managed_windows_remote_probe_reads_target_tree_without_controller_io() {
        let (component, host) = fixture("9.9.36-test\n");
        let detected = component.detect(&host).await.unwrap().unwrap();
        assert_eq!(detected.version, "9.9.36-test");
        assert_eq!(
            detected.source,
            r"Z:\target-only\components\QQ\versions\9.9.36-test"
        );
    }
    #[tokio::test]
    async fn malicious_remote_version_pointer_cannot_escape_managed_versions() {
        let (component, host) = fixture("..\\outside");
        assert!(
            component
                .detect(&host)
                .await
                .unwrap_err()
                .to_string()
                .contains("版本指针无效")
        );
    }
}

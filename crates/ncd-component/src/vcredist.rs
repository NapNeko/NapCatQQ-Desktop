//! VcRedistComponent：Visual C++ 2015-2022 x64 运行库组件
//!
//! NapCat 官方注入器（NapCatWinBootMain.exe / NapCatWinBootHook.dll）动态链接
//! MSVCP140 / VCRUNTIME140*，机器缺这套运行库时注入器秒退、没有可读报错。
//!
//! detect 走注册表 HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64：
//! Installed（REG_DWORD）非 0 即算已装；Version（REG_SZ，形如 v14.44.35211.0）
//! 能解出来就带上，解不出不影响「已安装」判定。
//!
//! install 下载 pin 住的微软官方 VC_redist.x64.exe 后 `/quiet /norestart`
//! 静默安装，需 UAC 提权。缓存包已存在且 SHA256 匹配则复用；装成功才删包，
//! 失败保留下次接着用。

use std::path::Path;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use ncd_host::{Host, HostCommand, HostError, HostPath, Locality, Os, PathStyle};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// VC++ 2015-2022 x64 运行库检测键（微软文档确认的固定位置）
const VCREDIST_REGISTRY_KEY: &str = r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64";

/// 固定下载地址（14.44.35211.0 一代）。aka.ms 短链随微软发版滚动，滚了之后
/// 字节就配不上钉死的 SHA256，所以 URL 与摘要配套 pin，不走 aka.ms
pub const VCREDIST_X64_URL: &str = "https://download.visualstudio.microsoft.com/download/pr/bd1c8d9d-ba95-4eee-bc6e-df1fcc876373/CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B/VC_redist.x64.exe";

/// 与 VCREDIST_X64_URL 对应的产物摘要（小写十六进制）
pub const VCREDIST_X64_SHA256: &str =
    "cc0ff0eb1dc3f5188ae6300faef32bf5beeba4bdd6e8e445a9184072096b713b";

/// 与 pin 对应的产物版本号，只进日志，不参与校验
pub const VCREDIST_VERSION: &str = "14.44.35211.0";

/// 安装包缓存文件名（稳定，跨进程 / 重启可复用）
const INSTALLER_FILE_NAME: &str = "VC_redist.x64.exe";

/// vc_redist 的「装成功但建议重启」退出码（ERROR_SUCCESS_REBOOT_REQUIRED）
const EXIT_SUCCESS_REBOOT_REQUIRED: i32 = 3010;

/// UAC 提权失败时给前端的固定文案（安装包已保留，重试不重新下载）
const VCREDIST_ELEVATION_REQUIRED_MSG: &str = "VC++ 运行库安装需要管理员权限。UAC 提权未成功。请退出后右键本程序，选择「以管理员身份运行」，再点一次安装（安装包已保留，不会重新下载）。";

const SUPPORTED: &[(Os, Locality)] = &[(Os::Windows, Locality::Local)];

/// VC++ 运行库 component；只认 Windows 本机
#[derive(Debug, Clone)]
pub struct VcRedistComponent {
    /// 安装包缓存目录（工厂注入 data_root 派生的 runtime/cache/vcredist）
    pub cache_dir: HostPath,
}

impl VcRedistComponent {
    pub fn new(cache_dir: HostPath) -> Self {
        Self { cache_dir }
    }

    /// 组件元数据，给 list_components Tauri command 使用
    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::VcRedist,
            display_name: "VC++ 运行库".to_string(),
            description: "NapCat 注入器运行所需的 Visual C++ x64 运行库".to_string(),
            repo_url: Some(
                "https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist".to_string(),
            ),
            supported_targets: vec![crate::types::SupportedTarget::new(
                Os::Windows,
                Locality::Local,
            )],
            category: crate::types::ComponentCategory::RuntimeDep,
            // 系统运行库注册在 Windows「已安装的应用」里,卸载走 OS 流程
            uninstall: crate::types::UninstallSupport::SystemManaged,
        }
    }
}

#[async_trait]
impl Component for VcRedistComponent {
    fn id(&self) -> ComponentId {
        ComponentId::VcRedist
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let Some(stdout) = query_registry_value(host, "Installed").await else {
            // 键不存在（未装）时 reg.exe 退出码非 0；reg 起不来同样按未装，
            // 与 QQ 注册表探测同口径
            return Ok(None);
        };
        match parse_reg_installed(&stdout) {
            Some(true) => {
                let version = match query_registry_value(host, "Version").await {
                    Some(out) => parse_reg_version(&out).unwrap_or_else(|| "unknown".to_string()),
                    None => "unknown".to_string(),
                };
                Ok(Some(DetectedVersion {
                    version,
                    source: VCREDIST_REGISTRY_KEY.to_string(),
                }))
            }
            _ => Ok(None),
        }
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 2 }).await;

        // Step 1:下载安装包到稳定缓存;已有完整包(SHA256 匹配)直接复用
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "download VC_redist.x64.exe".into(),
        })
        .await;
        let local_exe = installer_local_path(&self.cache_dir);
        if let Some(parent) = local_exe.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ActionError::other(format!("创建 VC++ 运行库安装包缓存目录失败: {e}"))
            })?;
        }
        if cached_installer_matches(&local_exe, VCREDIST_X64_SHA256).await {
            ctx.info(format!("复用已下载的运行库安装包 {}", local_exe.display()))
                .await;
        } else {
            // 不完整 / 摘要不符的旧包不留:下载层只会再产出一份完整包
            if local_exe.is_file() {
                let _ = tokio::fs::remove_file(&local_exe).await;
            }
            // 下载链路自带落盘后 SHA256 校验(pin 摘要投毒也过不了)
            DownloadHelper::new()?
                .download_with_mirrors(
                    std::slice::from_ref(&VCREDIST_X64_URL.to_string()),
                    &local_exe,
                    Some(VCREDIST_X64_SHA256),
                    ctx,
                    1,
                )
                .await?;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // Step 2:UAC 提权静默安装;0 与 3010(需重启)都算成功,只有成功才删缓存包
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: format!("run VC_redist.x64.exe /quiet /norestart (v{VCREDIST_VERSION})"),
        })
        .await;
        let installer = local_exe.to_string_lossy().to_string();
        ctx.info(format!("运行 VC++ 运行库静默安装器: {installer}"))
            .await;
        let cmd = HostCommand::new(installer)
            .arg("/quiet")
            .arg("/norestart")
            .elevated()
            .timeout(std::time::Duration::from_secs(600));
        match host.run_to_string(cmd).await {
            Ok(out) if vcredist_exit_success(out.exit_code) => {
                let _ = tokio::fs::remove_file(&local_exe).await;
                if out.exit_code == Some(EXIT_SUCCESS_REBOOT_REQUIRED) {
                    ctx.info("VC++ 运行库安装完成，需要重启系统后生效").await;
                } else {
                    ctx.info("VC++ 运行库安装完成").await;
                }
            }
            Ok(out) => {
                // 安装器非 0:保留安装包原样报错,用户重试不用重新下载
                return Err(ActionError::install_step(
                    "vcredist_silent_install",
                    format!(
                        "installer exit={:?} stderr={}",
                        out.exit_code,
                        out.stderr.trim()
                    ),
                ));
            }
            Err(e) => return Err(map_windows_install_error(e)),
        }
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let detected = self.detect(host).await?;
        Ok(VerifyReport::ok().with_check(
            "registry Installed != 0",
            detected.is_some(),
            detected.as_ref().map(|v| v.version.clone()),
        ))
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        _args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // 系统运行库没有可启动入口,调用方拿不到的(这台组件不出现在任何启动链路)
        Err(ActionError::other("VC++ 运行库没有启动命令"))
    }
}

/// 跑 `reg query <key> /v <value>`,成功(success)才带回 stdout,其余 None
async fn query_registry_value(host: &dyn Host, value: &str) -> Option<String> {
    let cmd = HostCommand::new("reg")
        .arg("query")
        .arg(VCREDIST_REGISTRY_KEY)
        .arg("/v")
        .arg(value);
    let out = host.run_to_string(cmd).await.ok()?;
    out.success().then_some(out.stdout)
}

/// 解析 reg query /v <name> 的 stdout,取 `<name>    <ty>    <value>` 里的值
/// stdout 形如(第二行带前导缩进,真实输出是 CRLF):
///     HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\X64
///         Installed    REG_DWORD    0x1
fn parse_reg_value_line<'a>(stdout: &'a str, name: &str, ty: &str) -> Option<&'a str> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(name) {
            continue;
        }
        if let Some((_, rest)) = trimmed.split_once(ty) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Installed REG_DWORD 解析:非 0 即已装;值缺失 / 解不出为 None
fn parse_reg_installed(stdout: &str) -> Option<bool> {
    let raw = parse_reg_value_line(stdout, "Installed", "REG_DWORD")?;
    let value = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .or_else(|| raw.parse::<u32>().ok())?;
    Some(value != 0)
}

/// Version REG_SZ 解析(形如 v14.44.35211.0),缺失 / 空串为 None
fn parse_reg_version(stdout: &str) -> Option<String> {
    parse_reg_value_line(stdout, "Version", "REG_SZ").map(str::to_string)
}

/// vc_redist 退出码:0 = 成功;3010 = 成功但建议重启;其它(含被杀无码)= 失败
fn vcredist_exit_success(exit_code: Option<i32>) -> bool {
    matches!(exit_code, Some(0) | Some(EXIT_SUCCESS_REBOOT_REQUIRED))
}

/// HostError → ActionError:提权失败换成固定中文文案,其余原样转 Host 变体
fn map_windows_install_error(err: HostError) -> ActionError {
    match err {
        HostError::ElevationFailed { .. } => ActionError::other(VCREDIST_ELEVATION_REQUIRED_MSG),
        other => other.into(),
    }
}

/// 缓存目录下的安装包路径(稳定文件名)
fn installer_cache_path(cache_dir: &HostPath) -> HostPath {
    cache_dir.join(INSTALLER_FILE_NAME)
}

/// 缓存安装包的本地文件系统路径(Windows host 上 tokio::fs 直接可用)
fn installer_local_path(cache_dir: &HostPath) -> std::path::PathBuf {
    std::path::PathBuf::from(installer_cache_path(cache_dir).render(PathStyle::Windows))
}

/// 缓存安装包已存在且 SHA256 与 pin 值一致才复用;残缺 / 被换过的包重下
async fn cached_installer_matches(local_exe: &Path, expected_sha256: &str) -> bool {
    if !local_exe.is_file() {
        return false;
    }
    match sha256_file(local_exe).await {
        Ok(actual) => actual.eq_ignore_ascii_case(expected_sha256),
        Err(_) => false,
    }
}

async fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use bytes::Bytes;
    use ncd_host::shell::PowerShellShell;
    use ncd_host::{
        ArchiveKind, CommandOutput, DirEntry, HostProcess, HostShell,
    };

    #[test]
    fn supported_targets_windows_local_only() {
        let comp = VcRedistComponent::new(HostPath::from_posix("/x"));
        assert_eq!(comp.supported_targets(), SUPPORTED);
        assert_eq!(comp.id(), ComponentId::VcRedist);
    }

    #[test]
    fn info_matches_trait_supported_targets() {
        let info = VcRedistComponent::info();
        assert_eq!(info.supported_targets.len(), 1);
        assert_eq!(info.supported_targets[0].os, Os::Windows);
        assert_eq!(info.supported_targets[0].locality, Locality::Local);
        assert_eq!(info.category, crate::types::ComponentCategory::RuntimeDep);
        assert_eq!(
            info.uninstall,
            crate::types::UninstallSupport::SystemManaged
        );
    }

    #[test]
    fn parse_reg_installed_reads_dword_with_crlf() {
        let stdout = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\X64\r\n    Installed    REG_DWORD    0x1\r\n\r\n";
        assert_eq!(parse_reg_installed(stdout), Some(true));
    }

    #[test]
    fn parse_reg_installed_zero_is_not_installed() {
        let stdout = "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\X64\r\n    Installed    REG_DWORD    0x0\r\n";
        assert_eq!(parse_reg_installed(stdout), Some(false));
    }

    #[test]
    fn parse_reg_installed_missing_or_garbage_is_none() {
        let not_found = "ERROR: The system was unable to find the specified registry key or value.";
        assert_eq!(parse_reg_installed(not_found), None);
        let garbage = "    Installed    REG_DWORD    not-a-number\r\n";
        assert_eq!(parse_reg_installed(garbage), None);
    }

    #[test]
    fn parse_reg_version_extracts_sz() {
        let stdout = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\X64\r\n    Version    REG_SZ    v14.44.35211.0\r\n\r\n";
        assert_eq!(
            parse_reg_version(stdout),
            Some("v14.44.35211.0".to_string())
        );
        assert_eq!(parse_reg_version("ERROR: not found"), None);
    }

    #[test]
    fn exit_code_zero_and_3010_count_as_success() {
        assert!(vcredist_exit_success(Some(0)));
        assert!(vcredist_exit_success(Some(EXIT_SUCCESS_REBOOT_REQUIRED)));
        assert!(!vcredist_exit_success(Some(1603)));
        assert!(!vcredist_exit_success(None));
    }

    #[test]
    fn map_elevation_failed_uses_admin_relaunch_copy() {
        let err = map_windows_install_error(HostError::ElevationFailed {
            locality: "local",
            reason: "user cancelled UAC".into(),
        });
        let text = err.to_string();
        assert!(text.contains("以管理员身份运行"));
        assert!(text.contains("不会重新下载"));
    }

    #[test]
    fn installer_cache_path_is_stable() {
        let dir = HostPath::from_windows(r"C:\ProgramData\NapCatQQ Desktop\runtime\cache\vcredist");
        let a = installer_cache_path(&dir);
        let b = installer_cache_path(&dir);
        assert_eq!(a, b);
        assert!(a.as_posix().ends_with("VC_redist.x64.exe"));
        assert_eq!(
            a.render(PathStyle::Windows),
            r"C:\ProgramData\NapCatQQ Desktop\runtime\cache\vcredist\VC_redist.x64.exe"
        );
    }

    #[tokio::test]
    async fn cached_installer_matches_only_when_sha256_equal() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("VC_redist.x64.exe");
        // "hello" 的 sha256
        tokio::fs::write(&exe, b"hello").await.unwrap();
        let hello_sha = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(cached_installer_matches(&exe, hello_sha).await);
        // 大写摘要同样接受(eq_ignore_ascii_case)
        assert!(cached_installer_matches(&exe, &hello_sha.to_uppercase()).await);
        assert!(!cached_installer_matches(&exe, VCREDIST_X64_SHA256).await);
        let missing = dir.path().join("nope.exe");
        assert!(!cached_installer_matches(&missing, hello_sha).await);
    }

    /// 全脚本化 Windows host:按 (program + args) 查表返回;查不到视为命令起不来
    struct ScriptedHost {
        responses: HashMap<String, CommandOutput>,
        shell: PowerShellShell,
    }

    impl ScriptedHost {
        fn keyed(entries: Vec<(String, i32, &str)>) -> Self {
            let responses = entries
                .into_iter()
                .map(|(key, code, stdout)| {
                    (
                        key,
                        CommandOutput {
                            exit_code: Some(code),
                            stdout: stdout.to_string(),
                            stderr: String::new(),
                        },
                    )
                })
                .collect();
            Self {
                responses,
                shell: PowerShellShell,
            }
        }
    }

    fn reg_key(value_name: &str) -> String {
        format!("reg|query|{VCREDIST_REGISTRY_KEY}|/v|{value_name}")
    }

    #[async_trait]
    impl Host for ScriptedHost {
        fn os(&self) -> Os {
            Os::Windows
        }
        fn arch(&self) -> ncd_host::Arch {
            ncd_host::Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Local
        }
        fn id(&self) -> &str {
            "scripted"
        }
        fn shell(&self) -> &dyn HostShell {
            &self.shell
        }
        async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
            Ok(false)
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            let key = std::iter::once(cmd.program.as_str())
                .chain(cmd.args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("|");
            match self.responses.get(&key) {
                Some(out) => Ok(out.clone()),
                // 表里没有:模拟 reg 起不来(等价 PATH 缺失)
                None => Err(HostError::Io(std::io::Error::from(
                    std::io::ErrorKind::NotFound,
                ))),
            }
        }
    }

    fn comp() -> VcRedistComponent {
        VcRedistComponent::new(HostPath::from_posix("/x"))
    }

    #[tokio::test]
    async fn detect_none_when_registry_key_missing() {
        // reg.exe 在键不存在时退出码非 0
        let host = ScriptedHost::keyed(vec![(reg_key("Installed"), 1, "ERROR: The system was unable to find the specified registry key or value.")]);
        assert_eq!(comp().detect(&host).await.unwrap(), None);
    }

    #[tokio::test]
    async fn detect_none_when_reg_itself_cannot_start() {
        let host = ScriptedHost::keyed(vec![]);
        assert_eq!(comp().detect(&host).await.unwrap(), None);
    }

    #[tokio::test]
    async fn detect_installed_reads_version_value() {
        let installed = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\X64\r\n    Installed    REG_DWORD    0x1\r\n\r\n";
        let version = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\X64\r\n    Version    REG_SZ    v14.44.35211.0\r\n\r\n";
        let host = ScriptedHost::keyed(vec![
            (reg_key("Installed"), 0, installed),
            (reg_key("Version"), 0, version),
        ]);
        let found = comp().detect(&host).await.unwrap().unwrap();
        assert_eq!(found.version, "v14.44.35211.0");
        assert_eq!(found.source, VCREDIST_REGISTRY_KEY);
    }

    #[tokio::test]
    async fn detect_installed_without_version_still_installed() {
        // Installed=1 但 Version 值不存在(老运行库 / 部分写入):也要算已装
        let installed = "    Installed    REG_DWORD    0x1\r\n";
        let host = ScriptedHost::keyed(vec![
            (reg_key("Installed"), 0, installed),
            (reg_key("Version"), 1, "ERROR: value not found"),
        ]);
        let found = comp().detect(&host).await.unwrap().unwrap();
        assert_eq!(found.version, "unknown");
    }

    #[tokio::test]
    async fn detect_installed_zero_is_not_installed() {
        let host = ScriptedHost::keyed(vec![(
            reg_key("Installed"),
            0,
            "    Installed    REG_DWORD    0x0\r\n",
        )]);
        assert_eq!(comp().detect(&host).await.unwrap(), None);
    }
}

//! Windows SSH 授权项和私钥使用 SID ACL。
use super::super::authorized_keys::validate_key_request;
use super::{RemoteWindowsHost, ensure_success, ps_literal};
use crate::{Host, HostCommand, HostError, HostPath};

const KEY_PATH_SCRIPT: &str = r#"$ErrorActionPreference='Stop';$id=[Security.Principal.WindowsIdentity]::GetCurrent();$principal=New-Object Security.Principal.WindowsPrincipal($id);$admin=$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);if($admin){$path=[IO.Path]::Combine([Environment]::GetFolderPath('CommonApplicationData'),'ssh','administrators_authorized_keys')}else{$path=[IO.Path]::Combine([Environment]::GetFolderPath('UserProfile'),'.ssh','authorized_keys')}"#;
const KEY_ACL_SCRIPT: &str = r#"$acl=New-Object Security.AccessControl.FileSecurity;$acl.SetAccessRuleProtection($true,$false);$system=New-Object Security.Principal.SecurityIdentifier('S-1-5-18');$admins=New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544');foreach($sid in @($system,$admins)){$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','Allow')))};if(!$admin){$acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($id.User,'FullControl','Allow')));$acl.SetOwner($id.User)}else{$acl.SetOwner($admins)};[IO.File]::SetAccessControl($path,$acl)"#;

impl RemoteWindowsHost {
    pub(super) async fn edit_key(&self, line: Option<&str>, marker: &str) -> Result<(), HostError> {
        validate_key_request(line, marker)?;
        let script = format!(
            "{KEY_PATH_SCRIPT};$action=[Console]::In.ReadLine();$marker=[Console]::In.ReadLine();$key=[Console]::In.ReadLine();if($action -eq 'remove' -and ![IO.File]::Exists($path)){{exit 0}};[void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path));$lines=@();if([IO.File]::Exists($path)){{$lines=@([IO.File]::ReadAllLines($path)|Where-Object{{!$_.EndsWith(' '+$marker,[StringComparison]::Ordinal)}})}};if($action -eq 'install'){{$lines+=,($key+' '+$marker)}};[IO.File]::WriteAllLines($path,$lines,(New-Object Text.UTF8Encoding($false)));{KEY_ACL_SCRIPT}"
        );
        let command = encoded_command(&script).stdin(
            format!(
                "{}\n{marker}\n{}\n",
                if line.is_some() { "install" } else { "remove" },
                line.unwrap_or_default()
            )
            .into_bytes(),
        );
        ensure_success(self.run_to_string(command).await?, "authorized_keys")
    }

    pub(super) async fn protect_key(&self, path: &HostPath) -> Result<(), HostError> {
        let path = ps_literal(&self.windows_path(path)?);
        // SSH 只接受属于当前调用者的私钥；不继承用户根目录里的服务身份权限。
        let script = format!(
            "$ErrorActionPreference='Stop';$path={path};$id=[Security.Principal.WindowsIdentity]::GetCurrent();$admin=$false;{KEY_ACL_SCRIPT}"
        );
        ensure_success(
            self.run_to_string(encoded_command(&script)).await?,
            "secure_private_key",
        )
    }
}

fn encoded_command(script: &str) -> HostCommand {
    use base64::Engine;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    HostCommand::new("powershell.exe").args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
        &base64::engine::general_purpose::STANDARD.encode(bytes),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_acl_never_uses_localized_names_or_inherited_permissions() {
        assert!(KEY_ACL_SCRIPT.contains("SetAccessRuleProtection($true,$false)"));
        assert!(KEY_ACL_SCRIPT.contains("S-1-5-18"));
        assert!(KEY_ACL_SCRIPT.contains("S-1-5-32-544"));
        assert!(!KEY_ACL_SCRIPT.contains("LocalService"));
    }
}

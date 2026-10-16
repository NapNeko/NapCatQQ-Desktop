//! SSH 对接密钥按专用标记管理，保留用户的其它授权项。
use crate::{CommandOutput, HostCommand, HostError};

pub(super) fn validate_key_request(line: Option<&str>, marker: &str) -> Result<(), HostError> {
    if marker.is_empty()
        || marker.len() > 160
        || !marker
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_@:.".contains(&byte))
    {
        return Err(HostError::InvalidArgument {
            reason: "SSH密钥标记不合法".into(),
        });
    }
    if line.is_some_and(|line| {
        line.is_empty() || line.len() > 16_384 || line.contains(['\r', '\n', '\0'])
    }) {
        return Err(HostError::InvalidArgument {
            reason: "SSH授权项必须为一行且不超过16KiB".into(),
        });
    }
    Ok(())
}

pub(super) fn linux_key_command(
    line: Option<&str>,
    marker: &str,
) -> Result<HostCommand, HostError> {
    validate_key_request(line, marker)?;
    let action = if line.is_some() { "install" } else { "remove" };
    let script = r#"set -eu
umask 077
IFS= read -r action
IFS= read -r marker
IFS= read -r key
dir="$HOME/.ssh"
file="$dir/authorized_keys"
if [ "$action" = remove ] && [ ! -f "$file" ]; then exit 0; fi
mkdir -p "$dir"
chmod 700 "$dir"
tmp=$(mktemp "$dir/.ncd-keys.XXXXXX")
trap 'rm -f "$tmp"' EXIT HUP INT TERM
if [ -f "$file" ]; then
  while IFS= read -r existing || [ -n "$existing" ]; do
    case "$existing" in *" $marker") ;; *) printf '%s\n' "$existing" >> "$tmp";; esac
  done < "$file"
fi
if [ "$action" = install ]; then printf '%s %s\n' "$key" "$marker" >> "$tmp"; fi
chmod 600 "$tmp"
mv -f "$tmp" "$file"
"#;
    Ok(HostCommand::new("sh")
        .args(["-c", script])
        .stdin(format!("{action}\n{marker}\n{}\n", line.unwrap_or_default()).into_bytes()))
}

pub(super) fn check_output(output: CommandOutput, program: &str) -> Result<(), HostError> {
    if output.success() {
        Ok(())
    } else {
        Err(HostError::command_failed(
            program,
            output.exit_code,
            output.stderr,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_markers_and_lines_cannot_inject_another_authorized_entry() {
        assert!(
            validate_key_request(Some("ssh-ed25519 AAAA\nssh-rsa BBBB"), "ncd-link:1").is_err()
        );
        assert!(validate_key_request(Some("ssh-ed25519 AAAA"), "x\nother").is_err());
        let line = "restrict,port-forwarding,permitlisten=\"127.0.0.1:443\" ssh-ed25519 AAAA";
        let command = linux_key_command(Some(line), "ncd-link:1").unwrap();
        assert_eq!(
            String::from_utf8(command.stdin.unwrap()).unwrap(),
            format!("install\nncd-link:1\n{line}\n")
        );
    }
}

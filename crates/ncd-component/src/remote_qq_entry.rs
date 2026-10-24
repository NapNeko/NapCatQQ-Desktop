//! 远端 Linux 共用 QQ 安装树($HOME/Napcat/opt/QQ)的 package.json::main 切换
//!
//! NapCat 注入与 SnowLuma 纯 QQ 互斥依赖同一入口字段;启动前按底座写入,避免多处复制 patch 逻辑

use std::time::Duration;

use ncd_host::{Host, HostCommand, HostError, HostPath};
use sha2::{Digest, Sha256};

const REPLACE_SYSTEM_QQ_ENTRY: &str = r#"set -eu
target=$1
length=$2
desired_hash=$3
original_hash=$4
[ -f "$target" ] && [ ! -L "$target" ]
current=$(sha256sum "$target")
[ "${current%% *}" = "$original_hash" ]
directory=${target%/*}
temporary=$(mktemp "$directory/.ncd-qq-entry.XXXXXX")
trap 'rm -f -- "$temporary"' 0
trap 'exit 1' 1 2 15
tail -c "$length" > "$temporary"
actual=$(sha256sum "$temporary")
[ "${actual%% *}" = "$desired_hash" ]
chown --reference="$target" -- "$temporary"
chmod --reference="$target" -- "$temporary"
sync -f "$temporary"
current=$(sha256sum "$target")
[ "${current%% *}" = "$original_hash" ]
mv -f -- "$temporary" "$target"
"#;

/// NapCat Linux 注入后的 package.json main(与 [NapCatComponent] install 一致)
pub const QQ_MAIN_NAPCAT_INJECT: &str = "./loadNapCat.js";

/// QQ 官方默认 main(SnowLuma 冷启 / NapCat 卸载后)
pub const QQ_MAIN_NATIVE: &str = "./app_launcher/index.js";

pub fn qq_package_json_path(install_base: &HostPath) -> HostPath {
    HostPath::from_posix(ncd_domain::qq_package_json(install_base.as_posix()))
}

pub fn load_napcat_js_path(install_base: &HostPath) -> HostPath {
    HostPath::from_posix(ncd_domain::load_napcat_js(install_base.as_posix()))
}

pub fn napcat_mjs_path(install_base: &HostPath) -> HostPath {
    HostPath::from_posix(ncd_domain::napcat_mjs(install_base.as_posix()))
}

fn qq_package_json(install_base: &HostPath) -> HostPath {
    qq_package_json_path(install_base)
}

/// 仅在入口不同的时候切换；系统安装树复用 Host 提权，不改变安装目录权限。
pub async fn set_remote_qq_package_main(
    host: &dyn Host,
    install_base: &HostPath,
    main_entry: &str,
) -> Result<(), String> {
    let path = qq_package_json(install_base);
    let bytes = match host.read_file(&path).await {
        Ok(bytes) => bytes,
        Err(HostError::PathNotFound { .. }) => {
            return Err(format!(
                "未找到 QQ package.json（{}）。请先在组件页安装 QQ。",
                path.as_posix()
            ));
        }
        Err(error) => return Err(error.to_string()),
    };
    let mut json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析 package.json: {e}"))?;
    let Some(obj) = json.as_object_mut() else {
        return Err("package.json 根节点不是 object".into());
    };
    let normalized = |entry: &str| entry.strip_prefix("./").unwrap_or(entry).to_owned();
    if obj
        .get("main")
        .and_then(|value| value.as_str())
        .is_some_and(|entry| normalized(entry) == normalized(main_entry))
    {
        return Ok(());
    }
    obj.insert(
        "main".to_string(),
        serde_json::Value::String(main_entry.to_string()),
    );
    let new_bytes =
        serde_json::to_vec_pretty(&json).map_err(|e| format!("序列化 package.json: {e}"))?;
    if path.as_posix() == "/opt/QQ/resources/app/package.json" {
        let length = new_bytes.len().to_string();
        let desired_hash = hex::encode(Sha256::digest(&new_bytes));
        let original_hash = hex::encode(Sha256::digest(&bytes));
        // sudo 未消费的输入前缀不能混入 JSON；长度和摘要共同拒绝截断输入。
        let command = HostCommand::new("sh")
            .args([
                "-c",
                REPLACE_SYSTEM_QQ_ENTRY,
                "ncd-qq-entry",
                path.as_posix(),
                &length,
                &desired_hash,
                &original_hash,
            ])
            .stdin(new_bytes)
            .elevated()
            .timeout(Duration::from_secs(10));
        let output = host
            .run_to_string(command)
            .await
            .map_err(|error| error.to_string())?;
        if !output.success() {
            return Err("切换系统 QQ 入口失败，请确认远端提权权限".into());
        }
    } else {
        host.write_file(&path, &new_bytes)
            .await
            .map_err(|e| e.to_string())?;
    }
    let written = host
        .read_file(&path)
        .await
        .map_err(|error| error.to_string())?;
    let written: serde_json::Value = serde_json::from_slice(&written)
        .map_err(|_| "切换 QQ 入口后回读不是有效 JSON".to_string())?;
    if written.get("main").and_then(|value| value.as_str()) != Some(main_entry) {
        return Err("切换 QQ 入口后回读不一致，已取消启动".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "remote_qq_entry_tests.rs"]
mod io_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_constants_match_napcat_uninstall() {
        assert_eq!(QQ_MAIN_NAPCAT_INJECT, "./loadNapCat.js");
        assert_eq!(QQ_MAIN_NATIVE, "./app_launcher/index.js");
    }

    #[test]
    fn system_root_install_base_does_not_double_slash() {
        let base = HostPath::from_posix("/");
        assert_eq!(
            qq_package_json(&base).as_posix(),
            "/opt/QQ/resources/app/package.json"
        );
        assert_eq!(
            load_napcat_js_path(&base).as_posix(),
            "/opt/QQ/resources/app/loadNapCat.js"
        );
        assert_eq!(
            napcat_mjs_path(&base).as_posix(),
            "/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs"
        );
    }

    #[test]
    fn rootless_install_base_stays_under_napcat() {
        let base = HostPath::from_posix("/home/u/Napcat");
        assert_eq!(
            qq_package_json(&base).as_posix(),
            "/home/u/Napcat/opt/QQ/resources/app/package.json"
        );
    }
}

//! 远端 Linux 共用 QQ 安装树（`$HOME/Napcat/opt/QQ`）的 `package.json::main` 切换。
//!
//! NapCat 注入与 SnowLuma 纯 QQ 互斥依赖同一入口字段；启动前按底座写入，避免多处复制 patch 逻辑。

use ncd_host::{Host, HostPath};

/// NapCat Linux 注入后的 `package.json` main（与 [`NapCatComponent`] install 一致）。
pub const QQ_MAIN_NAPCAT_INJECT: &str = "./loadNapCat.js";

/// QQ 官方默认 main（SnowLuma 冷启 / NapCat 卸载后）。
pub const QQ_MAIN_NATIVE: &str = "./app_launcher/index.js";

fn qq_package_json(install_base: &HostPath) -> HostPath {
    install_base.join("opt/QQ/resources/app/package.json")
}

/// 把远端 QQ `package.json` 的 `main` 设为 `main_entry`（rootless 走 SFTP，system 布局需调用方 elevated）。
pub async fn set_remote_qq_package_main(
    host: &dyn Host,
    install_base: &HostPath,
    main_entry: &str,
) -> Result<(), String> {
    let path = qq_package_json(install_base);
    if !host.exists(&path).await.map_err(|e| e.to_string())? {
        return Err(format!(
            "未找到 QQ package.json（{}）。请先在组件页安装 QQ。",
            path.as_posix()
        ));
    }
    let bytes = host
        .read_file(&path)
        .await
        .map_err(|e| e.to_string())?;
    let mut json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析 package.json: {e}"))?;
    let Some(obj) = json.as_object_mut() else {
        return Err("package.json 根节点不是 object".into());
    };
    obj.insert(
        "main".to_string(),
        serde_json::Value::String(main_entry.to_string()),
    );
    let new_bytes =
        serde_json::to_vec_pretty(&json).map_err(|e| format!("序列化 package.json: {e}"))?;
    host.write_file(&path, &new_bytes)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_constants_match_napcat_uninstall() {
        assert_eq!(QQ_MAIN_NAPCAT_INJECT, "./loadNapCat.js");
        assert_eq!(QQ_MAIN_NATIVE, "./app_launcher/index.js");
    }
}
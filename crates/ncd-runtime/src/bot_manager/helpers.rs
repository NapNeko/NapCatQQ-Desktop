// BotManager 配置差异比较与路径写入。

use std::ffi::OsStr;
use std::path::Path;

/// Docker 的发布端口、资源和环境由容器创建参数决定，不能靠协议 WebUI 热更新。
/// 只处理同一部署目标；切换主机 / backend 的生命周期由调用方分别处理。
pub(super) fn docker_container_config_changed(
    previous: &ncd_domain::BotConfig,
    current: &ncd_domain::BotConfig,
) -> bool {
    use ncd_domain::DeploymentType;

    if previous.bot.deployment_type != DeploymentType::Docker
        || current.bot.deployment_type != DeploymentType::Docker
        || previous.bot.backend_type != current.bot.backend_type
        || previous.bot.runtime_target != current.bot.runtime_target
    {
        return false;
    }
    let mut previous_spec = ncd_deploy::DockerDeployment::build_spec(previous);
    let mut current_spec = ncd_deploy::DockerDeployment::build_spec(current);
    previous_spec.ports.sort_by_key(|p| (p.container, p.host));
    current_spec.ports.sort_by_key(|p| (p.container, p.host));
    let mut previous_settings = previous.docker.clone().unwrap_or_default();
    let mut current_settings = current.docker.clone().unwrap_or_default();
    // 比较最终映射，显式填写自动默认值或调整行顺序无需中断容器。
    previous_settings.port_mappings.clear();
    current_settings.port_mappings.clear();
    previous_spec != current_spec || previous_settings != current_settings
}

/// NapCat 启动期 Bypass 与 WebUI「系统配置」读的是共享 `napcat.json`，
/// 不随单个 Bot 切 backend / 删除清掉。
pub(crate) fn is_shared_napcat_json(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == OsStr::new("napcat.json"))
}

// ConfigDriftDialog 的 AcceptExternal / DropAdded 走 dot-path（含数组下标）。
// 中间 object 缺失时建空 object；数组越界 / 非数字段必须 Err，避免「点了没生效」。
// value=null 表示删除该位置（DropAdded）。
pub(crate) fn set_value_at_dot_path(
    root: &mut serde_json::Value,
    dot_path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let segments: Vec<&str> = dot_path.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(format!("非法 dot-path: '{dot_path}'"));
    }

    let mut cursor = root;
    for seg in &segments[..segments.len() - 1] {
        cursor = match cursor {
            serde_json::Value::Object(map) => map
                .entry((*seg).to_string())
                .or_insert_with(|| serde_json::json!({})),
            serde_json::Value::Array(arr) => {
                let idx: usize = seg
                    .parse()
                    .map_err(|_| format!("数组路径段 '{seg}' 不是合法下标"))?;
                let len = arr.len();
                arr.get_mut(idx)
                    .ok_or_else(|| format!("数组下标 {idx} 越界(长度 {len})"))?
            }
            _ => return Err(format!("路径段 '{seg}' 落在非容器值上,无法继续")),
        };
    }

    let last = segments[segments.len() - 1];
    match cursor {
        serde_json::Value::Object(map) => {
            if value.is_null() {
                map.remove(last);
            } else {
                map.insert(last.to_string(), value);
            }
            Ok(())
        }
        serde_json::Value::Array(arr) => {
            let idx: usize = last
                .parse()
                .map_err(|_| format!("数组路径段 '{last}' 不是合法下标"))?;
            if value.is_null() {
                // DropAdded 整个数组元素:越界视作已不存在(已达成),不报错
                if idx < arr.len() {
                    arr.remove(idx);
                }
                Ok(())
            } else {
                let len = arr.len();
                let slot = arr
                    .get_mut(idx)
                    .ok_or_else(|| format!("数组下标 {idx} 越界(长度 {len})"))?;
                *slot = value;
                Ok(())
            }
        }
        _ => Err(format!("路径 '{dot_path}' 的父级不是 object / array")),
    }
}

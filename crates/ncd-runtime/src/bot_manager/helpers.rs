// BotManager 纯函数 helpers：drift 路径写入 + 远端传输错误判定

use ncd_traits::runtime_backend::BotBackendError;

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

// stop/start 失败时区分「远端传输层挂了」与业务错误，便于 UI 提示重连而非假崩溃。
// Io 变体只能字符串启发式（历史错误未全量结构化）。
pub(crate) fn is_remote_transport_error(err: &BotBackendError) -> bool {
    match err {
        BotBackendError::RemoteHostTransport(_) => true,
        BotBackendError::Io(msg) => {
            let m = msg.to_ascii_lowercase();
            m.contains("disconnected")
                || m.contains("remote_disconnected")
                || m.contains("ssh")
                || m.contains("connection")
                || m.contains("poisoned")
                || m.contains("broken pipe")
                || m.contains("transport")
        }
        _ => false,
    }
}

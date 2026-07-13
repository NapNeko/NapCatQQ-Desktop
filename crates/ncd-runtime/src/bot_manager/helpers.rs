// BotManager 纯函数 helpers：drift 路径写入 + 远端传输错误判定

use ncd_traits::runtime_backend::BotBackendError;

/// 按 dot-path(如 network.httpServers)在 JSON Value 树里设值
/// 路径不存在的中间节点自动创建为 object用于应用前端 ConfigDriftDialog
/// 的 AcceptExternal 决议到渲染输出
/// 把 value 写到 root 的 dot-path 位置;value 为 null 表示删除该位置(DropAdded)
///
/// 支持 object key 与 array index(纯数字段)混合,如 network.httpClients.0.token:
/// ConfigDrift 对连接数组里的字段就是这种路径中间 object 缺失自动建;遇到数组时
/// 按下标定位现有元素,越界 / 段非数字 / 落到非容器值上一律返回错误,而不是像旧实现
/// 那样静默 return——否则用户在 ConfigDriftDialog 里对 token/url 的 AcceptExternal /
/// DropAdded 决议会"看起来点了却没生效"
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

/// 判断 BotBackendError 是否属于“远端主机传输层问题”
/// 优先匹配显式的 RemoteHostTransport 变体;
/// 对 Io 变体做字符串启发式(包含 disconnected / remote_disconnected / ssh / connection / poisoned / broken pipe 等)
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

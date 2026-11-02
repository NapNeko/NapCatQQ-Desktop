//! 为 NoneBot 反向 OneBot 对接补齐服务端驱动。

use super::manifest::DRIVER_FASTAPI;

/// 已有 ASGI 服务端驱动时保留顺序，否则把 fastapi 放到最前。
pub fn ensure_reverse_driver(current: &str) -> String {
    let current = current.trim();
    if current.is_empty() {
        return DRIVER_FASTAPI.to_string();
    }
    if current.contains("fastapi") || current.contains("aiohttp") || current.contains("quart") {
        return current.to_string();
    }
    format!("{DRIVER_FASTAPI}+{current}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_driver_prefixes_fastapi_for_forward_only_projects() {
        assert_eq!(ensure_reverse_driver(""), "~fastapi");
        assert_eq!(
            ensure_reverse_driver("~httpx+~websockets"),
            "~fastapi+~httpx+~websockets"
        );
        assert_eq!(ensure_reverse_driver("~fastapi+~httpx"), "~fastapi+~httpx");
    }
}

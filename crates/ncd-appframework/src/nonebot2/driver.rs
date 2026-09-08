//! NoneBot DRIVER 拼装:按启用适配器并 mixin,未知模块按最严(HTTP+WS 客户端)处理

use super::manifest::{DRIVER_FASTAPI, DRIVER_HTTPX, DRIVER_WEBSOCKETS};

const ONEBOT_V11: &str = "nonebot.adapters.onebot.v11";
const ONEBOT_V12: &str = "nonebot.adapters.onebot.v12";
const CONSOLE: &str = "nonebot.adapters.console";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverMixins {
    pub http: bool,
    pub ws: bool,
}

impl DriverMixins {
    pub const NONE: Self = Self {
        http: false,
        ws: false,
    };
    pub const HTTP: Self = Self {
        http: true,
        ws: false,
    };
    pub const HTTP_WS: Self = Self {
        http: true,
        ws: true,
    };

    pub fn union(self, other: Self) -> Self {
        Self {
            http: self.http || other.http,
            ws: self.ws || other.ws,
        }
    }

    pub fn as_list(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.http {
            out.push(DRIVER_HTTPX);
        }
        if self.ws {
            out.push(DRIVER_WEBSOCKETS);
        }
        out
    }
}

pub fn merge_driver(current: &str, mixins: &[&str]) -> String {
    let mut parts: Vec<String> = current
        .split('+')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        parts.push(DRIVER_FASTAPI.to_string());
    }
    for mixin in mixins {
        if !parts.iter().any(|p| p == mixin) {
            parts.push((*mixin).to_string());
        }
    }
    parts.join("+")
}

/// 官方店常见模块的 Driver 需求;未列入的按 HTTP+WS,避免再踩 QQ 那种启动即炸
pub fn adapter_driver_mixins(module_name: &str) -> DriverMixins {
    match module_name {
        ONEBOT_V11 | ONEBOT_V12 | CONSOLE => DriverMixins::NONE,
        "nonebot.adapters.telegram"
        | "nonebot.adapters.feishu"
        | "nonebot.adapters.ding"
        | "nonebot.adapters.wxmp"
        | "nonebot.adapters.github"
        | "nonebot.adapters.mail" => DriverMixins::HTTP,
        _ => DriverMixins::HTTP_WS,
    }
}

pub fn required_forward_mixins<'a>(modules: impl IntoIterator<Item = &'a str>) -> Vec<&'static str> {
    modules
        .into_iter()
        .map(adapter_driver_mixins)
        .fold(DriverMixins::NONE, DriverMixins::union)
        .as_list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_driver_adds_httpx_once() {
        assert_eq!(
            merge_driver("~fastapi", &[DRIVER_HTTPX, DRIVER_WEBSOCKETS]),
            "~fastapi+~httpx+~websockets"
        );
        assert_eq!(
            merge_driver("~fastapi+~httpx", &[DRIVER_HTTPX, DRIVER_WEBSOCKETS]),
            "~fastapi+~httpx+~websockets"
        );
        assert_eq!(merge_driver("", &[DRIVER_HTTPX]), "~fastapi+~httpx");
    }

    #[test]
    fn mixins_differ_by_adapter_family() {
        assert_eq!(adapter_driver_mixins(ONEBOT_V11), DriverMixins::NONE);
        assert_eq!(adapter_driver_mixins(ONEBOT_V12), DriverMixins::NONE);
        assert_eq!(adapter_driver_mixins(CONSOLE), DriverMixins::NONE);
        assert_eq!(
            adapter_driver_mixins("nonebot.adapters.telegram"),
            DriverMixins::HTTP
        );
        assert_eq!(
            adapter_driver_mixins("nonebot.adapters.feishu"),
            DriverMixins::HTTP
        );
        assert_eq!(
            adapter_driver_mixins("nonebot.adapters.qq"),
            DriverMixins::HTTP_WS
        );
        assert_eq!(
            adapter_driver_mixins("nonebot.adapters.discord"),
            DriverMixins::HTTP_WS
        );
        assert_eq!(
            adapter_driver_mixins("nonebot.adapters.satori"),
            DriverMixins::HTTP_WS
        );
        assert_eq!(
            required_forward_mixins([ONEBOT_V11, CONSOLE]),
            Vec::<&str>::new()
        );
        assert_eq!(
            required_forward_mixins([ONEBOT_V11, "nonebot.adapters.telegram"]),
            vec![DRIVER_HTTPX]
        );
        assert_eq!(
            required_forward_mixins([
                ONEBOT_V11,
                "nonebot.adapters.telegram",
                "nonebot.adapters.qq"
            ]),
            vec![DRIVER_HTTPX, DRIVER_WEBSOCKETS]
        );
    }
}

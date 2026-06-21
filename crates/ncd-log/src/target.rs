//! 将 tracing target 压成 legacy position 段的可读短名。

/// ncd_runtime::bot_manager → bot_manager
pub fn short_module_from_target(target: &str) -> String {
    let t = target.trim();
    if t.is_empty() {
        return "app".to_string();
    }
    if let Some(last) = t.rsplit("::").next() {
        return last.to_string();
    }
    t.to_string()
}

/// 按 target 前缀推断 LogSource（仅用于展示段，非业务枚举）。
pub fn log_source_from_target(target: &str) -> crate::facet::LogSource {
    use crate::facet::LogSource;
    if target.contains("bot_manager") || target.contains("bot_actor") || target.contains("napcat") {
        return LogSource::Bot;
    }
    if target.contains("server_manager") || target.contains("ncd_host") {
        return LogSource::Remote;
    }
    if target.contains("ncd_component") || target.contains("ncd_deploy") {
        return LogSource::Component;
    }
    if target.contains("snowluma") {
        return LogSource::Bot;
    }
    if target.contains("ncd_tauri") {
        return LogSource::Ui;
    }
    LogSource::Core
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_module() {
        assert_eq!(
            short_module_from_target("ncd_runtime::bot_manager"),
            "bot_manager"
        );
    }
}
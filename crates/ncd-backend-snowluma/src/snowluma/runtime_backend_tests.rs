use super::*;

#[test]
fn read_start_mode_defaults_to_cold() {
    // BotRuntimeConfig 不带 start_mode 字段;缺失环境变量时默认走 ColdStart
    let config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
    match read_start_mode(&config) {
        SnowLumaStartMode::ColdStart => {}
        other => panic!("expected ColdStart, got {other:?}"),
    }
}

#[test]
fn read_start_mode_recognizes_hot_start_env() {
    let mut config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
    config
        .environment
        .insert("SNOWLUMA_START_MODE".to_string(), "hot_start".to_string());
    assert!(matches!(
        read_start_mode(&config),
        SnowLumaStartMode::HotStart
    ));
}

#[test]
fn read_qq_id_parses_decimal_string() {
    let mut config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
    config
        .environment
        .insert("SNOWLUMA_QQ_ID".to_string(), "572381217".to_string());
    assert_eq!(read_qq_id(&config), Some(572381217));
}

#[test]
fn read_qq_id_returns_none_when_missing_or_invalid() {
    let mut config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
    assert_eq!(read_qq_id(&config), None);
    config
        .environment
        .insert("SNOWLUMA_QQ_ID".to_string(), "abc".to_string());
    assert_eq!(read_qq_id(&config), None);
}

//! 本机与远端 Windows agent 共用的 NapCat 加载诊断。

pub fn render_napcat_loader(entry_uri: &str, preparation: &str) -> String {
    let entry = js_string(entry_uri);
    format!(
        "console.info('[Desktop] NapCat loader entered');\n{preparation}\nimport({entry}).then(() => console.info('[Desktop] NapCat core imported')).catch(error => console.error('[Desktop] NapCat core import failed:', error && error.stack || String(error)));\n"
    )
}

pub fn js_string(value: &str) -> String {
    format!(
        "'{}'",
        value
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029")
    )
}

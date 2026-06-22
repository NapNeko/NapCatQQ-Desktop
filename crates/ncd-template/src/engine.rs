use minijinja::Environment;
use serde::Serialize;

use crate::error::{TemplateError, TemplateResult};

/// 模板引擎,编译时嵌入模板,运行时渲染
///
/// 基于 MiniJinja,支持 Jinja2 语法(变量、条件、循环、过滤器)。
/// 内置模板通过 include_str! 编译时嵌入,运行时无需文件 IO。
///
/// autoescape 默认关闭,因为 compose 模板需要控制引号位置。
/// 敏感字段(token/password)手动用 |tojson 转义。
pub struct TemplateEngine {
    env: Environment<'static>,
}

impl TemplateEngine {
    /// 创建新的模板引擎
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
        Self { env }
    }

    /// 创建预装了内置模板的引擎(NapCat, SnowLuma)
    pub fn with_builtin_templates() -> TemplateResult<Self> {
        let mut engine = Self::new();
        engine.add_template("napcat.yml", include_str!("../templates/napcat.yml.j2"))?;
        engine.add_template("snowluma.yml", include_str!("../templates/snowluma.yml.j2"))?;
        Ok(engine)
    }

    /// 添加模板(通常用 include_str! 嵌入)
    ///
    /// 模板语法错误会返回 Err,编译期 include_str! 的模板正常情况下不会失败,
    /// 但调用方仍应处理错误以防模板被修改后引入语法问题。
    ///
    /// ```ignore
    /// use ncd_template::TemplateEngine;
    ///
    /// let mut engine = TemplateEngine::new();
    /// engine.add_template(
    ///     "napcat.yml",
    ///     include_str!("../templates/napcat.yml.j2")
    /// )?;
    /// # Ok::<(), ncd_template::TemplateError>(())
    /// ```
    pub fn add_template(&mut self, name: &str, source: &str) -> TemplateResult<()> {
        self.env
            .add_template_owned(name.to_string(), source.to_string())?;
        Ok(())
    }

    /// 渲染模板
    ///
    /// ```
    /// use ncd_template::TemplateEngine;
    /// use serde_json::json;
    ///
    /// let mut engine = TemplateEngine::new();
    /// engine.add_template("test", "Hello {{ name }}!");
    ///
    /// let output = engine.render("test", &json!({
    ///     "name": "World"
    /// })).unwrap();
    /// assert_eq!(output, "Hello World!");
    /// ```
    pub fn render<S: Serialize>(&self, template_name: &str, ctx: &S) -> TemplateResult<String> {
        let tmpl = self
            .env
            .get_template(template_name)
            .map_err(|_| TemplateError::TemplateNotFound(template_name.to_string()))?;
        Ok(tmpl.render(ctx)?)
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TemplateError;
    use serde_json::json;

    #[test]
    fn render_simple_template() {
        let mut engine = TemplateEngine::new();
        engine.add_template("test", "Hello {{ name }}!").unwrap();

        let output = engine
            .render("test", &json!({ "name": "World" }))
            .unwrap();
        assert_eq!(output, "Hello World!");
    }

    #[test]
    fn render_with_conditional() {
        let mut engine = TemplateEngine::new();
        engine
            .add_template("test", "{% if show %}Visible{% else %}Hidden{% endif %}")
            .unwrap();

        let output = engine.render("test", &json!({ "show": true })).unwrap();
        assert_eq!(output, "Visible");

        let output = engine.render("test", &json!({ "show": false })).unwrap();
        assert_eq!(output, "Hidden");
    }

    #[test]
    fn render_with_loop() {
        let mut engine = TemplateEngine::new();
        engine
            .add_template("test", "{% for item in items %}{{ item }}\n{% endfor %}")
            .unwrap();

        let output = engine
            .render("test", &json!({ "items": ["a", "b", "c"] }))
            .unwrap();
        assert_eq!(output, "a\nb\nc\n");
    }

    #[test]
    fn template_not_found_error() {
        let engine = TemplateEngine::new();
        let result = engine.render("nonexistent", &json!({}));
        assert!(matches!(result, Err(TemplateError::TemplateNotFound(_))));
    }

    #[test]
    fn dot_path_access() {
        let mut engine = TemplateEngine::new();
        engine
            .add_template("test", "{{ port.host }}:{{ port.container }}")
            .unwrap();

        let output = engine
            .render(
                "test",
                &json!({ "port": { "host": "6099", "container": "6099" } }),
            )
            .unwrap();
        assert_eq!(output, "6099:6099");
    }

    #[test]
    fn tojson_filter_escapes_yaml_special_chars() {
        // tojson 过滤器把字符串渲染成 JSON 字面量(带双引号 + 转义),
        // YAML 兼容 JSON,所以 {{ token|tojson }} 生成的 "value" 在 YAML 里是合法的字符串
        let mut engine = TemplateEngine::new();
        engine.add_template("test", "TOKEN: {{ token|tojson }}").unwrap();

        // 普通 token
        let output = engine
            .render("test", &json!({ "token": "secret-123" }))
            .unwrap();
        assert_eq!(output, "TOKEN: \"secret-123\"");

        // 含特殊字符的 token(# 在 YAML 里是注释,引号保护后安全)
        let output = engine
            .render("test", &json!({ "token": "secret#comment" }))
            .unwrap();
        assert_eq!(output, "TOKEN: \"secret#comment\"");

        // 含引号的 token
        let output = engine
            .render("test", &json!({ "token": "a\"b" }))
            .unwrap();
        assert_eq!(output, "TOKEN: \"a\\\"b\"");
    }

    #[test]
    fn builtin_templates_load() {
        let engine = TemplateEngine::with_builtin_templates().unwrap();

        // 验证 NapCat 模板加载
        let napcat = engine
            .render(
                "napcat.yml",
                &json!({
                    "image": "mlikiowa/napcat-docker:latest",
                    "name": "napcat-test",
                    "uid": 1000,
                    "gid": 1000,
                    "token": "secret-123",
                    "ports": [
                        {"host": "6099", "container": "6099"}
                    ]
                }),
            )
            .unwrap();
        assert!(napcat.contains("mlikiowa/napcat-docker:latest"));
        assert!(napcat.contains("napcat-test"));
        assert!(napcat.contains("WEBUI_TOKEN: \"secret-123\""));

        // 验证 SnowLuma 模板加载
        let snowluma = engine
            .render(
                "snowluma.yml",
                &json!({
                    "image": "snowluma/snowluma:latest",
                    "name": "snowluma-test",
                    "uid": 1000,
                    "gid": 1000,
                    "vnc_passwd": "passwd-456",
                    "ports": [
                        {"host": "5900", "container": "5900"}
                    ]
                }),
            )
            .unwrap();
        assert!(snowluma.contains("snowluma/snowluma:latest"));
        assert!(snowluma.contains("snowluma-test"));
        assert!(snowluma.contains("VNC_PASSWD: \"passwd-456\""));
    }
}

//! ncd-template: 轻量模板渲染工具
//!
//! 基于 MiniJinja 的模板引擎,用于生成 Docker Compose、配置文件等文本。
//!
//! 特点:
//! - 编译时嵌入模板(include_str!),单一二进制
//! - Jinja2 兼容语法(条件、循环、过滤器)
//! - 内置 YAML 安全转义(tojson 过滤器),防止注入
//! - 类型安全的上下文(基于 serde)

mod engine;
mod error;

pub use engine::TemplateEngine;
pub use error::{TemplateError, TemplateResult};

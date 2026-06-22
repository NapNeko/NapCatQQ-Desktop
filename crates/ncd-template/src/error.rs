use thiserror::Error;

pub type TemplateResult<T> = Result<T, TemplateError>;

#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("模板未找到: {0}")]
    TemplateNotFound(String),

    #[error("模板引擎错误: {0}")]
    EngineError(#[from] minijinja::Error),
}

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[cfg(feature = "dynamic_sql")]
    #[error("error while accessing database")]
    DatabaseError(#[from] rusqlite::Error),

    #[cfg(feature = "dynamic_sql")]
    #[error("error while rendering template")]
    TemplateRenderError(#[from] handlebars::RenderError),

    #[cfg(feature = "dynamic_sql")]
    #[error("error while registering template")]
    TemplateError(#[from] handlebars::TemplateError),
}
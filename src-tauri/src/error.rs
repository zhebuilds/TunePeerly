// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Lofty(#[from] lofty::error::LoftyError),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
    #[error(transparent)]
    Rcgen(#[from] rcgen::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        Self::Message(value.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

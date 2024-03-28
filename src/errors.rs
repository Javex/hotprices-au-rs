use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Serde error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Conversion error: {0}")]
    ProductConversion(String),
    #[error("Ad result")]
    AdResult,
    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

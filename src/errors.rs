use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Http(#[from] Box<ureq::Error>),
    #[error("Io error")]
    IoError(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
    #[error("Serde error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Conversion error: {0}")]
    ProductConversion(String),
}

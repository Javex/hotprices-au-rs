use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Http error: {message:?}")]
    Http {
        url: Option<reqwest::Url>,
        status: Option<reqwest::StatusCode>,
        message: String,
    },
    #[error("Invalid header errro")]
    InvalidHeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error("Io error")]
    IoError(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
    #[error("Serde error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Conversion error: {0}")]
    ProductConversion(String),
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Http {
            url: err.url().cloned(),
            status: err.status(),
            message: err.to_string(),
        }
    }
}

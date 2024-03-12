// use std::error::Error as StdErr;
use std::fmt::Display;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    Http {
        url: Option<reqwest::Url>,
        status: Option<reqwest::StatusCode>,
        message: String,
    },
    InvalidHeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    IoError(#[from] std::io::Error),
    Message(String),
    SerdeJson(#[from] serde_json::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Error::Http { message, .. } => message.to_string(),
            e => e.to_string(),
        };
        write!(f, "{}", message)
    }
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

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
pub mod coles;

#[derive(ValueEnum, Clone, Debug, Serialize, Deserialize)]
pub enum Store {
    #[serde(rename = "coles")]
    Coles,
}

impl Display for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coles => write!(f, "coles"),
        }
    }
}

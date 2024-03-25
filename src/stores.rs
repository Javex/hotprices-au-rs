use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use strum::EnumIter;
pub mod coles;
pub mod woolies;

#[derive(ValueEnum, Clone, Debug, Serialize, Deserialize, Eq, Hash, PartialEq, Copy, EnumIter)]
pub enum Store {
    #[serde(rename = "coles")]
    Coles,
    #[serde(rename = "woolies")]
    Woolies,
}

impl Display for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coles => write!(f, "coles"),
            Self::Woolies => write!(f, "woolies"),
        }
    }
}

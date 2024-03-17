use clap::ValueEnum;
use std::fmt::Display;
pub mod coles;

#[derive(ValueEnum, Clone)]
pub enum Store {
    Coles,
}

impl Display for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coles => write!(f, "coles"),
        }
    }
}

use lazy_static::lazy_static;
use regex::Regex;
use time::{Date, OffsetDateTime};

use crate::errors::{Error, Result};

#[derive(Debug)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub price: f64,
    pub price_history: Vec<PriceHistory>,
    pub is_weighted: bool,
    pub unit: Unit,
    pub quantity: f64,
}

impl Product {
    pub fn new(
        id: i64,
        name: String,
        description: String,
        price: f64,
        is_weighted: Option<bool>,
        unit: Unit,
        quantity: f64,
    ) -> Self {
        let price_history = vec![PriceHistory {
            date: OffsetDateTime::now_utc().date(),
            price,
        }];

        Self {
            id,
            name,
            description,
            price,
            price_history,
            is_weighted: is_weighted.unwrap_or(false),
            unit,
            quantity,
        }
    }
}

#[derive(Debug)]
pub struct PriceHistory {
    pub date: Date,
    pub price: f64,
}

#[derive(Debug, PartialEq)]
pub enum Unit {
    Each,
    Grams,
    Millilitre,
    Centimetre,
}

lazy_static! {
    static ref UNIT_REGEX: Regex = Regex::new(r#"(?P<quantity>[0-9]+) ?(?P<unit>[a-z]+)"#).unwrap();
    static ref EACH_WORDS: Vec<&'static str> = vec![
        "ea", "each", "pk", "pack", "bunch", "sheets", "sachets", "capsules", "ss", "set", "pair",
        "pairs", "piece", "tablets", "rolls",
    ];
}

pub fn normalise_unit(unit: &str) -> Result<(f64, Unit)> {
    let (factor, unit) = match unit {
        // Grams
        "g" => (1.0, Unit::Grams),
        "kg" => (1000.0, Unit::Grams),
        "mg" => (0.001, Unit::Grams),

        // Millilitre
        "ml" => (1.0, Unit::Millilitre),
        "l" => (1000.0, Unit::Millilitre),

        // Centimetre
        "cm" => (1.0, Unit::Centimetre),
        "m" | "metre" => (100.0, Unit::Centimetre),

        // Each
        "dozen" => (12.0, Unit::Each),
        x if EACH_WORDS.contains(&x) => (1.0, Unit::Each),

        _ => return Err(Error::ProductConversion(format!("unknown unit: {}", unit))),
    };
    Ok((factor, unit))
}

pub fn parse_str_unit(size: &str) -> Result<(f64, Unit)> {
    let size = size.to_lowercase();
    let captures = UNIT_REGEX
        .captures(&size)
        .ok_or(Error::ProductConversion(format!(
            "regex didn't match for {}",
            size
        )))?;

    let quantity: f64 = captures
        .name("quantity")
        .ok_or(Error::ProductConversion(format!(
            "missing field quantity in {}",
            size
        )))?
        .as_str()
        .parse()
        .map_err(|e| Error::ProductConversion(format!("can't parse quantity as f64: {}", e)))?;

    let unit = captures
        .name("unit")
        .ok_or(Error::ProductConversion(format!(
            "missing field unit for {}",
            size
        )))?
        .as_str();
    let (factor, unit) = normalise_unit(unit)?;
    let quantity = quantity * factor;

    Ok((quantity, unit))
}

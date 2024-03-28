use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::errors::Error;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Copy)]
pub(crate) enum Unit {
    Each,
    Grams,
    Millilitre,
    Centimetre,
}

lazy_static! {
    static ref UNIT_REGEX: Regex = Regex::new(r"(?P<quantity>[0-9]+) ?(?P<unit>[a-z]+)").unwrap();
    static ref EACH_WORDS: Vec<&'static str> = vec![
        "ea", "each", "pk", "pack", "bunch", "sheets", "sachets", "capsules", "ss", "set", "pair",
        "pairs", "piece", "tablets", "rolls",
    ];
}

fn normalise_unit(unit: &str) -> anyhow::Result<(f64, Unit)> {
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

        _ => {
            let err = Error::ProductConversion(format!("unknown unit: {unit}"));
            return Err(anyhow::Error::from(err));
        }
    };
    Ok((factor, unit))
}

pub(crate) fn parse_str_unit(size: &str) -> anyhow::Result<(f64, Unit)> {
    let size = size.to_lowercase();
    let captures = UNIT_REGEX
        .captures(&size)
        .ok_or(Error::ProductConversion(format!(
            "regex didn't match for {size}"
        )))?;

    let quantity: f64 = captures
        .name("quantity")
        .ok_or(Error::ProductConversion(format!(
            "missing field quantity in {size}"
        )))?
        .as_str()
        .parse()
        .map_err(|e| Error::ProductConversion(format!("can't parse quantity as f64: {e}")))?;

    let unit = captures
        .name("unit")
        .ok_or(Error::ProductConversion(format!(
            "missing field unit for {size}"
        )))?
        .as_str();
    let (factor, unit) = normalise_unit(unit)?;
    let quantity = quantity * factor;

    Ok((quantity, unit))
}

#[cfg(test)]
mod test {

    use super::{parse_str_unit, Unit};

    #[test]
    fn test_unit_from_size() {
        // Grams
        assert_eq!(parse_str_unit("150g").unwrap(), (150.0, Unit::Grams));
        assert_eq!(parse_str_unit("1kg").unwrap(), (1000.0, Unit::Grams));
        assert_eq!(parse_str_unit("50mg").unwrap(), (0.05, Unit::Grams));

        // Millilitre
        assert_eq!(parse_str_unit("10ml").unwrap(), (10.0, Unit::Millilitre));
        assert_eq!(parse_str_unit("1l").unwrap(), (1000.0, Unit::Millilitre));

        // Centimetre
        assert_eq!(parse_str_unit("10cm").unwrap(), (10.0, Unit::Centimetre));
        assert_eq!(parse_str_unit("1m").unwrap(), (100.0, Unit::Centimetre));
        assert_eq!(
            parse_str_unit("1 metre").unwrap(),
            (100.0, Unit::Centimetre)
        );

        // Each
        assert_eq!(parse_str_unit("5ea").unwrap(), (5.0, Unit::Each));
        assert_eq!(parse_str_unit("5 each").unwrap(), (5.0, Unit::Each));
        assert_eq!(parse_str_unit("10 pack").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10pk").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 bunch").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 sheets").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 sachets").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 capsules").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 ss").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 set").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 pair").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("10 pairs").unwrap(), (10.0, Unit::Each));
        assert_eq!(parse_str_unit("3 piece").unwrap(), (3.0, Unit::Each));
        assert_eq!(parse_str_unit("500 tablets").unwrap(), (500.0, Unit::Each));
        assert_eq!(parse_str_unit("12 rolls").unwrap(), (12.0, Unit::Each));
        assert_eq!(parse_str_unit("2 dozen").unwrap(), (24.0, Unit::Each));
    }
}

use crate::errors::{Error, Result};
use crate::product::{PriceHistory, Product, Unit};

use itertools::{Either, Itertools};
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use time::OffsetDateTime;

const IGNORED_RESULT_TYPES: [&str; 2] = ["SINGLE_TILE", "CONTENT_ASSOCIATION"];

lazy_static! {
    static ref UNIT_REGEX: Vec<Regex> = vec![Regex::new(
        // r#"^.* (?P<quantity>[0-9]+)(?P<unit>[a-z]+):(pack(?P<count>[0-9]+)|(?P<each>ea))"#
        r#"(?P<quantity>[0-9]+) ?(?P<unit>[a-z]+)"#
    )
    .unwrap(),];

    static ref EACH_WORDS: Vec<&'static str> = vec![
    "ea", "each", "pk", "pack", "bunch", "sheets", "sachets", "capsules", "ss", "set", "pair", "pairs", "piece", "tablets", "rolls",
    ];
}

#[derive(Deserialize, Debug)]
struct PricingUnit {
    quantity: f64,
    #[serde(rename = "ofMeasureUnits")]
    of_measure_units: Option<String>,
    #[serde(rename = "isWeighted")]
    is_weighted: Option<bool>,
}

#[derive(Deserialize, Debug)]
struct Pricing {
    now: f64,
    unit: PricingUnit,
    comparable: String,
}

#[derive(Deserialize, Debug)]
pub struct SearchResult {
    id: i64,
    name: String,
    brand: String,
    description: String,
    size: String,
    pricing: Option<Pricing>,
}

impl SearchResult {
    fn from_json_value(mut value: serde_json::Value) -> Result<Option<SearchResult>> {
        let obj = match &value {
            serde_json::Value::Object(map) => map,
            x => {
                return Err(Error::Message(format!(
                    "Invalid object type value for {}",
                    x
                )))
            }
        };

        // _type field must be present
        let result_type = obj
            .get("_type")
            .ok_or(Error::Message("Missing key _type".to_string()))?;
        let result_type = match result_type {
            serde_json::Value::String(s) => s,
            x => {
                return Err(Error::Message(format!(
                    "Invalid type for _type, expected string: {}",
                    x
                )))
            }
        };

        // Ads are ignored here
        if IGNORED_RESULT_TYPES.contains(&result_type.as_str())
            && obj.get("adId").is_some_and(|x| !x.is_null())
        {
            return Ok(None);
        }

        let search_result: SearchResult = serde_json::from_value(value)?;

        Ok(Some(search_result))
    }
}

fn get_quantity_and_unit(item: &SearchResult) -> Result<(f64, Unit)> {
    let size = &item.size;
    let (parsed_quantity, unit) = parse_str_unit(size)?;
    Ok((parsed_quantity, unit))
}

fn parse_str_unit(size: &str) -> Result<(f64, Unit)> {
    let size = size.to_lowercase();
    let re = UNIT_REGEX.get(0).ok_or(Error::ProductConversion)?;
    let captures = re.captures(&size).ok_or(Error::ProductConversion)?;

    let quantity: f64 = captures
        .name("quantity")
        .ok_or(Error::ProductConversion)?
        .as_str()
        .parse()
        .map_err(|_e| Error::ProductConversion)?;

    let unit = captures
        .name("unit")
        .ok_or(Error::ProductConversion)?
        .as_str();
    let (factor, unit) = normalise_unit(unit)?;
    let quantity = quantity * factor;

    Ok((quantity, unit))
}

fn normalise_unit(unit: &str) -> Result<(f64, Unit)> {
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

        _ => return Err(Error::ProductConversion),
    };
    Ok((factor, unit))
}
//
// fn parse_comparable(item: &SearchResult) -> Result<(i64, Unit)> {
//     todo!()
// }

impl TryFrom<SearchResult> for Product {
    type Error = Error;
    fn try_from(item: SearchResult) -> std::prelude::v1::Result<Self, Self::Error> {
        let pricing = item.pricing.ok_or(Error::ProductConversion)?;
        let mut name = item.name;
        if !item.brand.is_empty() {
            name = format!("{} {}", item.brand, name);
        }

        let price_history = vec![PriceHistory {
            date: OffsetDateTime::now_utc().date(),
            price: pricing.now,
        }];

        let product = Product {
            id: item.id,
            name,
            description: item.description,
            price_history,
            is_weighted: pricing.unit.is_weighted.unwrap_or(false),
            unit: Unit::Grams,
            quantity: pricing.unit.quantity,
        };
        Ok(product)
    }
}

#[derive(Deserialize)]
struct LegacyCategory {
    #[serde(rename = "Products")]
    products: Vec<serde_json::Value>,
}

pub fn load_from_legacy(file: impl Read) -> Result<()> {
    let json_data: Vec<LegacyCategory> = serde_json::from_reader(file)?;
    let data: Vec<LegacyData> = json_data
        .into_iter()
        .map(|c| load_legacy_products(c.products))
        .collect();

    let mut all_legacy = LegacyData {
        success: Vec::new(),
        failure: Vec::new(),
    };
    for item in data.into_iter() {
        all_legacy.success.extend(item.success);
        all_legacy.failure.extend(item.failure);
    }
    println!(
        "Success: {}, Failure: {}",
        all_legacy.success.len(),
        all_legacy.failure.len(),
    );
    Ok(())
}

struct LegacyData {
    success: Vec<SearchResult>,
    failure: Vec<Error>,
}

fn load_legacy_products(products: Vec<serde_json::Value>) -> LegacyData {
    let (success, failure): (Vec<_>, Vec<_>) =
        products
            .into_iter()
            .partition_map(|v| match SearchResult::from_json_value(v) {
                Ok(v) => Either::Left(v),
                Err(v) => Either::Right(v),
            });
    let success = success.into_iter().flatten().collect();
    LegacyData { success, failure }
}

#[cfg(test)]
mod test {
    use super::super::test::load_file;
    use super::*;

    #[test]
    fn test_load_search_result() {
        let file = load_file("search_results/product.json");
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();

        let product = SearchResult::from_json_value(json_data)
            .expect("Returned error instead of result")
            .expect("Returned None instead of Some");
        assert_eq!(product.id, 42);
        assert_eq!(product.name, "Product name");
        assert_eq!(product.brand, "Brand name");
        assert_eq!(product.description, "BRAND NAME PRODUCT NAME 150G");
        assert_eq!(product.size, "150g");
        let pricing = product.pricing.expect("Price should not be missing");
        assert_eq!(pricing.now, 6.7);
        assert_eq!(pricing.comparable, "$4.47 per 100g");
        let unit = pricing.unit;
        assert_eq!(unit.quantity, 1.0);
        assert_eq!(unit.of_measure_units.unwrap(), "g");
        assert!(!unit.is_weighted.unwrap());
    }

    #[test]
    fn test_load_ad() {
        let file = load_file("search_results/ad.json");
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();

        let ad =
            SearchResult::from_json_value(json_data).expect("Returned error instead of result");
        assert!(ad.is_none());
    }

    fn get_product_result(filename: &str) -> Result<Product> {
        let file = load_file(filename);
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();
        let product = SearchResult::from_json_value(json_data)
            .expect("Returned error instead of result")
            .expect("Returned None instead of Some");
        product.try_into()
    }

    fn get_product(filename: &str) -> Product {
        get_product_result(filename).expect("Expected conversion to succeed")
    }

    #[test]
    fn test_load_normal() {
        let product = get_product("search_results/product.json");
        assert_eq!(product.id, 42);
        assert_eq!(product.name, "Brand name Product name");
        assert_eq!(product.description, "BRAND NAME PRODUCT NAME 150G");
        assert_eq!(product.price_history.len(), 1);
        let price_history = &product.price_history[0];
        assert_eq!(price_history.price, 6.7);
        // todo: date?
        assert!(!product.is_weighted);
    }

    #[test]
    #[should_panic]
    fn test_missing_price() {
        get_product_result("search_results/missing_price.json").unwrap();
    }

    #[test]
    fn test_load_empty_brand() {
        let product = get_product("search_results/empty_brand.json");
        assert_eq!(product.name, "Product name");
    }

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

use crate::conversion::{Conversion, Product};
use crate::errors::{Error, Result};
use crate::product::{price_serde, Price};
use crate::product::{ProductInfo, ProductSnapshot};
use crate::stores::coles::category::Category;
use crate::stores::Store;
use crate::unit::{parse_str_unit, Unit};
use std::fmt::Display;

use anyhow::anyhow;
use serde::Deserialize;
use std::io::Read;
use time::Date;

const IGNORED_RESULT_TYPES: [&str; 2] = ["SINGLE_TILE", "CONTENT_ASSOCIATION"];

#[derive(Deserialize, Debug)]
struct PricingUnit {
    #[serde(rename = "isWeighted")]
    is_weighted: Option<bool>,
}

#[derive(Deserialize, Debug)]
struct Pricing {
    #[serde(with = "price_serde")]
    now: Price,
    unit: PricingUnit,
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
    pub fn from_json_value(value: serde_json::Value) -> Result<SearchResult> {
        let obj = match &value {
            serde_json::Value::Object(map) => map,
            x => return Err(anyhow!("Invalid object type value for {x}").into()),
        };

        // _type field must be present
        let result_type = obj
            .get("_type")
            .ok_or_else(|| anyhow!("Missing key _type"))?;
        let result_type = match result_type {
            serde_json::Value::String(s) => s,
            x => return Err(anyhow!("Invalid type for _type, expected string: {x}").into()),
        };

        // Ads are ignored here
        if IGNORED_RESULT_TYPES.contains(&result_type.as_str())
            && obj.get("adId").is_some_and(|x| !x.is_null())
        {
            return Err(Error::AdResult);
        }

        let search_result: SearchResult = serde_json::from_value(value)?;

        Ok(search_result)
    }
}

impl Product for SearchResult {
    fn try_into_snapshot_and_date(self, date: Date) -> Result<ProductSnapshot> {
        let pricing = self.pricing.as_ref().ok_or(Error::ProductConversion(
            "missing field pricing".to_string(),
        ))?;
        let mut name = self.name.clone();
        if !self.brand.is_empty() {
            name = format!("{} {}", self.brand, name);
        }

        let (quantity, unit) = get_quantity_and_unit(&self)?;
        let product_info = ProductInfo::new(
            self.id,
            name,
            self.description,
            pricing.unit.is_weighted,
            unit,
            quantity,
            Store::Coles,
        );
        Ok(ProductSnapshot::new(product_info, pricing.now, date))
    }
}

fn get_quantity_and_unit(item: &SearchResult) -> Result<(f64, Unit)> {
    let size = &item.size;
    if size.is_empty() {
        return Err(Error::ProductConversion(String::from("empty field size")));
    }
    let (parsed_quantity, unit) = parse_str_unit(size)?;
    Ok((parsed_quantity, unit))
}

struct ConversionMetrics {
    success: usize,
    fail_search_result: usize,
    fail_product: usize,
}

impl ConversionMetrics {
    pub fn failure_rate(&self) -> f64 {
        (self.fail_search_result + self.fail_product) as f64
            / (self.success + self.fail_search_result + self.fail_product) as f64
    }
}

impl Display for ConversionMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Success: {}, Fail Search Result: {}, Fail Product: {}, Failure rate: {:.2}%",
            self.success,
            self.fail_search_result,
            self.fail_product,
            self.failure_rate() * 100.0,
        )
    }
}

pub fn load_snapshot(file: impl Read, date: Date) -> Result<Vec<ProductSnapshot>> {
    let success = Conversion::from_reader::<Category>(file, date)?;
    Ok(success)
}

#[cfg(test)]
mod test {
    use time::Month;

    use super::super::test::load_file;
    use super::*;

    #[test]
    fn test_load_search_result() {
        let file = load_file("search_results/product.json");
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();

        let product =
            SearchResult::from_json_value(json_data).expect("Returned error instead of result");
        assert_eq!(product.id, 42);
        assert_eq!(product.name, "Product name");
        assert_eq!(product.brand, "Brand name");
        assert_eq!(product.description, "BRAND NAME PRODUCT NAME 150G");
        assert_eq!(product.size, "150g");
        let pricing = product.pricing.expect("Price should not be missing");
        assert_eq!(pricing.now, 6.7.into());
        let unit = pricing.unit;
        assert!(!unit.is_weighted.unwrap());
    }

    #[test]
    fn test_load_ad() {
        let file = load_file("search_results/ad.json");
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();

        let ad = SearchResult::from_json_value(json_data);
        let err = ad.expect_err("Search result should contain ad and thus return error");
        assert!(matches!(err, Error::AdResult));
    }

    fn get_product_result(filename: &str) -> Result<ProductSnapshot> {
        let file = load_file(filename);
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();
        let product =
            SearchResult::from_json_value(json_data).expect("Returned error instead of result");
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        product.try_into_snapshot_and_date(date)
    }

    fn get_product(filename: &str) -> ProductSnapshot {
        get_product_result(filename).expect("Expected conversion to succeed")
    }

    #[test]
    fn test_load_normal() {
        let product = get_product("search_results/product.json");
        assert_eq!(product.id(), 42);
        assert_eq!(product.name(), "Brand name Product name");
        assert_eq!(product.description(), "BRAND NAME PRODUCT NAME 150G");
        assert_eq!(product.price(), 6.7.into());
        // todo: date?
        assert!(!product.is_weighted());
    }

    #[test]
    fn test_missing_price() {
        let err = get_product_result("search_results/missing_price.json").unwrap_err();
        match err {
            Error::ProductConversion(msg) => assert_eq!(msg, "missing field pricing"),
            _ => panic!("unexpected type err type"),
        }
    }

    #[test]
    fn test_load_empty_brand() {
        let product = get_product("search_results/empty_brand.json");
        assert_eq!(product.name(), "Product name");
    }
}

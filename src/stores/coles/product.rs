use crate::errors::{Error, Result};
use crate::product::{PriceHistory, Product, Unit};
use std::fmt::Display;
use std::result::Result as StdResult;

use itertools::{Either, Itertools};
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::io::Read;
use tar::Archive;
use time::OffsetDateTime;

use super::category::SearchResults;

const IGNORED_RESULT_TYPES: [&str; 2] = ["SINGLE_TILE", "CONTENT_ASSOCIATION"];

lazy_static! {
    static ref UNIT_REGEX: Regex = Regex::new(r#"(?P<quantity>[0-9]+) ?(?P<unit>[a-z]+)"#).unwrap();
    static ref EACH_WORDS: Vec<&'static str> = vec![
        "ea", "each", "pk", "pack", "bunch", "sheets", "sachets", "capsules", "ss", "set", "pair",
        "pairs", "piece", "tablets", "rolls",
    ];
}

#[derive(Deserialize, Debug)]
struct PricingUnit {
    #[serde(rename = "isWeighted")]
    is_weighted: Option<bool>,
}

#[derive(Deserialize, Debug)]
struct Pricing {
    now: f64,
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
            return Err(Error::AdResult);
        }

        let search_result: SearchResult = serde_json::from_value(value)?;

        Ok(search_result)
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

fn parse_str_unit(size: &str) -> Result<(f64, Unit)> {
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

        _ => return Err(Error::ProductConversion(format!("unknown unit: {}", unit))),
    };
    Ok((factor, unit))
}

impl TryFrom<SearchResult> for Product {
    type Error = Error;
    fn try_from(item: SearchResult) -> StdResult<Self, Self::Error> {
        let pricing = item.pricing.as_ref().ok_or(Error::ProductConversion(
            "missing field pricing".to_string(),
        ))?;
        let mut name = item.name.clone();
        if !item.brand.is_empty() {
            name = format!("{} {}", item.brand, name);
        }

        let price_history = vec![PriceHistory {
            date: OffsetDateTime::now_utc().date(),
            price: pricing.now,
        }];

        let (quantity, unit) = get_quantity_and_unit(&item)?;

        let product = Product {
            id: item.id,
            name,
            description: item.description,
            price_history,
            is_weighted: pricing.unit.is_weighted.unwrap_or(false),
            unit,
            quantity,
        };
        Ok(product)
    }
}

#[derive(Deserialize)]
struct LegacyCategory {
    #[serde(rename = "Products")]
    products: Vec<serde_json::Value>,
}

struct ConversionMetrics {
    success: usize,
    fail_search_result: usize,
    fail_product: usize,
}

impl ConversionMetrics {
    fn failure_rate(&self) -> f64 {
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

fn load_stats(all_legacy: SearchResultConversion) -> Vec<Product> {
    let legacy_success = all_legacy.success.len();
    let legacy_failure = all_legacy.failure.len();
    let products: Vec<StdResult<Product, Error>> = all_legacy
        .success
        .into_iter()
        .map(|s| s.try_into())
        .collect();
    let (success, failure): (Vec<_>, Vec<_>) = products.into_iter().partition_map(|v| match v {
        Ok(v) => Either::Left(v),
        Err(v) => Either::Right(v),
    });
    let metrics = ConversionMetrics {
        success: legacy_success,
        fail_search_result: legacy_failure,
        fail_product: failure.len(),
    };
    eprintln!("{}", metrics);
    success
}

pub fn load_from_legacy(file: impl Read) -> Result<Vec<Product>> {
    let all_legacy = SearchResultConversion::from_legacy_reader(file)?;
    let success = load_stats(all_legacy);
    Ok(success)
}

pub fn load_from_archive(archive: Archive<impl Read>) -> Result<Vec<Product>> {
    let all_legacy = SearchResultConversion::from_archive(archive)?;
    let success = load_stats(all_legacy);
    Ok(success)
}

struct SearchResultConversion {
    success: Vec<SearchResult>,
    failure: Vec<Error>,
}

type ProductList = Vec<serde_json::Value>;

impl SearchResultConversion {
    fn from_legacy_reader(file: impl Read) -> Result<Self> {
        let json_data: Vec<LegacyCategory> = serde_json::from_reader(file)?;
        let json_data: Vec<ProductList> = json_data.into_iter().map(|c| c.products).collect();
        let all_legacy = Self::full_product_list(json_data);

        Ok(all_legacy)
    }

    fn from_archive(mut archive: Archive<impl Read>) -> Result<Self> {
        let json_data: Vec<ProductList> = archive
            .entries()?
            .filter_map_ok(|entry| match entry.size() {
                0 => None,
                _ => Some(SearchResults::from_reader(entry).map(|r| r.results)),
            })
            .flatten()
            .collect::<Result<Vec<_>>>()?;
        let conversion_result_all = Self::full_product_list(json_data);
        Ok(conversion_result_all)
    }

    fn full_product_list(json_data: Vec<ProductList>) -> Self {
        let data: Vec<Self> = json_data.into_iter().map(Self::from_json_vec).collect();

        let mut all_legacy = Self {
            success: Vec::new(),
            failure: Vec::new(),
        };
        for item in data.into_iter() {
            all_legacy.success.extend(item.success);
            all_legacy.failure.extend(item.failure);
        }
        all_legacy
    }

    fn from_json_vec(products: Vec<serde_json::Value>) -> Self {
        let (success, failure): (Vec<_>, Vec<_>) =
            products
                .into_iter()
                .partition_map(|v| match SearchResult::from_json_value(v) {
                    Ok(v) => Either::Left(v),
                    Err(v) => Either::Right(v),
                });
        // remove ad results, not "real" errors
        let failure = failure
            .into_iter()
            .filter(|e| !matches!(e, Error::AdResult))
            .collect();
        SearchResultConversion { success, failure }
    }
}

#[cfg(test)]
mod test {
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
        assert_eq!(pricing.now, 6.7);
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

    fn get_product_result(filename: &str) -> Result<Product> {
        let file = load_file(filename);
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();
        let product =
            SearchResult::from_json_value(json_data).expect("Returned error instead of result");
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

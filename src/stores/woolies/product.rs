use log::{debug, warn};
use serde::Deserialize;
use std::fmt::Display;
use std::io::Read;
use time::Date;

use crate::conversion::{Conversion, Product};
use crate::errors::{Error, Result};
use crate::product::{Price, ProductInfo, ProductSnapshot};
use crate::stores::Store;
use crate::unit::{parse_str_unit, Unit};

use super::category::Category;

#[derive(Deserialize, Debug)]
pub struct BundleProduct {
    #[serde(rename = "Stockcode")]
    stockcode: i64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Description")]
    description: String,
    #[serde(rename = "Price")]
    price: Option<f64>,
    #[serde(rename = "PackageSize")]
    package_size: String,
    #[serde(rename = "CupPrice")]
    cup_price: Option<f64>,
    #[serde(rename = "CupMeasure")]
    cup_measure: String,
    #[serde(rename = "Unit")]
    unit: String,
}

impl BundleProduct {
    fn get_quantity_and_unit(&self, price: f64) -> Result<(f64, Unit)> {
        if let Ok((q, u)) = parse_str_unit(&self.package_size) {
            return Ok((q, u));
        }

        if self.unit.to_lowercase() == "each" && self.package_size.to_lowercase() == "each" {
            return Ok((1.0, Unit::Each));
        }

        // Try cup_measure which is standardised. We can multiply!
        let (std_quantity, unit) = match parse_str_unit(&self.cup_measure) {
            Ok((q, u)) => (q, u),
            Err(e) => {
                debug!("Error converting {self:?} due to parsing error {e}");
                return Err(e.into());
            }
        };

        let cup_price = match self.cup_price {
            Some(v) => v,
            None => {
                return Err(Error::ProductConversion(String::from(
                    "Missing cup price, unable to calculate quantity",
                )));
            }
        };
        let quantity = (price / cup_price * std_quantity).round();
        if quantity < 100.0 {
            warn!("Low quantity of {quantity} during conversion of {self:?}");
            return Err(Error::ProductConversion(String::from(
                "Low quantity for conversion",
            )));
        }
        Ok((quantity, unit))
    }
}

impl Product for BundleProduct {
    fn try_into_snapshot_and_date(self, date: Date) -> Result<ProductSnapshot> {
        let price = self
            .price
            .ok_or_else(|| Error::ProductConversion(format!("Missing price on {}", self.name)))?;

        let (quantity, unit) = if self.cup_measure == "1EA" {
            (1.0, Unit::Each)
        } else {
            self.get_quantity_and_unit(price)?
        };
        // let (quantity, unit) = parse_str_unit(&self.package_size)?;
        let is_weighted = Some(false);
        let product_info = ProductInfo::new(
            self.stockcode,
            self.name,
            self.description,
            is_weighted,
            unit,
            quantity,
            Store::Woolies,
        );
        Ok(ProductSnapshot::new(product_info, Price::from(price), date))
    }
}

#[derive(Deserialize, Debug)]
pub struct Bundle {
    #[serde(rename = "Products")]
    pub products: Vec<BundleProduct>,
}

impl Bundle {
    pub fn from_json_value(value: serde_json::Value) -> Result<Bundle> {
        let bundle = match serde_json::from_value(value.clone()) {
            Ok(b) => b,
            Err(e) => {
                debug!("Error reading {value:?}: {e}");
                return Err(e.into());
            }
        };
        Ok(bundle)
    }
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
    let success = Conversion::<BundleProduct>::from_reader::<Category>(file, date)?;
    Ok(success)
}

#[cfg(test)]
mod test {
    use core::panic;
    use std::{fs, path::PathBuf};

    use time::Month;

    use super::*;

    pub fn load_file(fname: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("resources/test/woolies");
        path.push(fname);
        fs::read_to_string(path).expect("Failed to load test file")
    }

    #[test]
    fn test_load_product() {
        let file = load_file("categories/one-product.json");
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();

        let bundle = Bundle::from_json_value(json_data).expect("Returned error instead of result");
        assert_eq!(bundle.products.len(), 1);
        let [ref product] = bundle.products[..] else {
            panic!("invalid size")
        };

        assert_eq!(product.stockcode, 123);
        assert_eq!(product.name, "product name");
        assert_eq!(product.description, "some long product description");
        assert_eq!(product.price, Some(12.02));
        assert_eq!(product.package_size, "100g");
        assert_eq!(product.cup_price, Some(2.07));
        assert_eq!(product.cup_measure, "100G");
    }

    fn get_product_result(filename: &str) -> Result<ProductSnapshot> {
        let file = load_file(filename);
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();
        let mut bundle =
            Bundle::from_json_value(json_data).expect("Returned error instead of result");
        assert_eq!(bundle.products.len(), 1);
        let product = bundle.products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        product.try_into_snapshot_and_date(date)
    }

    fn get_product(filename: &str) -> ProductSnapshot {
        get_product_result(filename).expect("Expected conversion to succeed")
    }

    #[test]
    fn test_load_normal() {
        let product = get_product("categories/one-product.json");
        assert_eq!(product.id(), 123);
        assert_eq!(product.name(), "product name");
        assert_eq!(product.description(), "some long product description");
        assert_eq!(product.price(), 12.02.into());
        // todo: date?
        assert!(!product.is_weighted());
    }

    #[test]
    fn test_missing_price() {
        let err = get_product_result("categories/missing-price.json").unwrap_err();
        match err {
            Error::ProductConversion(msg) => assert_eq!(msg, "Missing price on product name"),
            _ => panic!("unexpected type err type"),
        }
    }

    #[test]
    fn test_std_quantity() {
        let product = get_product("categories/unit-from-cup.json");
        assert_eq!(product.unit(), Unit::Grams);
        assert_eq!(product.quantity(), 560.0);
    }

    impl Default for BundleProduct {
        fn default() -> Self {
            Self {
                stockcode: 1,
                name: String::from("product name"),
                description: String::from("product description"),
                price: Some(1.0),
                package_size: String::from("Each"),
                cup_price: Some(1.0),
                cup_measure: String::from("100g"),
                unit: String::from("Each"),
            }
        }
    }

    #[test]
    fn test_low_quantity_error() {
        let product = BundleProduct {
            cup_price: Some(1.0),
            cup_measure: String::from("1g"),
            unit: String::from("G"),
            ..Default::default()
        };
        let err = product.get_quantity_and_unit(1.0).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Conversion error: Low quantity for conversion"
        );
    }
}

use itertools::{Either, Itertools};
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::fmt::Display;
use std::io::Read;
use std::result::Result as StdResult;
use tar::Archive;
use time::Date;

use crate::errors::{Error, Result};
use crate::product::{Price, ProductInfo, ProductSnapshot};
use crate::stores::Store;
use crate::unit::{parse_str_unit, Unit};

use super::category::CategoryResponse;

// If more than 5% of conversions fail then it should be an error
const CONVERSION_SUCCESS_THRESHOLD: f64 = 0.05;

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
            return Err(Error::ProductConversion(format!(
                "Low quantity for conversion"
            )));
        }
        Ok((quantity, unit))
    }
}

#[derive(Deserialize, Debug)]
pub struct Bundle {
    #[serde(rename = "Products")]
    products: Vec<BundleProduct>,
}

impl Bundle {
    pub fn from_json_value(value: serde_json::Value) -> Result<Bundle> {
        // let bundle = serde_json::from_value(value)?;
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

#[derive(Deserialize)]
struct LegacyCategory {
    #[serde(rename = "Products")]
    products: Option<Vec<serde_json::Value>>,
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

pub fn load_from_legacy(file: impl Read, date: Date) -> Result<Vec<ProductSnapshot>> {
    let conversion_results = BundleConversion::from_legacy_reader(file)?;
    let success = conversion_results.validate_conversion(None, date)?;
    Ok(success)
}

pub fn load_from_archive(archive: Archive<impl Read>, date: Date) -> Result<Vec<ProductSnapshot>> {
    let conversion_results = BundleConversion::from_archive(archive)?;
    let success = conversion_results.validate_conversion(None, date)?;
    Ok(success)
}

struct BundleConversion {
    success: Vec<BundleProduct>,
    failure: Vec<Error>,
}

type ProductList = Vec<serde_json::Value>;

impl BundleConversion {
    fn from_legacy_reader(file: impl Read) -> Result<Self> {
        let json_data: Vec<LegacyCategory> = serde_json::from_reader(file)?;
        let json_data: Vec<ProductList> = json_data
            .into_iter()
            .map(|c| c.products.unwrap_or_default())
            .collect();
        let conversion_results = Self::full_product_list(json_data);

        Ok(conversion_results)
    }

    fn from_archive(mut archive: Archive<impl Read>) -> Result<Self> {
        let json_data: Vec<ProductList> = archive
            .entries()?
            .filter_map_ok(|entry| match entry.size() {
                0 => None,
                _ => Some(CategoryResponse::from_reader(entry).map(|r| r.bundles)),
            })
            .flatten()
            .collect::<anyhow::Result<Vec<_>>>()?;
        let conversion_result_all = Self::full_product_list(json_data);
        Ok(conversion_result_all)
    }

    fn full_product_list(json_data: Vec<ProductList>) -> Self {
        let data: Vec<Self> = json_data.into_iter().map(Self::from_json_vec).collect();

        let mut conversion_results = Self {
            success: Vec::new(),
            failure: Vec::new(),
        };
        for item in data {
            conversion_results.success.extend(item.success);
            conversion_results.failure.extend(item.failure);
        }
        conversion_results
    }

    fn from_json_vec(products: Vec<serde_json::Value>) -> Self {
        let (success, failure): (Vec<_>, Vec<_>) =
            products
                .into_iter()
                .partition_map(|v| match Bundle::from_json_value(v) {
                    Ok(v) => match v.products.len() {
                        1 => Either::Left(v.products.into_iter().next().unwrap()),
                        _ => Either::Right(Error::ProductConversion(format!(
                            "Invalid number of products in bundle: {}",
                            v.products.len()
                        ))),
                    },
                    Err(v) => Either::Right(v),
                });
        BundleConversion { success, failure }
    }

    pub fn validate_conversion(
        self,
        success_threshold: Option<f64>,
        date: Date,
    ) -> Result<Vec<ProductSnapshot>> {
        let legacy_success = self.success.len();
        let legacy_failure = self.failure.len();
        let products: Vec<StdResult<ProductSnapshot, Error>> = self
            .success
            .into_iter()
            .map(|s| s.try_into_snapshot_and_date(date))
            .collect();
        let (success, failure): (Vec<_>, Vec<_>) =
            products.into_iter().partition_map(|v| match v {
                Ok(v) => Either::Left(v),
                Err(v) => Either::Right(v),
            });

        for fail in self.failure.iter() {
            debug!("{fail:?}");
        }
        let metrics = ConversionMetrics {
            success: legacy_success,
            fail_search_result: legacy_failure,
            fail_product: failure.len(),
        };

        // Use default if none given
        let success_threshold = success_threshold.unwrap_or(CONVERSION_SUCCESS_THRESHOLD);

        if metrics.failure_rate() > success_threshold {
            error!(
                "Conversion exceeds threshold of {}: {}",
                success_threshold, metrics
            );
            return Err(Error::ProductConversion(format!(
                "Error threshold of {success_threshold} for conversion exceeded: {metrics}",
            )));
        }
        info!("Conversion succeeded: {}", metrics);
        Ok(success)
    }
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
        assert_eq!(product.cup_price, 2.07);
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
                cup_price: 1.0,
                cup_measure: String::from("100g"),
            }
        }
    }

    #[test]
    fn test_low_quantity_error() {
        let product = BundleProduct {
            cup_price: 1.0,
            cup_measure: String::from("1g"),
            ..Default::default()
        };
        let err = product.get_quantity_and_unit(1.0).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Conversion error: Low quantity for conversion"
        );
    }
}

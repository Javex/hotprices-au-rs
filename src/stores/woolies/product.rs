use anyhow::anyhow;
use log::{debug, warn};
use serde::Deserialize;
use std::io::Read;
use time::Date;

use crate::conversion::{self, Product};
use crate::errors::{Error, Result};
use crate::product::{Price, ProductInfo, ProductSnapshot};
use crate::stores::Store;
use crate::unit::{parse_str_unit, Unit};

use super::category::Category;

#[derive(Deserialize, Debug)]
pub(crate) struct BundleProduct {
    #[serde(rename = "Stockcode")]
    stockcode: i64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Description")]
    description: String,
    #[serde(rename = "Price")]
    price: Option<f64>,
    #[serde(rename = "WasPrice")]
    was_price: f64,
    #[serde(rename = "IsInStock")]
    is_in_stock: bool,
    #[serde(rename = "PackageSize")]
    package_size: String,
    #[serde(rename = "CupPrice")]
    cup_price: Option<f64>,
    #[serde(rename = "CupMeasure")]
    cup_measure: Option<String>,
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
        let (std_quantity, unit) = match self.cup_measure {
            Some(ref cup_measure) => match parse_str_unit(cup_measure) {
                Ok((q, u)) => (q, u),
                Err(e) => {
                    debug!("Error converting {self:?} due to parsing error {e}");
                    return Err(e.into());
                }
            },
            None => {
                return Err(anyhow!("Missing CupMeasure, ran out of options to convert").into());
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
        if quantity < 10.0 {
            warn!("Low quantity of {quantity} during conversion of {self:?}");
            return Err(Error::ProductConversion(String::from(
                "Low quantity for conversion",
            )));
        }
        Ok((quantity, unit))
    }
}

impl Product for BundleProduct {
    fn store() -> Store {
        Store::Woolies
    }

    fn try_into_snapshot_and_date(self, date: Date) -> Result<ProductSnapshot> {
        let price = match self.price {
            Some(price) => price,
            None => {
                if !self.is_in_stock && self.was_price > 0.0 {
                    self.was_price
                } else {
                    return Err(Error::ProductConversion(format!(
                        "Missing price on {}",
                        self.name
                    )));
                }
            }
        };

        let (quantity, unit) = match self.cup_measure {
            Some(ref cup_measure) if cup_measure == "1EA" => (1.0, Unit::Each),
            _ => self.get_quantity_and_unit(price)?,
        };

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
pub(crate) struct Bundle {
    #[serde(rename = "Products")]
    pub(crate) products: Vec<BundleProduct>,
}

pub(crate) fn load_snapshot(file: impl Read, date: Date) -> Result<Vec<ProductSnapshot>> {
    let success = conversion::from_reader::<Category, BundleProduct>(file, date)?;
    Ok(success)
}

#[cfg(test)]
mod test {
    use core::panic;

    use serde_json::json;
    use time::Month;

    use super::*;

    #[test]
    fn test_load_product() {
        let json_data = json!(
            {
              "Products": [
                {
                  "Stockcode": 123,
                  "CupPrice": 2.07,
                  "CupMeasure": "100G",
                  "Price": 12.02,
                  "WasPrice": 12.02,
                  "IsInStock": true,
                  "Name": "product name",
                  "Description": "some long product description",
                  "Unit": "Each",
                  "PackageSize": "100g"
                }
              ]
            }
        );
        let json_data: serde_json::Value = serde_json::from_value(json_data).unwrap();

        let bundle =
            serde_json::from_value::<Bundle>(json_data).expect("Returned error instead of result");
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
        assert_eq!(product.cup_measure, Some(String::from("100G")));
    }

    #[test]
    fn test_load_normal() {
        let json_data = json!(
            {
              "Products": [
                {
                  "Stockcode": 123,
                  "CupPrice": 2.07,
                  "CupMeasure": "100G",
                  "Price": 12.02,
                  "WasPrice": 12.02,
                  "IsInStock": true,
                  "Name": "product name",
                  "Description": "some long product description",
                  "Unit": "Each",
                  "PackageSize": "100g"
                }
              ]
            }
        );
        let json_data: serde_json::Value = serde_json::from_value(json_data).unwrap();
        let mut bundle =
            serde_json::from_value::<Bundle>(json_data).expect("Returned error instead of result");
        assert_eq!(bundle.products.len(), 1);
        let product = bundle.products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let product = product
            .try_into_snapshot_and_date(date)
            .expect("Expected conversion to succeed");
        assert_eq!(product.id(), 123);
        assert_eq!(product.name(), "product name");
        assert_eq!(product.description(), "some long product description");
        assert_eq!(product.price(), 12.02.into());
        // todo: date?
        assert!(!product.is_weighted());
    }

    #[test]
    fn test_missing_price() {
        let json_data = json!(
            {
              "Products": [
                {
                  "Stockcode": 123,
                  "CupPrice": 2.07,
                  "CupMeasure": "100G",
                  "Price": null,
                  "WasPrice": 12.02,
                  "IsInStock": true,
                  "Name": "product name",
                  "Description": "some long product description",
                  "Unit": "Each",
                  "PackageSize": "100g"
                }
              ]
            }
        );
        let mut bundle =
            serde_json::from_value::<Bundle>(json_data).expect("Returned error instead of result");
        assert_eq!(bundle.products.len(), 1);
        let product = bundle.products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let err = product.try_into_snapshot_and_date(date).unwrap_err();
        match err {
            Error::ProductConversion(msg) => assert_eq!(msg, "Missing price on product name"),
            _ => panic!("unexpected type err type"),
        }
    }

    #[test]
    fn test_std_quantity() {
        let json_data = json!(
            {
              "Products": [
                {
                  "Stockcode": 124,
                  "CupPrice": 2.68,
                  "CupMeasure": "100G",
                  "Price": 15,
                  "WasPrice": 12.02,
                  "IsInStock": true,
                  "Name": "product name",
                  "Description": "some long product description",
                  "Unit": "Each",
                  "PackageSize": "8x70g"
                }
              ]
            }
        );
        let json_data: serde_json::Value = serde_json::from_value(json_data).unwrap();
        let mut bundle =
            serde_json::from_value::<Bundle>(json_data).expect("Returned error instead of result");
        assert_eq!(bundle.products.len(), 1);
        let product = bundle.products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let product = product
            .try_into_snapshot_and_date(date)
            .expect("Expected conversion to succeed");
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
                was_price: 1.0,
                is_in_stock: true,
                package_size: String::from("Each"),
                cup_price: Some(1.0),
                cup_measure: Some(String::from("100g")),
                unit: String::from("Each"),
            }
        }
    }

    #[test]
    fn test_low_quantity_error() {
        let product = BundleProduct {
            cup_price: Some(1.0),
            cup_measure: Some(String::from("1g")),
            unit: String::from("G"),
            ..Default::default()
        };
        let err = product.get_quantity_and_unit(1.0).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Conversion error: Low quantity for conversion"
        );
    }

    #[test]
    fn test_was_price() {
        let product = BundleProduct {
            price: None,
            was_price: 1.0,
            is_in_stock: false,
            ..Default::default()
        };

        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();

        let product = product.try_into_snapshot_and_date(date).unwrap();
        assert_eq!(product.price(), 1.0.into());
    }
}

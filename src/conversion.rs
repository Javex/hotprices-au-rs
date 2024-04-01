use anyhow::Context;
use itertools::{Either, Itertools};
use log::{error, info};
use serde::Deserialize;
use std::fmt::Display;
use std::io::Read;
use std::result::Result as StdResult;
use time::Date;

use crate::{
    errors::{Error, Result},
    product::ProductSnapshot,
    stores::Store,
};

// If more than 5% of conversions fail then it should be an error
const CONVERSION_SUCCESS_THRESHOLD: f64 = 0.05;

struct ConversionMetrics {
    success: usize,
    failure: usize,
}

impl ConversionMetrics {
    pub(crate) fn failure_rate(&self) -> f64 {
        (self.failure) as f64 / (self.success + self.failure) as f64
    }
}

impl Display for ConversionMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Success: {}, Fail Product: {}, Failure rate: {:.2}%",
            self.success,
            self.failure,
            self.failure_rate() * 100.0,
        )
    }
}

pub(crate) trait Category<T> {
    fn is_filtered(&self) -> bool;
    fn into_products(self) -> anyhow::Result<Vec<T>>;
}

pub(crate) trait Product {
    fn try_into_snapshot_and_date(self, date: Date) -> Result<ProductSnapshot>;
    fn store() -> Store;
}

pub(crate) fn from_reader<C, T>(file: impl Read, date: Date) -> anyhow::Result<Vec<ProductSnapshot>>
where
    C: for<'a> Deserialize<'a> + Category<T>,
    T: Product,
{
    let categories: Vec<C> = serde_json::from_reader(file)?;
    let categories: Vec<C> = categories
        .into_iter()
        .filter(|c| !c.is_filtered())
        .collect();
    let success = convert_all::<C, T>(categories)?;

    let success = convert(success, date)?;
    Ok(success)
}

fn convert_all<C, T>(categories: Vec<C>) -> anyhow::Result<Vec<T>>
where
    C: Category<T>,
    T: Product,
{
    categories
        .into_iter()
        .map(|c| {
            c.into_products()
                .with_context(|| "Failed to convert from json into store-specific product")
        })
        .flatten_ok()
        .collect()
}

fn convert<T>(success: Vec<T>, date: Date) -> Result<Vec<ProductSnapshot>>
where
    T: Product,
{
    let products: Vec<StdResult<ProductSnapshot, Error>> = success
        .into_iter()
        .map(|s| s.try_into_snapshot_and_date(date))
        .collect();
    let (success, failure): (Vec<_>, Vec<_>) = products.into_iter().partition_map(|v| match v {
        Ok(v) => Either::Left(v),
        Err(v) => Either::Right(v),
    });

    let metrics = ConversionMetrics {
        success: success.len(),
        failure: failure.len(),
    };

    // Global default value, currently fixed but could be changed
    let success_threshold = CONVERSION_SUCCESS_THRESHOLD;

    if metrics.failure_rate() > success_threshold {
        error!(
            "Conversion exceeds threshold of {}: {}",
            success_threshold, metrics
        );
        return Err(Error::ProductConversion(format!(
            "Error threshold of {success_threshold} for conversion of {date} exceeded: {metrics}",
        )));
    }
    let store = T::store();
    info!("Conversion of {store}/{date} succeeded: {metrics}");
    Ok(success)
}

#[cfg(test)]
mod test {
    use anyhow::bail;
    use serde_json::json;

    use super::*;

    #[derive(Deserialize)]
    struct TestProduct {}

    impl Product for TestProduct {
        fn store() -> Store {
            Store::Woolies
        }

        fn try_into_snapshot_and_date(self, _: Date) -> Result<ProductSnapshot> {
            Ok(ProductSnapshot::default())
        }
    }

    #[derive(Deserialize)]
    struct TestCategory {
        is_filtered: bool,
        products: Vec<TestProduct>,
        #[serde(default)]
        throw_error: bool,
    }

    impl Category<TestProduct> for TestCategory {
        fn is_filtered(&self) -> bool {
            self.is_filtered
        }
        fn into_products(self) -> anyhow::Result<Vec<TestProduct>> {
            if self.throw_error {
                bail!("")
            } else {
                Ok(self.products)
            }
        }
    }

    #[test]
    fn conversion() {
        let json_data = json!([
            {
                "is_filtered": false,
                "products": [{}],
            }
        ])
        .to_string();
        let date = Date::from_calendar_date(2024, time::Month::January, 1).unwrap();
        let products =
            from_reader::<TestCategory, TestProduct>(json_data.as_bytes(), date).unwrap();
        assert_eq!(products.len(), 1);
    }

    #[test]
    fn conversion_is_filtered() {
        let json_data = json!([
            {
                "is_filtered": false,
                "products": [{}],
            },
            {
                "is_filtered": true,
                "products": [{}],
            }
        ])
        .to_string();
        let date = Date::from_calendar_date(2024, time::Month::January, 1).unwrap();
        let products =
            from_reader::<TestCategory, TestProduct>(json_data.as_bytes(), date).unwrap();
        assert_eq!(products.len(), 1);
    }

    #[test]
    fn conversion_fail_into_products() {
        let categories = vec![TestCategory {
            throw_error: true,
            is_filtered: false,
            products: vec![],
        }];
        let result = convert_all::<TestCategory, TestProduct>(categories);
        assert!(result.is_err());
    }

    #[test]
    fn conversion_fail_into_snapshot() {
        struct TestProduct {}

        impl Product for TestProduct {
            fn store() -> Store {
                Store::Woolies
            }

            fn try_into_snapshot_and_date(self, _: Date) -> Result<ProductSnapshot> {
                Err(Error::ProductConversion(String::new()))
            }
        }

        let success = vec![TestProduct {}];
        let date = Date::from_calendar_date(2024, time::Month::January, 1).unwrap();
        let err = convert(success, date).unwrap_err();
        match err {
            Error::ProductConversion(msg) => assert!(
                msg.contains("Error threshold"),
                "Message did not contain expected value in '{msg}'"
            ),
            _ => panic!("Wrong error type, expected conversion error with threshold message"),
        };
    }
}

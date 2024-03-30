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
};

// If more than 5% of conversions fail then it should be an error
const CONVERSION_SUCCESS_THRESHOLD: f64 = 0.05;

struct ConversionMetrics {
    success: usize,
    fail_search_result: usize,
    fail_product: usize,
}

impl ConversionMetrics {
    pub(crate) fn failure_rate(&self) -> f64 {
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

pub(crate) trait Category<T> {
    fn is_filtered(&self) -> bool;
    fn into_products(self) -> (Vec<T>, Vec<Error>);
}

pub(crate) trait Product {
    fn try_into_snapshot_and_date(self, date: Date) -> Result<ProductSnapshot>;
}

pub(crate) struct Conversion<T> {
    success: Vec<T>,
    failure: Vec<Error>,
}

impl<T> Conversion<T>
where
    T: Product,
{
    pub(crate) fn from_reader<C>(file: impl Read, date: Date) -> Result<Vec<ProductSnapshot>>
    where
        C: for<'a> Deserialize<'a> + Category<T>,
    {
        let categories: Vec<C> = serde_json::from_reader(file)?;
        let categories: Vec<C> = categories
            .into_iter()
            .filter(|c| !c.is_filtered())
            .collect();
        let conversion_results = Self::convert_all::<C>(categories);

        let success = conversion_results.convert(date)?;
        Ok(success)
    }

    fn convert_all<C>(categories: Vec<C>) -> Self
    where
        C: Category<T>,
    {
        let mut conversion_results = Self {
            success: Vec::new(),
            failure: Vec::new(),
        };
        for category in categories {
            let (success, failure) = category.into_products();
            conversion_results.success.extend(success);
            conversion_results.failure.extend(failure);
        }
        conversion_results
    }

    fn convert(self, date: Date) -> Result<Vec<ProductSnapshot>> {
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

        let metrics = ConversionMetrics {
            success: legacy_success,
            fail_search_result: legacy_failure,
            fail_product: failure.len(),
        };

        // Global default value, currently fixed but could be changed
        let success_threshold = CONVERSION_SUCCESS_THRESHOLD;

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
    use serde_json::json;

    use super::*;

    #[derive(Deserialize)]
    struct TestProduct {}

    impl Product for TestProduct {
        fn try_into_snapshot_and_date(self, _: Date) -> Result<ProductSnapshot> {
            Ok(ProductSnapshot::default())
        }
    }

    #[derive(Deserialize)]
    struct TestCategory {
        is_filtered: bool,
        products: Vec<TestProduct>,
        #[serde(default)]
        into_errors: u32,
    }

    impl Category<TestProduct> for TestCategory {
        fn is_filtered(&self) -> bool {
            self.is_filtered
        }
        fn into_products(self) -> (Vec<TestProduct>, Vec<Error>) {
            let errors = (0..self.into_errors).map(|_| Error::ProductConversion(String::new()));
            (self.products, errors.collect())
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
            Conversion::<TestProduct>::from_reader::<TestCategory>(json_data.as_bytes(), date)
                .unwrap();
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
            Conversion::<TestProduct>::from_reader::<TestCategory>(json_data.as_bytes(), date)
                .unwrap();
        assert_eq!(products.len(), 1);
    }

    #[test]
    fn conversion_fail_into_products() {
        let categories = vec![TestCategory {
            into_errors: 1,
            is_filtered: false,
            products: vec![],
        }];
        let conversion = Conversion::<TestProduct>::convert_all::<TestCategory>(categories);
        assert_eq!(conversion.failure.len(), 1);
        assert_eq!(conversion.success.len(), 0);
    }

    #[test]
    fn conversion_fail_into_snapshot() {
        struct TestProduct {}

        impl Product for TestProduct {
            fn try_into_snapshot_and_date(self, _: Date) -> Result<ProductSnapshot> {
                Err(Error::ProductConversion(String::new()))
            }
        }

        let conversion = Conversion {
            success: vec![TestProduct {}],
            failure: vec![],
        };
        let date = Date::from_calendar_date(2024, time::Month::January, 1).unwrap();
        let err = conversion.convert(date).unwrap_err();
        match err {
            Error::ProductConversion(msg) => assert!(
                msg.contains("Error threshold"),
                "Message did not contain expected value in '{msg}'"
            ),
            _ => panic!("Wrong error type, expected conversion error with threshold message"),
        };
    }
}

use std::{fs, path::Path};

use log::debug;
use strum::IntoEnumIterator;
use time::{macros::format_description, Date};

use crate::{
    product::{deduplicate_products, merge_price_history},
    storage::{load_daily_snapshot, load_history, save_result, save_to_site},
    stores::Store,
};

pub enum AnalysisType {
    Day(Date),
    History,
}

impl AnalysisType {
    pub(crate) fn days(
        self,
        output_dir: &Path,
        store_filter: Option<Store>,
    ) -> anyhow::Result<Vec<Date>> {
        match self {
            AnalysisType::History => history_days(output_dir, store_filter),
            AnalysisType::Day(day) => Ok(vec![day]),
        }
    }
}

fn history_days(output_dir: &Path, store_filter: Option<Store>) -> anyhow::Result<Vec<Date>> {
    let mut entries: Vec<Date> = Vec::new();
    for store in Store::iter() {
        if store_filter.is_some_and(|s| s != store) {
            continue;
        }

        for entry in fs::read_dir(output_dir.join(store.to_string()))? {
            let entry = entry.map_err(anyhow::Error::from)?;
            let path = entry.path();
            if !path.is_file() {
                debug!("Skipping path {path:?} since it is not a file");
                continue;
            }
            let file_name = match path.file_name() {
                Some(file_name) => file_name,
                None => {
                    return Err(anyhow::Error::msg(format!(
                        "Path {path:?} is not a file, can't read"
                    )))
                }
            };
            let file_name = file_name.to_str().expect("file path should be valid str");
            let mut splits = file_name.split('.');
            let basename = match splits.next() {
                Some(b) => b,
                None => {
                    return Err(anyhow::Error::msg(format!(
                        "File {file_name:?} can't be split"
                    )));
                }
            };

            let format = format_description!("[year]-[month]-[day]");
            let date = match Date::parse(basename, &format) {
                Ok(date) => date,
                Err(e) => {
                    return Err(anyhow::Error::from(e)
                        .context(format!("Cannot convert {basename:?} to date")));
                }
            };
            debug!("Extracted date {date:?} fromm path {path:?}");
            entries.push(date);
        }
    }
    entries.sort();
    Ok(entries)
}

pub fn do_analysis(
    analysis_type: AnalysisType,
    store: Option<Store>,
    compress: bool,
    output_dir: &Path,
    data_dir: &Path,
) -> anyhow::Result<()> {
    let previous_products = match load_history(output_dir) {
        Ok(products) => products,
        Err(e) => match analysis_type {
            AnalysisType::History => Vec::new(),
            AnalysisType::Day(_) => return Err(e),
        },
    };

    let mut products = previous_products;
    // todo: make this return files instead of dates
    for day in analysis_type.days(output_dir, store)? {
        let new_products = load_daily_snapshot(output_dir, day, store)?;
        let new_products = deduplicate_products(new_products);
        products = merge_price_history(products, new_products, store);
    }
    save_result(&products, output_dir)?;
    save_to_site(&products, data_dir, compress)?;
    Ok(())
}

#[cfg(test)]
mod test_do_analysis {
    use std::{
        fs::{create_dir_all, File},
        io::Write,
        path::Path,
    };

    use flate2::{write::GzEncoder, Compression};
    use serde_json::json;
    use tempfile::tempdir;
    use time::{Date, Month};

    use crate::{analysis::AnalysisType, storage::load_history, stores::Store};

    use super::{do_analysis, history_days};

    fn init() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .is_test(true)
            .try_init();
    }

    fn write_compressed(contents: &[u8], dst: &Path) {
        let dst = File::create(dst).unwrap();
        let mut dst = GzEncoder::new(dst, Compression::default());
        dst.write_all(contents).unwrap();
    }

    #[test]
    fn no_files() {
        let output_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let day = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let store: Option<Store> = Some(Store::Coles);
        let compress = false;
        let result = do_analysis(
            AnalysisType::Day(day),
            store,
            compress,
            output_dir.path(),
            data_dir.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn one_file() {
        init();
        let output_dir = tempdir().unwrap();

        // create an existing history
        let latest_canoncial = json!([
            {
                "id": 1,
                "name": "Product Name",
                "description": "PRODUCT DESCRIPTION",
                "price": 12.0,
                "price_history": [
                  {
                    "date": "2024-01-01",
                    "price": 12.0
                  }
                ],
                "is_weighted": true,
                "unit": "Grams",
                "quantity": 500.0,
                "store": "coles"
            }
        ]);
        write_compressed(
            latest_canoncial.to_string().as_bytes(),
            &output_dir.path().join("latest-canonical.json.gz"),
        );

        // add a new snapshot to it
        let data_dir = tempdir().unwrap();
        let day = Date::from_calendar_date(2024, Month::January, 2).unwrap();
        let snapshot = json!(
            [
              {
                "seoToken": "category-slug",
                "Products": [
                  {
                    "_type": "PRODUCT",
                    "id": 1,
                    "adId": null,
                    "name": "Product name",
                    "brand": "Brand name",
                    "description": "BRAND NAME PRODUCT NAME 150G",
                    "size": "150g",
                    "pricing": {
                      "now": 6.7,
                      "unit": {
                        "isWeighted": false
                      }
                    }
                  }
                ]
              }
            ]
        );
        let store = Store::Coles;
        let dst_dir = output_dir.path().join(store.to_string());
        create_dir_all(&dst_dir).unwrap();
        let dst = dst_dir.join(format!("{day}.json.gz"));
        write_compressed(snapshot.to_string().as_bytes(), &dst);

        let compress = false;
        do_analysis(
            AnalysisType::Day(day),
            Some(store),
            compress,
            output_dir.path(),
            data_dir.path(),
        )
        .expect("analysis should succeed");

        // validate result
        let products = load_history(output_dir.path()).expect("should contain history");
        let products = serde_json::to_value(products).unwrap();
        assert_eq!(
            products,
            json!([
                {
                    "id": 1,
                    "name": "Brand name Product name",
                    "description": "BRAND NAME PRODUCT NAME 150G",
                    "is_weighted": false,
                    "unit": "Grams",
                    "quantity": 150.0,
                    "store": "coles",
                    "price_history": [
                        { "date": "2024-01-02", "price": 6.7 },
                        { "date": "2024-01-01", "price": 12.0 },
                    ]
                }
            ]),
        );
    }

    #[test]
    fn history() {
        init();
        // prepare test folders
        let output_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let store = Store::Coles;
        let dst_dir = output_dir.path().join(store.to_string());
        create_dir_all(&dst_dir).unwrap();

        // add first snapshot
        let day = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let snapshot = json!(
            [
              {
                "seoToken": "category-slug",
                "Products": [
                  {
                    "_type": "PRODUCT",
                    "id": 1,
                    "adId": null,
                    "name": "Product name",
                    "brand": "Brand name",
                    "description": "BRAND NAME PRODUCT NAME 150G",
                    "size": "150g",
                    "pricing": {
                      "now": 6.7,
                      "unit": {
                        "isWeighted": false
                      }
                    }
                  }
                ]
              }
            ]
        );
        let dst = dst_dir.join(format!("{day}.json.gz"));
        write_compressed(snapshot.to_string().as_bytes(), &dst);

        // Add second snapshot
        let day = Date::from_calendar_date(2024, Month::January, 2).unwrap();
        let snapshot = json!(
            [
              {
                "seoToken": "category-slug",
                "Products": [
                  {
                    "_type": "PRODUCT",
                    "id": 1,
                    "adId": null,
                    "name": "Product name",
                    "brand": "Brand name",
                    "description": "BRAND NAME PRODUCT NAME 150G",
                    "size": "150g",
                    "pricing": {
                      "now": 7.8,
                      "unit": {
                        "isWeighted": false
                      }
                    }
                  }
                ]
              }
            ]
        );
        let dst = dst_dir.join(format!("{day}.json.gz"));
        write_compressed(snapshot.to_string().as_bytes(), &dst);
        let compress = false;
        do_analysis(
            AnalysisType::History,
            Some(store),
            compress,
            output_dir.path(),
            data_dir.path(),
        )
        .expect("analysis should succeed");

        // validate result
        let products = load_history(output_dir.path()).expect("should contain history");
        let products = serde_json::to_value(products).unwrap();
        assert_eq!(
            products[0]["price_history"],
            json!([
                {"date": "2024-01-02", "price": 7.8},
                {"date": "2024-01-01", "price": 6.7}
            ])
        );
    }

    #[test]
    fn history_days_skips_folder() {
        init();
        let output_dir = tempdir().unwrap();
        let store = Store::Coles;
        let store_dir = output_dir.path().join(store.to_string());
        let empty_day_dir = store_dir.join("2024-01-01");
        create_dir_all(empty_day_dir).unwrap();
        let days = history_days(output_dir.path(), Some(store))
            .expect("should skip folders and still return results");
        assert_eq!(
            days.len(),
            0,
            "Should have skipped folders and returned empty result but got {days:?}"
        );
    }
}

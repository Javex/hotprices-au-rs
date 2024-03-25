use std::{fs, path::Path};

use log::{debug, error};
use strum::IntoEnumIterator;
use time::{macros::format_description, Date};

use crate::{
    errors::{Error, Result},
    product::{deduplicate_products, merge_price_history},
    storage::{load_daily_snapshot, load_history, save_result, save_to_site},
    stores::Store,
};

pub enum AnalysisType {
    Day(Date),
    History,
}

impl AnalysisType {
    pub fn days(self, output_dir: &Path, store_filter: Option<Store>) -> Result<Vec<Date>> {
        match self {
            AnalysisType::History => history_days(output_dir, store_filter),
            AnalysisType::Day(day) => Ok(vec![day]),
        }
    }
}

fn history_days(output_dir: &Path, store_filter: Option<Store>) -> Result<Vec<Date>> {
    let mut entries: Vec<Date> = Vec::new();
    for store in Store::iter() {
        if store_filter.is_some_and(|s| s != store) {
            continue;
        }

        for entry in fs::read_dir(output_dir.join(store.to_string()))? {
            let entry = entry.map_err(Error::from)?;
            let path = entry.path();
            if !path.is_file() {
                debug!("Skipping path {path:?} since it is not a file");
                continue;
            }
            let file_name = match path.file_name() {
                Some(file_name) => file_name,
                None => {
                    error!("Path {path:?} is not a file, can't read");
                    return Err(Error::Message("not a file".to_string()));
                }
            };
            let file_name = file_name.to_str().expect("file path should be valid str");
            let mut splits = file_name.split('.');
            let basename = match splits.next() {
                Some(b) => b,
                None => {
                    error!("File {file_name:?} can't be split");
                    return Err(Error::Message("missing file extension".to_string()));
                }
            };

            let format = format_description!("[year]-[month]-[day]");
            let date = match Date::parse(basename, &format) {
                Ok(date) => date,
                Err(e) => {
                    error!("Cannot convert {basename:?} to date");
                    return Err(Error::Message(e.to_string()));
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
) -> Result<()> {
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
        io::{Read, Write},
        path::{Path, PathBuf},
    };

    use flate2::{write::GzEncoder, Compression};
    use tempfile::tempdir;
    use time::{Date, Month};

    use crate::{analysis::AnalysisType, storage::load_history, stores::Store, unit::Unit};

    use super::{do_analysis, history_days};

    fn init() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .is_test(true)
            .try_init();
    }

    fn copy_compressed(src: &Path, dst: &Path) {
        let mut src = File::open(src).unwrap();
        let dst = File::create(dst).unwrap();
        let mut dst = GzEncoder::new(dst, Compression::default());

        let mut contents = Vec::new();
        src.read_to_end(&mut contents).unwrap();
        dst.write_all(&contents).unwrap();
    }

    fn setup_latest_canonical(resource: &str, output_dir: &Path) {
        let src = PathBuf::from(format!("resources/test/latest-canonical/{resource}"));
        let dst = output_dir.join("latest-canonical.json.gz");
        copy_compressed(&src, &dst);
    }

    fn setup_snapshot_legacy(resource: &str, output_dir: &Path, day: Date, store: Store) {
        let src = PathBuf::from(format!("resources/test/{resource}"));
        let dst_dir = output_dir.join(store.to_string());
        create_dir_all(&dst_dir).unwrap();
        let dst = dst_dir.join(format!("{day}.json.gz"));
        copy_compressed(&src, &dst);
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
        setup_latest_canonical("one-product.json", output_dir.path());
        let data_dir = tempdir().unwrap();
        let day = Date::from_calendar_date(2024, Month::January, 2).unwrap();
        setup_snapshot_legacy(
            "legacy/coles/one-product.json",
            output_dir.path(),
            day,
            Store::Coles,
        );
        let store: Option<Store> = Some(Store::Coles);
        let compress = false;
        do_analysis(
            AnalysisType::Day(day),
            store,
            compress,
            output_dir.path(),
            data_dir.path(),
        )
        .expect("analysis should succeed");

        // validate result
        let products = load_history(output_dir.path()).expect("should contain history");
        let [ref product] = products[..] else {
            panic!("should contain exactly one product")
        };
        assert_eq!(product.id(), 1);
        assert_eq!(product.name(), "Brand name Product name");
        assert_eq!(product.description(), "BRAND NAME PRODUCT NAME 150G");
        assert_eq!(product.unit(), Unit::Grams);
        assert_eq!(product.quantity(), 150.0);
        let price_history = product.price_history();
        assert_eq!(price_history.len(), 2);
        let new_price = price_history.first();
        let old_price = price_history.get(1).unwrap();
        assert_eq!(new_price.price, 6.7.into());
        assert_eq!(
            new_price.date,
            Date::from_calendar_date(2024, Month::January, 2).unwrap()
        );
        assert_eq!(old_price.price, 12.0.into());
        assert_eq!(
            old_price.date,
            Date::from_calendar_date(2024, Month::January, 1).unwrap()
        );
    }

    #[test]
    fn history() {
        init();
        let output_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let store = Store::Coles;
        setup_snapshot_legacy(
            "legacy/coles/one-product.json",
            output_dir.path(),
            Date::from_calendar_date(2024, Month::January, 1).unwrap(),
            store,
        );
        setup_snapshot_legacy(
            "legacy/coles/one-product-new-price.json",
            output_dir.path(),
            Date::from_calendar_date(2024, Month::January, 2).unwrap(),
            store,
        );
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
        let [ref product] = products[..] else {
            panic!("should contain exactly one product")
        };
        let price_history = product.price_history();
        assert_eq!(price_history.len(), 2);
        let new_price = price_history.first();
        let old_price = price_history.get(1).unwrap();
        assert_eq!(new_price.price, 7.8.into());
        assert_eq!(
            new_price.date,
            Date::from_calendar_date(2024, Month::January, 2).unwrap()
        );
        assert_eq!(old_price.price, 6.7.into());
        assert_eq!(
            old_price.date,
            Date::from_calendar_date(2024, Month::January, 1).unwrap()
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

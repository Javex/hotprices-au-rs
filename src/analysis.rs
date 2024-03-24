use std::path::Path;

use time::Date;

use crate::{
    errors::Result,
    product::{deduplicate_products, merge_price_history},
    storage::{load_daily_snapshot, load_history, save_result, save_to_site},
    stores::Store,
};

pub fn do_analysis(
    day: Date,
    store: Option<Store>,
    compress: bool,
    history: bool,
    output_dir: &Path,
    data_dir: &Path,
) -> Result<()> {
    assert!(!history, "history backfill");
    let previous_products = load_history(output_dir)?;
    let new_products = load_daily_snapshot(output_dir, day, store)?;
    let new_products = deduplicate_products(new_products);
    let products = merge_price_history(previous_products, new_products, store);
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

    use crate::{storage::load_history, stores::Store, unit::Unit};

    use super::do_analysis;

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
        let history = false;
        let result = do_analysis(
            day,
            store,
            compress,
            history,
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
        let history = false;
        do_analysis(
            day,
            store,
            compress,
            history,
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
}

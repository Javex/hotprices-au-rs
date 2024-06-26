use anyhow::Context;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use log::{debug, info};
use std::fs::{self};
use std::path::PathBuf;
use std::{
    fs::{create_dir_all, File},
    io::{BufReader, BufWriter, Write},
    path::Path,
};
use strum::IntoEnumIterator;
use time::Date;

use crate::product::{ProductHistory, ProductSnapshot};
use crate::stores::{coles, woolies, Store};

pub(crate) fn remove(source: &Path) -> anyhow::Result<()> {
    info!("Removing cache directory {}", source.to_string_lossy());
    fs::remove_dir_all(source)
        .with_context(|| format!("Failed to remove cache folder {}", source.to_string_lossy()))
}

pub(crate) fn get_snapshot_path(output_dir: &Path, store: Store, day: Date) -> PathBuf {
    let mut path = PathBuf::from(output_dir);
    path.push(store.to_string());
    path.push(format!("{day}.json.gz"));
    path
}

pub(crate) fn save_fetch_data(data: String, snapshot_path: &Path) -> anyhow::Result<()> {
    let snapshot_dir = snapshot_path.parent().with_context(|| {
        format!(
            "Snapshot path {} does not contain parent",
            snapshot_path.to_string_lossy()
        )
    })?;
    create_dir_all(snapshot_dir).with_context(|| {
        format!(
            "Failed to create snapshot dir at {}",
            snapshot_dir.to_string_lossy()
        )
    })?;
    let file = File::create(snapshot_path).with_context(|| {
        format!(
            "Failed to save fetched data to {}",
            snapshot_path.to_string_lossy()
        )
    })?;
    let mut file = GzEncoder::new(file, Compression::default());
    file.write_all(data.as_bytes()).with_context(|| {
        format!(
            "Failed to write all bytes of fetched data to {}",
            snapshot_path.to_string_lossy()
        )
    })?;
    Ok(())
}

pub(crate) fn load_history(output_dir: &Path) -> anyhow::Result<Vec<ProductHistory>> {
    let file = output_dir.join("latest-canonical.json.gz");
    let fpath = file.to_string_lossy();
    let file = File::open(&file).with_context(|| format!("Failed to open history file {fpath}"))?;
    let file = GzDecoder::new(file);
    let file = BufReader::new(file);
    let products: Vec<ProductHistory> = serde_json::from_reader(file)
        .with_context(|| format!("Failed to load history from {fpath}"))?;
    debug!("Loaded {} products from history", products.len());
    Ok(products)
}

pub(crate) fn load_daily_snapshot(
    output_dir: &Path,
    day: Date,
    store_filter: Option<Store>,
) -> anyhow::Result<Vec<ProductSnapshot>> {
    let mut products = Vec::new();
    for store in Store::iter() {
        if store_filter.is_some_and(|s| s != store) {
            continue;
        }
        let file = output_dir
            .join(store.to_string())
            .join(format!("{day}.json.gz"));
        debug!("Loading {}", file.to_string_lossy());
        let file = File::open(&file).context(format!(
            "Failed to open daily snapshot {}",
            file.to_string_lossy()
        ))?;
        let file = GzDecoder::new(file);
        let file = BufReader::new(file);
        let store_products = match store {
            Store::Coles => coles::load_snapshot(file, day)
                .context("Failed to load coles data from snapshot")?,
            Store::Woolies => woolies::load_snapshot(file, day)
                .context("Failed to load woolies data from snapshot")?,
        };
        products.extend(store_products);
    }
    debug!("Loaded {} products for date {:?}", products.len(), day);
    Ok(products)
}

pub(crate) fn save_result(products: &Vec<ProductHistory>, output_dir: &Path) -> anyhow::Result<()> {
    let file = output_dir.join("latest-canonical.json.gz");
    let file = File::create(file)?;
    let file = GzEncoder::new(file, Compression::default());
    let file = BufWriter::new(file);
    serde_json::to_writer(file, products)?;
    Ok(())
}

pub(crate) fn save_to_site(
    products: &[ProductHistory],
    data_dir: &Path,
    compress: bool,
) -> anyhow::Result<()> {
    // create the data_dir if it doesn't exist yet
    create_dir_all(data_dir)?;

    let filename_suffix = if compress { ".gz" } else { "" };

    for store in Store::iter() {
        let file = data_dir.join(format!(
            "latest-canonical.{store}.compressed.json{filename_suffix}"
        ));
        let file = File::create(file)?;
        let file: Box<dyn Write> = if compress {
            Box::new(GzEncoder::new(file, Compression::default()))
        } else {
            Box::new(file)
        };
        let file = BufWriter::new(file);
        let store_products: Vec<&ProductHistory> =
            products.iter().filter(|p| p.store() == store).collect();
        serde_json::to_writer(file, &store_products)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::fs::File;

    use tempfile::tempdir;

    use super::save_to_site;
    use crate::{
        product::{ProductHistory, ProductInfo},
        stores::Store,
    };

    #[test]
    fn test_save_to_site_compressed() {
        let products = vec![ProductHistory::default()];
        let tmpdir = tempdir().unwrap();
        let tmppath = tmpdir.path();
        save_to_site(&products, tmppath, true).unwrap();

        // check that a file is there
        assert!(tmppath
            .join("latest-canonical.coles.compressed.json.gz")
            .exists());
        assert!(tmppath
            .join("latest-canonical.woolies.compressed.json.gz")
            .exists());
    }

    #[test]
    fn test_save_to_site_uncompressed() {
        let products = vec![
            ProductHistory::default(),
            ProductHistory::with_info(ProductInfo::with_store(Store::Woolies)),
        ];
        let tmpdir = tempdir().unwrap();
        let tmppath = tmpdir.path();
        save_to_site(&products, tmppath, false).unwrap();

        // check that a file is there
        assert!(tmppath
            .join("latest-canonical.coles.compressed.json")
            .exists());

        // check that a file is there
        assert!(tmppath
            .join("latest-canonical.woolies.compressed.json")
            .exists());

        // load products from file and confirm that each store only has their own
        let file = tmppath.join("latest-canonical.coles.compressed.json");
        let file = File::open(file).unwrap();
        let products: Vec<ProductHistory> = serde_json::from_reader(file).unwrap();
        for product in products {
            assert_eq!(
                product.store(),
                Store::Coles,
                "Unexpected store for {:?}",
                product
            );
        }
        let file = tmppath.join("latest-canonical.woolies.compressed.json");
        let file = File::open(file).unwrap();
        let products: Vec<ProductHistory> = serde_json::from_reader(file).unwrap();
        for product in products {
            assert_eq!(
                product.store(),
                Store::Woolies,
                "Unexpected store for {:?}",
                product
            );
        }
    }
}

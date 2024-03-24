use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use log::{debug, info};
use std::fs::{self};
use std::io;
use std::path::PathBuf;
use std::{
    fs::{create_dir_all, File},
    io::{BufReader, BufWriter, Write},
    path::Path,
};
use strum::IntoEnumIterator;
use tar::Archive;
use time::Date;

use crate::errors::{Error, Result};
use crate::product::{ProductHistory, ProductSnapshot};
use crate::stores::{coles, Store};

pub fn compress(source: &PathBuf) -> Result<()> {
    let mut file = source.clone();
    file.set_extension("tar.gz");
    info!(
        "Saving results as {}",
        file.to_str().expect("File should be valid UTF-8 str")
    );
    let file = File::create(file)?;
    let file = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(file);
    // saves everything relative to source
    archive.append_dir_all(
        source
            .file_name()
            .ok_or(Error::Message("Bad file name".to_string()))?,
        source,
    )?;
    archive.finish()?;
    Ok(())
}

pub fn remove(source: &Path) -> io::Result<()> {
    info!("Removing cache directory {}", source.to_str().unwrap());
    fs::remove_dir_all(source)
}

pub fn load_history(output_dir: &Path) -> Result<Vec<ProductHistory>> {
    let file = output_dir.join("latest-canonical.json.gz");
    let file = File::open(file)?;
    let file = GzDecoder::new(file);
    let file = BufReader::new(file);
    let products: Vec<ProductHistory> = serde_json::from_reader(file)?;
    debug!("Loaded {} products from history", products.len());
    Ok(products)
}

pub fn load_daily_snapshot(
    output_dir: &Path,
    day: Date,
    store_filter: Option<Store>,
) -> Result<Vec<ProductSnapshot>> {
    let mut products = Vec::new();
    for store in Store::iter() {
        if store_filter.is_some_and(|s| s != store) {
            continue;
        }
        let file = output_dir
            .join(store.to_string())
            .join(format!("{day}.json.gz"));
        let store_products = if file.exists() {
            // legacy file
            debug!("Loading {}", file.to_str().expect("should be valid str"));
            let file = File::open(file)?;
            let file = GzDecoder::new(file);
            let file = BufReader::new(file);
            match store {
                Store::Coles => coles::product::load_from_legacy(file, day)?,
                Store::Woolies => todo!("load_from_legacy for woolies"),
            }
        } else {
            // non legacy format
            let file = output_dir
                .join(store.to_string())
                .join(format!("{day}.tar.gz"));
            debug!("Loading {}", file.to_str().expect("should be valid str"));
            let file = File::open(file)?;
            let file = GzDecoder::new(file);
            let file = BufReader::new(file);
            let file = Archive::new(file);
            match store {
                Store::Coles => coles::product::load_from_archive(file, day)?,
                Store::Woolies => todo!("load_from_archive"),
            }
        };
        products.extend(store_products);
    }
    debug!("Loaded {} products for date {:?}", products.len(), day);
    Ok(products)
}

pub fn save_result(products: &Vec<ProductHistory>, output_dir: &Path) -> Result<()> {
    let file = output_dir.join("latest-canonical.json.gz");
    let file = File::create(file)?;
    let file = GzEncoder::new(file, Compression::default());
    let file = BufWriter::new(file);
    serde_json::to_writer(file, products)?;
    Ok(())
}

pub fn save_to_site(products: &[ProductHistory], data_dir: &Path, compress: bool) -> Result<()> {
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

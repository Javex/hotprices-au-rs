use std::{
    collections::{HashMap, HashSet},
    fs::{create_dir_all, File},
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use log::{debug, info};
use strum::IntoEnumIterator;
use tar::Archive;
use time::Date;

use crate::{
    errors::Result,
    product::{merge_price_history, Product},
    stores::{coles, Store},
};

pub fn do_analysis(
    day: Date,
    store: Option<Store>,
    compress: bool,
    history: bool,
    output_dir: &Path,
    data_dir: &Path,
) -> Result<()> {
    if history {
        panic!("history backfill");
    }
    let previous_products = load_history(output_dir)?;
    let new_products = load_daily_snapshot(output_dir, day, store)?;
    let new_products = deduplicate_products(new_products);
    let products = merge_price_history(previous_products, new_products, store)?;
    save_result(&products, output_dir)?;
    save_to_site(&products, data_dir, compress)?;
    Ok(())
}

fn load_history(output_dir: &Path) -> Result<Vec<Product>> {
    let file = output_dir.join("latest-canonical.json.gz");
    let file = File::open(file)?;
    let file = GzDecoder::new(file);
    let file = BufReader::new(file);
    let products: Vec<Product> = serde_json::from_reader(file)?;
    debug!("Loaded {} products from history", products.len());
    Ok(products)
}

fn load_daily_snapshot(
    output_dir: &Path,
    day: Date,
    store_filter: Option<Store>,
) -> Result<Vec<Product>> {
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
            let file = File::open(file)?;
            let file = GzDecoder::new(file);
            let file = BufReader::new(file);
            match store {
                Store::Coles => coles::product::load_from_legacy(file)?,
                Store::Woolies => todo!("load_from_legacy for woolies"),
            }
        } else {
            // non legacy format
            let file = output_dir
                .join(store.to_string())
                .join(format!("{day}.tar.gz"));
            let file = File::open(file)?;
            let file = GzDecoder::new(file);
            let file = BufReader::new(file);
            let file = Archive::new(file);
            match store {
                Store::Coles => coles::product::load_from_archive(file)?,
                Store::Woolies => todo!("load_from_archive"),
            }
        };
        products.extend(store_products);
    }
    debug!("Loaded {} products for date {:?}", products.len(), day);
    Ok(products)
}

fn save_result(products: &Vec<Product>, output_dir: &Path) -> Result<()> {
    let file = output_dir.join("latest-canonical.json.gz");
    let file = File::create(file)?;
    let file = GzEncoder::new(file, Compression::default());
    let file = BufWriter::new(file);
    serde_json::to_writer(file, products)?;
    Ok(())
}

fn save_to_site(products: &[Product], data_dir: &Path, compress: bool) -> Result<()> {
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
        let store_products: Vec<&Product> = products.iter().filter(|p| p.store == store).collect();
        serde_json::to_writer(file, &store_products)?;
    }
    Ok(())
}

fn deduplicate_products(products: Vec<Product>) -> Vec<Product> {
    let mut lookup = HashSet::new();
    let mut dedup_products = Vec::new();
    let mut duplicates = HashMap::new();
    for product in products {
        let product_key = (product.store, product.id);
        if lookup.contains(&product_key) {
            *duplicates.entry(product.store).or_insert(0) += 1;
        } else {
            lookup.insert(product_key);
            dedup_products.push(product);
        }
    }

    if !duplicates.is_empty() {
        info!("Deduplicated products: {:?}", duplicates);
    }
    dedup_products
}

#[cfg(test)]
mod test {
    use std::fs::File;

    use tempfile::tempdir;

    use super::{deduplicate_products, save_to_site};
    use crate::{product::Product, stores::Store};

    #[test]
    fn test_deduplicate() {
        let products = vec![Product::default(), Product::default()];
        assert_eq!(deduplicate_products(products).len(), 1);
    }

    #[test]
    fn test_save_to_site_compressed() {
        let products = vec![Product::default()];
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
            Product::default(),
            Product {
                store: crate::stores::Store::Woolies,
                ..Default::default()
            },
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
        let products: Vec<Product> = serde_json::from_reader(file).unwrap();
        for product in products {
            assert_eq!(
                product.store,
                Store::Coles,
                "Unexpected store for {:?}",
                product
            );
        }
        let file = tmppath.join("latest-canonical.woolies.compressed.json");
        let file = File::open(file).unwrap();
        let products: Vec<Product> = serde_json::from_reader(file).unwrap();
        for product in products {
            assert_eq!(
                product.store,
                Store::Woolies,
                "Unexpected store for {:?}",
                product
            );
        }
    }
}

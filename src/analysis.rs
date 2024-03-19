use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::BufReader,
    path::PathBuf,
};

use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use log::info;
use tar::Archive;
use time::Date;

use crate::{
    errors::Result,
    product::Product,
    stores::{
        coles::product::{load_from_archive, load_from_legacy},
        Store,
    },
};

pub fn do_analysis(
    day: Date,
    store: Store,
    compress: bool,
    history: bool,
    output_dir: PathBuf,
) -> Result<()> {
    if history || compress {
        panic!("not implemented");
    }

    let file = output_dir
        .join(store.to_string())
        .join(format!("{day}.json.gz"));
    let products = if file.exists() {
        // legacy file
        let file = File::open(file)?;
        let file = GzDecoder::new(file);
        let file = BufReader::new(file);
        load_from_legacy(file)?
    } else {
        // non legacy format
        let file = output_dir
            .join(store.to_string())
            .join(format!("{day}.tar.gz"));
        let file = File::open(file)?;
        let file = GzDecoder::new(file);
        let file = BufReader::new(file);
        let file = Archive::new(file);
        load_from_archive(file)?
    };
    let products = deduplicate_products(products);
    save_result(products, output_dir)?;
    Ok(())
}

fn save_result(products: Vec<Product>, output_dir: PathBuf) -> Result<()> {
    let file = output_dir.join("latest-canonical.json.gz");
    let file = File::create(file)?;
    let file = GzEncoder::new(file, Compression::default());
    serde_json::to_writer(file, &products)?;
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
    use super::deduplicate_products;
    use crate::product::Product;

    #[test]
    fn test_deduplicate() {
        let products = vec![Product::default(), Product::default()];
        assert_eq!(deduplicate_products(products).len(), 1);
    }
}

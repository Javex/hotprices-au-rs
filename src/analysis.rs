use std::{fs::File, io::BufReader, path::PathBuf};

use flate2::read::GzDecoder;
use tar::Archive;
use time::Date;

use crate::{
    errors::Result,
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
    if file.exists() {
        // legacy file
        let file = File::open(file)?;
        let file = GzDecoder::new(file);
        let file = BufReader::new(file);
        load_from_legacy(file)?;
    } else {
        // non legacy format
        let file = output_dir
            .join(store.to_string())
            .join(format!("{day}.tar.gz"));
        let file = File::open(file)?;
        let file = GzDecoder::new(file);
        let file = BufReader::new(file);
        let file = Archive::new(file);
        load_from_archive(file)?;
    }
    Ok(())
}
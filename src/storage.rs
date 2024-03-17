use flate2::write::GzEncoder;
use flate2::Compression;
use log::info;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::errors::{Error, Result};

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

use flate2::write::GzEncoder;
use flate2::Compression;
use log::info;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

pub fn compress(source: &PathBuf) {
    let mut file = source.clone();
    file.set_extension("tar.gz");
    info!("Saving results as {}", file.to_str().unwrap());
    let file = File::create(file).unwrap();
    let file = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(file);
    // saves everything relative to source
    archive.append_dir_all("", source).unwrap();
    archive.finish().unwrap();
}

pub fn remove(source: &Path) -> io::Result<()> {
    info!("Removing cache directory {}", source.to_str().unwrap());
    fs::remove_dir_all(source)
}

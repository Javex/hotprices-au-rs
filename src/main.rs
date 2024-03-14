use flate2::read::GzDecoder;
use hotprices_au_rs::cache::FsCache;
use hotprices_au_rs::stores::coles::category::load_from_legacy;
use hotprices_au_rs::stores::coles::fetch;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::PathBuf;

fn configure_logging() {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    builder.filter_module("hotprices_au_rs", log::LevelFilter::Debug);
    builder.init();
}

fn main() {
    configure_logging();
    // let day = "2024-03-10";
    // let cache_path = PathBuf::from(format!("output/{day}"));
    // let cache: FsCache = FsCache::new(cache_path);
    // fetch(&cache);
    load_legacy_products();
}

fn load_legacy_products() {
    let file = "/home/flozza/src/hotprices-au/output/coles/2024-03-08.json.gz";
    let file = PathBuf::from(file);
    let file = File::open(file).unwrap();
    let file = BufReader::new(file);
    let file = GzDecoder::new(file);
    load_from_legacy(file).unwrap();
}

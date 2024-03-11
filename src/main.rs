use hotprices_au_rs::cache::FsCache;
use hotprices_au_rs::stores::coles::fetch;

fn configure_logging() {
    let mut builder = env_logger::Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    builder.filter_module("hotprices_au_rs", log::LevelFilter::Debug);
    builder.init();
}

fn main() {
    configure_logging();
    let day = "2024-03-10";
    let cache_path = format!("output/{day}");
    let cache: FsCache = FsCache::new(cache_path);
    fetch(&cache);
}

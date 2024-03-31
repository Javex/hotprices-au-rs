use crate::cache::FsCache;
use crate::storage::{get_snapshot_path, remove, save_fetch_data};
use crate::stores::{coles, woolies, Store};
use std::fs::create_dir_all;
use std::path::PathBuf;
use time::OffsetDateTime;

pub fn do_sync(
    store: Store,
    quick: bool,
    print_save_path: bool,
    skip_existing: bool,
    output_dir: PathBuf,
    cache_path: PathBuf,
) -> anyhow::Result<()> {
    let day = OffsetDateTime::now_utc().date();
    let snapshot_path = get_snapshot_path(&output_dir, store, day);
    if print_save_path {
        print!("{}", snapshot_path.to_string_lossy());
        return Ok(());
    }

    if skip_existing && snapshot_path.exists() {
        println!(
            "Skipping because outputfile {} already exists and requested to skip if output file exists.",
            snapshot_path.to_string_lossy(),
        );
        return Ok(());
    }

    let cache_path = cache_path.join(store.to_string()).join(day.to_string());
    create_dir_all(&cache_path)?;
    let cache: FsCache = FsCache::new(cache_path.clone());
    let fetch_data = match store {
        Store::Coles => coles::fetch(&cache, quick)?,
        Store::Woolies => woolies::fetch(&cache, quick)?,
    };
    save_fetch_data(fetch_data, &snapshot_path)?;
    remove(&cache_path)?;
    Ok(())
}

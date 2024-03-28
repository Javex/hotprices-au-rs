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
) -> anyhow::Result<()> {
    if skip_existing {
        todo!("Not implemented yet");
    }
    let day = OffsetDateTime::now_utc().date();
    if print_save_path {
        print!(
            "{}",
            get_snapshot_path(&output_dir, store, day).to_str().unwrap()
        );
        return Ok(());
    }
    let cache_path = output_dir.join(store.to_string()).join(day.to_string());
    create_dir_all(&cache_path)?;
    let cache: FsCache = FsCache::new(cache_path.clone());
    let fetch_data = match store {
        Store::Coles => coles::fetch(&cache, quick)?,
        Store::Woolies => woolies::fetch(&cache, quick)?,
    };
    save_fetch_data(fetch_data, &output_dir, store, day)?;
    remove(&cache_path)?;
    Ok(())
}

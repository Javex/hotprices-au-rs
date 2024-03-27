use crate::cache::FsCache;
use crate::storage::{compress, remove, save_fetch_data};
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
    if print_save_path || skip_existing {
        todo!("Not implemented yet");
    }
    let day = OffsetDateTime::now_utc().date();
    let cache_path = output_dir.join(store.to_string()).join(day.to_string());
    create_dir_all(&cache_path)?;
    let cache: FsCache = FsCache::new(cache_path.clone());
    match store {
        Store::Coles => {
            coles::fetch(&cache, quick)?;
            compress(&cache_path)?;
        }
        Store::Woolies => {
            let fetch_data = woolies::fetch(&cache, quick)?;
            save_fetch_data(fetch_data, &output_dir, Store::Woolies, day)?;
        }
    };
    remove(&cache_path)?;
    Ok(())
}

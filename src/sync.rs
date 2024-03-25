use crate::cache::FsCache;
use crate::storage::{compress, remove};
use crate::stores::{coles, Store};
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
    let day = OffsetDateTime::now_utc().date().to_string();
    let cache_path = output_dir.join(store.to_string()).join(day);
    let cache: FsCache = FsCache::new(cache_path.clone());
    match store {
        Store::Coles => coles::fetch(&cache, quick)?,
        Store::Woolies => todo!("not implemented"),
    };
    compress(&cache_path)?;
    remove(&cache_path)?;
    Ok(())
}

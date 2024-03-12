use crate::errors::Result;
use std::fs::{create_dir_all, read_to_string, File};
use std::io::prelude::*;
use std::path::PathBuf;

use mockall::automock;

pub type FetchCallback<'a> = &'a dyn Fn() -> Result<String>;

pub struct FsCache {
    path: PathBuf,
}

#[automock]
impl FsCache {
    pub fn new(path: String) -> FsCache {
        let path = PathBuf::from(path);
        FsCache { path }
    }

    fn store(&self, path: &PathBuf, resp: &str) -> std::io::Result<()> {
        // Ensure directory tree exists
        // Guaranteed to have a parent
        create_dir_all(&path.parent().unwrap())?;

        // Save page
        let mut file = File::create(&path)?;
        file.write_all(resp.as_bytes())
    }

    fn load(&self, path: &PathBuf) -> std::io::Result<String> {
        read_to_string(path)
    }

    pub fn get_or_fetch<'a>(&self, file: String, fetch: FetchCallback<'a>) -> Result<String> {
        let path = self.path.join(file.clone());
        match path.exists() {
            true => {
                log::debug!("get_or_fetch: Loading file \"{file}\" from cache");
                Ok(self.load(&path)?)
            }
            false => {
                log::debug!("get_or_fetch: Loading file \"{file}\" from backend");
                let resp = fetch()?;
                self.store(&path, &resp)?;
                Ok(resp)
            }
        }
    }
}

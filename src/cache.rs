use std::fs::{create_dir_all, read_to_string, File};
use std::io::prelude::*;
use std::path::PathBuf;
use std::error::Error;

pub type FetchCallback<'a> = &'a dyn Fn() -> reqwest::Result<String>;

pub trait Cache {
    fn get_or_fetch(&self, file: String, fetch: FetchCallback) -> Result<String, Box<dyn Error>>;
}

pub struct FsCache {
    path: PathBuf,
}

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
}

impl Cache for FsCache {
    fn get_or_fetch(&self, file: String, fetch: FetchCallback) -> Result<String, Box<dyn Error>>
    {
        let path = self.path.join(file.clone());
        match path.exists() {
            true => {
                log::debug!("get_or_fetch: Loading file \"{file}\" from cache");
Ok(self.load(&path)?)
            },
            false => {
                log::debug!("get_or_fetch: Loading file \"{file}\" from backend");
                let resp = fetch()?;
                self.store(&path, &resp)?;
                Ok(resp)
            }
        }
    }
}

pub struct NullCache {}

impl Cache for NullCache {
    fn get_or_fetch(&self, _file: String, fetch: FetchCallback) -> Result<String, Box<dyn Error>>
    {
        Ok(fetch()?)
    }
}

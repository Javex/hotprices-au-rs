#[double]
use super::http::WooliesHttpClient;
use crate::cache::FsCache;
use log::debug;
use mockall_double::double;
use serde::Deserialize;
use std::{fmt::Display, io::Read};

#[derive(Deserialize)]
pub struct CategoryResponse {
    #[serde(rename = "Bundles")]
    pub bundles: Vec<serde_json::Value>,
    #[serde(rename = "TotalRecordCount")]
    pub total_record_count: i64,
}

impl CategoryResponse {
    pub fn from_reader(reader: impl Read) -> anyhow::Result<CategoryResponse> {
        let json_data: CategoryResponse = serde_json::from_reader(reader)?;
        Ok(json_data)
    }
}

pub struct Category<'a> {
    client: &'a WooliesHttpClient,
    id: String,
    buf: Vec<serde_json::Value>,
    page: i32,
    product_count: i64,
    finished: bool,
    cache: &'a FsCache,
}

impl<'a> Category<'a> {
    pub fn new(id: String, client: &'a WooliesHttpClient, cache: &'a FsCache) -> Category<'a> {
        Self {
            client,
            id,
            buf: Vec::new(),
            page: 1,
            product_count: 0,
            finished: false,
            cache,
        }
    }

    fn get_category(&self, page: i32) -> anyhow::Result<CategoryResponse> {
        let path = format!("categories/{}/page_{}.json", self.id, page);
        let fetch = &|| self.client.get_category(&self.id, page);
        let resp = self.cache.get_or_fetch(path, fetch)?;
        Ok(serde_json::from_str(&resp)?)
    }
}

impl<'a> Display for Category<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl<'a> Iterator for Category<'a> {
    type Item = anyhow::Result<serde_json::Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf.is_empty() && !self.finished {
            let category_response = match self.get_category(self.page) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            self.buf = category_response.bundles;
            self.product_count += self.buf.len() as i64;
            self.page += 1;
            if self.product_count >= category_response.total_record_count {
                self.finished = true;
            }
            debug!("New page wit hresults loaded in iterator. Product count: {}, finished: {}, buffer size: {}", self.product_count, self.finished, self.buf.len());
        }
        self.buf.pop().map(Ok)
    }
}

#[cfg(test)]
mod test {
    use crate::cache::test::get_cache;
    use crate::stores::woolies::get_categories;

    use super::*;
    use std::fs;
    use std::path::PathBuf;

    pub fn load_file(fname: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("resources/test/woolies");
        path.push(fname);
        fs::read_to_string(path).expect("Failed to load test file")
    }

    #[test]
    fn test_get_categories() {
        let mut client = WooliesHttpClient::default();
        client.expect_get_categories().times(1).returning(|| {
            let file = load_file("categories.json");
            Result::Ok(file)
        });
        let cache = get_cache();
        let categories = get_categories(&cache, &client).unwrap();
        assert_eq!(categories.len(), 1);
        let [ref category] = categories[..] else {
            panic!("Invalid category size")
        };
        assert_eq!(category.id, "1-AAAAAAAA");
    }
}

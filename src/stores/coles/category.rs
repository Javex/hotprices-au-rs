use std::io::Read;

#[double]
use super::http::ColesHttpClient;
use crate::cache::FsCache;
use crate::errors::Result;
use log::debug;
use mockall_double::double;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SearchResults {
    #[serde(rename = "results")]
    pub results: Vec<serde_json::Value>,
    #[serde(rename = "noOfResults")]
    no_of_results: i64,
}

impl SearchResults {
    pub fn from_reader(reader: impl Read) -> Result<SearchResults> {
        let json_data: CategoryJson = serde_json::from_reader(reader)?;
        Ok(json_data.page_props.search_results)
    }
}

#[derive(Deserialize)]
struct PageProps {
    #[serde(rename = "searchResults")]
    search_results: SearchResults,
}

#[derive(Deserialize)]
pub struct CategoryJson {
    #[serde(rename = "pageProps")]
    page_props: PageProps,
}

pub struct Category<'a> {
    client: &'a ColesHttpClient,
    slug: String,
    buf: Vec<serde_json::Value>,
    page: i32,
    product_count: i64,
    finished: bool,
    cache: &'a FsCache,
}

impl<'a> Category<'a> {
    pub fn new(cat_slug: &str, client: &'a ColesHttpClient, cache: &'a FsCache) -> Category<'a> {
        Category {
            client,
            slug: cat_slug.to_string(),
            buf: Vec::new(),
            page: 1,
            product_count: 0,
            finished: false,
            cache,
        }
    }
    fn get_category(&self, page: i32) -> Result<SearchResults> {
        let path = format!("categories/{}/page_{}.json", self.slug, page);
        let fetch = &|| self.client.get_category(&self.slug, page);
        let resp = self.cache.get_or_fetch(path, fetch)?;
        let json_data: CategoryJson = serde_json::from_str(&resp)?;
        Ok(json_data.page_props.search_results)
    }
}

impl<'a> Iterator for Category<'a> {
    type Item = Result<serde_json::Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf.is_empty() && !self.finished {
            let search_results = match self.get_category(self.page) {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            self.buf = search_results.results;
            self.product_count += self.buf.len() as i64;
            self.page += 1;
            if self.product_count >= search_results.no_of_results {
                self.finished = true;
            }
            debug!(
                "New page with results loaded in iterator. Product count: {}, finished: {}, buffer size: {}",
                self.product_count,
                self.finished,
                self.buf.len()
            );
        }

        self.buf.pop().map(Ok)
    }
}

#[cfg(test)]
mod test {
    use super::super::test::load_file;
    use super::*;
    #[test]
    fn test_load_empty_search_results() {
        let file = load_file("empty_search_results.json");
        let json_data: CategoryJson = serde_json::from_str(&file).unwrap();
        let search_results = json_data.page_props.search_results;
        assert_eq!(search_results.no_of_results, 749);
        let results = search_results.results;
        assert_eq!(results.len(), 0);
    }
}

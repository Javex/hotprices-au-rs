#[double]
use super::http::ColesHttpClient;
#[double]
use crate::cache::FsCache;
use crate::errors::{Error, Result};
use crate::stores::coles::get_cache_key;
use mockall_double::double;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct PricingUnit {
    quantity: f64,
    #[serde(rename = "ofMeasureQuantity")]
    of_measure_quantity: f64,
    #[serde(rename = "ofMeasureUnits")]
    of_measure_units: String,
    price: f64,
    #[serde(rename = "ofMeasureType")]
    of_measure_type: String,
    #[serde(rename = "isWeighted")]
    is_weighted: bool,
}

#[derive(Deserialize, Debug)]
struct Pricing {
    now: f64,
    was: f64,
    unit: PricingUnit,
    comparable: String,
}

#[derive(Deserialize, Debug)]
struct SearchResult {
    #[serde(rename = "_type")]
    product_type: String,
    #[serde(rename = "adId")]
    ad_id: Option<String>,
    id: i64,
    name: String,
    brand: String,
    description: String,
    size: String,
    pricing: Pricing,
}

#[derive(Deserialize)]
struct SearchResults {
    #[serde(rename = "results")]
    // results: Vec<SearchResult>,
    results: Vec<serde_json::Value>,
    #[serde(rename = "noOfResults")]
    no_of_results: i64,
}

#[derive(Deserialize)]
struct PageProps {
    #[serde(rename = "searchResults")]
    search_results: SearchResults,
}

#[derive(Deserialize)]
struct CategoryJson {
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
        let path = get_cache_key(&format!("categories/{}/page_{}.json", self.slug, page));
        let fetch = &|| self.client.get_category(&self.slug, page);
        let resp = self.cache.get_or_fetch(path, fetch)?;
        let json_data: CategoryJson = serde_json::from_str(&resp)?;
        Ok(json_data.page_props.search_results)
    }
}

impl<'a> Iterator for Category<'a> {
    type Item = Result<serde_json::Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf.len() == 0 && !self.finished {
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
            println!(
                "product_count: {}, finished: {}, buf.len(): {}",
                self.product_count,
                self.finished,
                self.buf.len()
            );
        }

        match self.buf.pop() {
            Some(v) => Some(Ok(v)),
            None => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::super::test::load_file;
    #[test]
    fn test_load_json() {
        let file = load_file("page_1.json");
        let _json_data: super::CategoryJson = serde_json::from_str(&file).unwrap();
    }
}

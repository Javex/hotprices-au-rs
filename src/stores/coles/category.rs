#[double]
use super::http::ColesHttpClient;
#[double]
use crate::cache::FsCache;
use crate::errors::{Error, Result};
use crate::stores::coles::get_cache_key;
use mockall_double::double;
use serde::Deserialize;

const IGNORED_RESULT_TYPES: [&str; 2] = ["SINGLE_TILE", "CONTENT_ASSOCIATION"];

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
    id: i64,
    name: String,
    brand: String,
    description: String,
    size: String,
    pricing: Pricing,
}

impl SearchResult {
    fn from_json_value(mut value: serde_json::Value) -> Result<Option<SearchResult>> {
        let obj = match &value {
            serde_json::Value::Object(map) => map,
            x => {
                return Err(Error::Message(format!(
                    "Invalid object type value for {}",
                    x
                )))
            }
        };

        // _type field must be present
        let result_type = obj
            .get("_type")
            .ok_or(Error::Message("Missing key _type".to_string()))?;
        let result_type = match result_type {
            serde_json::Value::String(s) => s,
            x => {
                return Err(Error::Message(format!(
                    "Invalid type for _type, expected string: {}",
                    x
                )))
            }
        };

        // Ads are ignored here
        if IGNORED_RESULT_TYPES.contains(&result_type.as_str())
            && obj.get("adId").is_some_and(|x| !x.is_null())
        {
            return Ok(None);
        }

        let search_result: SearchResult = serde_json::from_value(value)?;

        Ok(Some(search_result))
    }
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
    use crate::stores::coles::category::SearchResult;

    use super::super::test::load_file;
    #[test]
    fn test_load_empty_search_results() {
        let file = load_file("empty_search_results.json");
        let json_data: super::CategoryJson = serde_json::from_str(&file).unwrap();
        let search_results = json_data.page_props.search_results;
        assert_eq!(search_results.no_of_results, 749);
        let results = search_results.results;
        assert_eq!(results.len(), 0);
        // let _ad = results.pop().unwrap();
    }

    #[test]
    fn test_load_product() {
        let file = load_file("product.json");
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();

        let product = SearchResult::from_json_value(json_data)
            .expect("Returned error instead of result")
            .expect("Returned None instead of Some");
        assert_eq!(product.id, 1113465);
    }

    #[test]
    fn test_load_ad() {
        let file = load_file("ad.json");
        let json_data: serde_json::Value = serde_json::from_str(&file).unwrap();

        let ad = SearchResult::from_json_value(json_data)
            .expect("Returned error instead of result");
        assert!(ad.is_none());
    }
}

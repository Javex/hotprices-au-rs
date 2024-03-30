use std::{collections::HashMap, fmt::Display};

#[double]
use super::http::ColesHttpClient;
use super::product::SearchResult;
use crate::{cache::FsCache, conversion, errors::Error};
use anyhow::Context;
use log::debug;
use mockall_double::double;
use serde::{Deserialize, Serialize};

const SKIP_CATEGORIES: [&str; 2] = ["down-down", "back-to-school"];

#[derive(Deserialize, Serialize)]
pub(crate) struct Category {
    #[serde(rename = "seoToken")]
    seo_token: String,

    // This field is missing when getting a response for the category list, it's a custom field
    // that will hold products as they are getting fetched
    #[serde(default, rename = "Products")]
    products: Vec<serde_json::Value>,

    // Capture any values not explicitly specified so they survive serialization/deserialization
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

impl Category {
    fn get_category(
        &self,
        client: &ColesHttpClient,
        cache: &FsCache,
        page: i32,
    ) -> anyhow::Result<SearchResults> {
        let path = format!("categories/{}/page_{}.json", self.seo_token, page);
        let fetch = &|| client.get_category(&self.seo_token, page);
        let resp = cache.get_or_fetch(path, fetch)?;
        let json_data: CategoryJson = serde_json::from_str(&resp)?;
        Ok(json_data.page_props.search_results)
    }

    pub(crate) fn fetch_products(
        &mut self,
        client: &ColesHttpClient,
        cache: &FsCache,
        quick: bool,
    ) -> anyhow::Result<usize> {
        let mut products = Vec::new();
        let mut page = 1;
        loop {
            let category_response = self.get_category(client, cache, page)?;
            let new_products = category_response.results;
            page += 1;
            debug!(
                "New page with results loaded. Product count: {}, products on this page: {}, expected total: {}",
                products.len(),
                new_products.len(),
                category_response.no_of_results,
            );
            products.extend(new_products);

            if products.len() as i64 >= category_response.no_of_results || quick {
                break;
            }
        }
        self.products = products;
        Ok(self.products.len())
    }
}

impl conversion::Category<SearchResult> for Category {
    fn is_filtered(&self) -> bool {
        if SKIP_CATEGORIES.contains(&self.seo_token.as_str()) {
            return true;
        }
        false
    }

    fn into_products(self) -> anyhow::Result<Vec<SearchResult>> {
        self.products
            .into_iter()
            .filter_map(|v| match SearchResult::from_json_value(v) {
                Ok(v) => Some(Ok(v)),
                Err(err) => match err {
                    Error::AdResult => None,
                    _ => Some(
                        Err(err)
                            .context("Failed to convert product {:#?} to SearchResult from JSON"),
                    ),
                },
            })
            .collect()
    }
}

impl Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.seo_token)
    }
}

#[derive(Deserialize)]
struct SearchResults {
    #[serde(rename = "results")]
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

#[cfg(test)]
mod test {
    use crate::conversion::Category as CategoryTrait;
    use crate::stores::coles::get_categories;

    use super::*;
    use serde_json::json;
    #[test]
    fn test_load_empty_search_results() {
        let empty_search_results = json!(
            {
              "pageProps": {
                "searchResults": {
                  "noOfResults": 749,
                  "results": []
                }
              }
            }
        );
        let json_data: CategoryJson = serde_json::from_value(empty_search_results).unwrap();
        let search_results = json_data.page_props.search_results;
        assert_eq!(search_results.no_of_results, 749);
        let results = search_results.results;
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_get_categories() {
        let mut client = ColesHttpClient::default();
        client.expect_get_categories().returning(|| {
            Ok(json!({
                "catalogGroupView": [
                    {
                        "seoToken": "category-slug",
                        "someExtraField": "Extra value"
                    }
                ]
            })
            .to_string())
        });

        let categories = get_categories(&client).unwrap();
        let mut categories = categories.catalog_group_view;
        assert_eq!(categories.len(), 1);
        let category = categories.pop().unwrap();
        assert!(category.products.is_empty());
        let cat_json = serde_json::to_value(category).unwrap();
        assert_eq!(cat_json["someExtraField"], "Extra value");
    }

    #[test]
    fn test_into_products_ad_result_is_removed() {
        let category = Category {
            seo_token: String::from("slug"),
            products: vec![json!({
                "_type": "SINGLE_TILE",
                "adId": "ad",
            })],
            extra: HashMap::new(),
        };
        let search_results = category.into_products().unwrap();
        assert!(search_results.is_empty());
    }
}

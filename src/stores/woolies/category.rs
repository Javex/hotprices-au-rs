#[double]
use super::http::WooliesHttpClient;
use super::product::{Bundle, BundleProduct};
use crate::cache::FsCache;
use crate::conversion;
use crate::errors::Error;
use itertools::Either;
use itertools::Itertools;
use log::debug;
use mockall_double::double;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display};

const IGNORED_CATEGORY_IDS: [&str; 2] = [
    "specialsgroup", // Ads
    "1_8E4DA6F",     // Beer, Wine & Spirits
];
const IGNORED_CATEGORY_DESCRIPTIONS: [&str; 2] = [
    "Front of Store",       // Expect duplicates
    "Beer, Wine & Spirits", // skip alcohol because it has weird sizing and isn't that important
];

#[derive(Deserialize, Serialize)]
pub(crate) struct Category {
    #[serde(rename = "NodeId")]
    node_id: String,
    #[serde(rename = "Description")]
    description: String,

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
        client: &WooliesHttpClient,
        cache: &FsCache,
        page: i32,
    ) -> anyhow::Result<CategoryResponse> {
        let path = format!("categories/{}/page_{}.json", self.node_id, page);
        let fetch = &|| client.get_category(&self.node_id, page);
        let resp = cache.get_or_fetch(path, fetch)?;
        Ok(serde_json::from_str(&resp)?)
    }

    pub(crate) fn fetch_products(
        &mut self,
        client: &WooliesHttpClient,
        cache: &FsCache,
        quick: bool,
    ) -> anyhow::Result<usize> {
        let mut products = Vec::new();
        let mut page = 1;
        loop {
            let category_response = self.get_category(client, cache, page)?;
            let new_products = category_response.bundles;
            let new_product_count = new_products.len();
            page += 1;
            debug!(
                "New page with results loaded. Product count: {}, products on this page: {}, expected total: {}",
                products.len(),
                new_products.len(),
                category_response.total_record_count,
            );
            products.extend(new_products);

            if products.len() as i64 >= category_response.total_record_count
                || new_product_count == 0
                || quick
            {
                break;
            }
        }
        self.products = products;
        Ok(self.products.len())
    }
}

impl conversion::Category<BundleProduct> for Category {
    fn is_filtered(&self) -> bool {
        if IGNORED_CATEGORY_IDS.contains(&self.node_id.as_str()) {
            return true;
        }

        if IGNORED_CATEGORY_DESCRIPTIONS.contains(&self.description.as_str()) {
            return true;
        }

        false
    }

    fn into_products(self) -> (Vec<BundleProduct>, Vec<Error>) {
        self.products
            .into_iter()
            .partition_map(|v| match serde_json::from_value::<Bundle>(v) {
                Ok(v) => match v.products.len() {
                    1 => Either::Left(v.products.into_iter().next().unwrap()),
                    _ => Either::Right(Error::ProductConversion(format!(
                        "Invalid number of products in bundle: {}",
                        v.products.len()
                    ))),
                },
                Err(v) => Either::Right(v.into()),
            })
    }
}

impl Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.node_id)
    }
}

#[derive(Deserialize)]
pub(crate) struct CategoryResponse {
    #[serde(rename = "Bundles")]
    pub(crate) bundles: Vec<serde_json::Value>,
    #[serde(rename = "TotalRecordCount")]
    pub(crate) total_record_count: i64,
}

#[cfg(test)]
mod test {
    use crate::cache::test::get_cache;
    use crate::conversion::Category as CategoryTrait;
    use crate::stores::woolies::get_categories;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_get_categories() {
        let mut client = WooliesHttpClient::default();
        client.expect_get_categories().returning(|| {
            Ok(json!({
                "Categories": [
                    {
                        "NodeId": "1_ABCDEF12",
                        "Description": "Category Description",
                        "SomeExtraField": "Extra value"
                    }
                ]
            })
            .to_string())
        });

        let categories = get_categories(&client).unwrap();
        let mut categories = categories.categories;
        assert_eq!(categories.len(), 1);
        let category = categories.pop().unwrap();
        assert!(category.products.is_empty());
        let cat_json = serde_json::to_value(category).unwrap();
        assert_eq!(cat_json["SomeExtraField"], "Extra value");
    }

    impl Default for Category {
        fn default() -> Self {
            Self {
                node_id: String::from("1_ABCDEF12"),
                description: String::from("Category Description"),
                products: vec![],
                extra: HashMap::default(),
            }
        }
    }

    #[test]
    fn test_fetch_products() {
        let mut client = WooliesHttpClient::default();
        client.expect_get_category().times(1).returning(|_, _| {
            Ok(json!({
                // Hack: Don't need full product here since it's just treated as arbitrary JSON
                "Bundles": [1, 2],
                "TotalRecordCount": 2,
            })
            .to_string())
        });
        let cache = get_cache();
        let mut category = Category::default();
        category.fetch_products(&client, &cache, false).unwrap();
        assert_eq!(category.products.len(), 2);
    }

    #[test]
    fn test_fetch_products_paginated() {
        let mut client = WooliesHttpClient::default();
        client.expect_get_category().times(2).returning(|_, _| {
            Ok(json!({
                // Hack: Don't need full product here since it's just treated as arbitrary JSON
                "Bundles": [1, 2],
                "TotalRecordCount": 4,
            })
            .to_string())
        });
        let cache = get_cache();
        let mut category = Category::default();
        category.fetch_products(&client, &cache, false).unwrap();
        assert_eq!(category.products.len(), 4);
    }

    // API "lies" and returns fewer products than it claims to
    #[test]
    fn test_fetch_products_empty() {
        let mut client = WooliesHttpClient::default();
        client.expect_get_category().times(1).returning(|_, _| {
            Ok(json!({
                "Bundles": [],
                "TotalRecordCount": 1,
            })
            .to_string())
        });
        let cache = get_cache();
        let mut category = Category::default();
        category.fetch_products(&client, &cache, false).unwrap();
        assert_eq!(category.products.len(), 0);
    }

    #[test]
    fn test_is_filtered() {
        let category = Category {
            description: String::from(IGNORED_CATEGORY_DESCRIPTIONS[0]),
            ..Default::default()
        };
        assert!(category.is_filtered());
    }
}

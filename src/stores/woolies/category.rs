#[double]
use super::http::WooliesHttpClient;
use super::product::{Bundle, BundleProduct};
use crate::cache::FsCache;
use crate::category::CategoryCode;
use crate::category::FruitAndVeg;
use crate::conversion;
use crate::errors::Result;
use anyhow::bail;
use anyhow::Context;
use log::debug;
use mockall_double::double;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::{collections::HashMap, fmt::Display};

const IGNORED_CATEGORY_IDS: [&str; 2] = [
    "specialsgroup", // Ads
    "1_8E4DA6F",     // Beer, Wine & Spirits
];
const IGNORED_CATEGORY_DESCRIPTIONS: [&str; 2] = [
    "Front of Store",       // Expect duplicates
    "Beer, Wine & Spirits", // skip alcohol because it has weird sizing and isn't that important
];

#[derive(Deserialize, Serialize, Debug, Default)]
pub(crate) struct CategoryInfo {
    #[serde(rename = "NodeId")]
    node_id: String,
    #[serde(rename = "Description")]
    description: String,
    #[serde(rename = "IsSpecial")]
    is_special: bool,

    // Capture any values not explicitly specified so they survive serialization/deserialization
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
struct SubCategory {
    #[serde(flatten)]
    category_info: CategoryInfo,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub(crate) struct Category {
    #[serde(flatten)]
    category_info: CategoryInfo,
    #[serde(rename = "Children")]
    children: Vec<SubCategory>,

    // This field is missing when getting a response for the category list, it's a custom field
    // that will hold products as they are getting fetched
    #[serde(default, rename = "Products")]
    products: Vec<serde_json::Value>,
}

impl Category {
    fn get_category(
        &self,
        client: &WooliesHttpClient,
        cache: &FsCache,
        page: i32,
    ) -> anyhow::Result<CategoryResponse> {
        let path = format!(
            "categories/{}/page_{}.json",
            self.category_info.node_id, page
        );
        let fetch = &|| client.get_category(&self.category_info.node_id, page);
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

    pub(crate) fn code(&self, subcategory_names: Vec<String>) -> Result<Option<CategoryCode>> {
        use crate::category::Category;

        let category = match self.children.iter().find(|c| {
            !c.category_info.is_special && subcategory_names.contains(&c.category_info.description)
        }) {
            Some(child) => match child.category_info.node_id.as_str() {
                "1-5931EE89" => Category::FruitAndVeg(FruitAndVeg::Fruit),
                "1_AC17EDD" => Category::FruitAndVeg(FruitAndVeg::Veg),
                "1_2684504" => Category::FruitAndVeg(FruitAndVeg::SaladAndHerbs),
                _ => return Ok(None),
            },
            None => return Ok(None),
        };

        Ok(Some(CategoryCode { category }))
    }
}

impl conversion::Category for Category {
    type Product = BundleProduct;
    fn is_filtered(&self) -> bool {
        if IGNORED_CATEGORY_IDS.contains(&self.category_info.node_id.as_str()) {
            return true;
        }

        if IGNORED_CATEGORY_DESCRIPTIONS.contains(&self.category_info.description.as_str()) {
            return true;
        }

        false
    }

    fn into_products(mut self) -> anyhow::Result<Vec<BundleProduct>> {
        let products: Vec<_> = self.products.drain(..).collect();
        let category = Rc::new(self);

        products
            .into_iter()
            .map(|v| match serde_json::from_value::<Bundle>(v) {
                Ok(v) => match v.products.len() {
                    1 => {
                        let mut product = v.products.into_iter().next().unwrap();
                        product.set_category(Rc::clone(&category));
                        Ok(product)
                    }
                    _ => bail!("Invalid number of products in bundle: {}", v.products.len()),
                },
                Err(err) => {
                    Err(err).context("Failed to convert product to BundleProduct from JSON")
                }
            })
            .collect()
    }
}

impl Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.category_info.node_id)
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
    use crate::conversion::Product as ProductTrait;
    use crate::stores::woolies::get_categories;
    use serde_json::json;
    use time::{Date, Month};

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
                        "IsSpecial": false,
                        "Children": [],
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
            category_info: CategoryInfo {
                description: String::from(IGNORED_CATEGORY_DESCRIPTIONS[0]),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(category.is_filtered());
    }

    #[test]
    fn test_category() {
        let json_data = json!(
            {
              "NodeId": "1-E5BEE36E",
              "Description": "Fruit & Veg",
              "IsSpecial": false,
              "Children": [{
                  "NodeId": "1-5931EE89",
                  "Description": "Fruit",
                  "IsSpecial": false,
              }, {
                  "NodeId": "1_AC17EDD",
                  "Description": "Vegetables",
                  "IsSpecial": false,
              }],
              "Products": [
                {
                  "Products": [
                    {
                      "Stockcode": 123,
                      "CupPrice": 2.07,
                      "CupMeasure": "100G",
                      "Price": 12.02,
                      "WasPrice": 12.02,
                      "IsInStock": true,
                      "Name": "product name",
                      "Description": "some long product description",
                      "Unit": "Each",
                      "PackageSize": "100g",
                      "AdditionalAttributes": {
                        "piessubcategorynamesjson": "[\"Apples & Pears\", \"Fruit\"]",
                        "piescategorynamesjson": "[]",
                      }
                    }
                  ]
                }
              ]
            }
        );
        let category: Category = serde_json::from_value(json_data).unwrap();
        let mut products = category.into_products().unwrap();
        assert_eq!(products.len(), 1);
        let product = products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let product = product
            .try_into_snapshot_and_date(date)
            .expect("Expected conversion to succeed");
        assert_eq!(
            product.category().unwrap(),
            crate::category::Category::FruitAndVeg(FruitAndVeg::Fruit)
        );
    }

    #[test]
    fn test_second_category() {
        let json_data = json!(
            {
              "NodeId": "1-E5BEE36E",
              "Description": "Fruit & Veg",
              "IsSpecial": false,
              "Children": [{
                  "NodeId": "1-5931EE89",
                  "Description": "Fruit",
                  "IsSpecial": false,
              }, {
                  "NodeId": "1_AC17EDD",
                  "Description": "Vegetables",
                  "IsSpecial": false,
              }],
              "Products": [
                {
                  "Products": [
                    {
                      "Stockcode": 123,
                      "CupPrice": 2.07,
                      "CupMeasure": "100G",
                      "Price": 12.02,
                      "WasPrice": 12.02,
                      "IsInStock": true,
                      "Name": "product name",
                      "Description": "some long product description",
                      "Unit": "Each",
                      "PackageSize": "100g",
                      "AdditionalAttributes": {
                        "piessubcategorynamesjson": "[\"Vegetables\"]",
                        "piescategorynamesjson": "[]",
                      }
                    }
                  ]
                }
              ]
            }
        );
        let category: Category = serde_json::from_value(json_data).unwrap();
        let mut products = category.into_products().unwrap();
        assert_eq!(products.len(), 1);
        let product = products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let product = product
            .try_into_snapshot_and_date(date)
            .expect("Expected conversion to succeed");
        assert_eq!(
            product.category().unwrap(),
            crate::category::Category::FruitAndVeg(FruitAndVeg::Veg)
        );
    }

    #[test]
    fn test_category_special() {
        let json_data = json!(
            {
              "NodeId": "1-E5BEE36E",
              "Description": "Fruit & Veg",
              "IsSpecial": false,
              "Children": [{
                  "NodeId": "1_AC17EDD_SPECIALS",
                  "Description": "Vegetables Specials",
                  "IsSpecial": true,
              }, {
                  "NodeId": "1_AC17EDD",
                  "Description": "Vegetables",
                  "IsSpecial": false,
              }],
              "Products": [
                {
                  "Products": [
                    {
                      "Stockcode": 123,
                      "CupPrice": 2.07,
                      "CupMeasure": "100G",
                      "Price": 12.02,
                      "WasPrice": 12.02,
                      "IsInStock": true,
                      "Name": "product name",
                      "Description": "some long product description",
                      "Unit": "Each",
                      "PackageSize": "100g",
                      "AdditionalAttributes": {
                        "piessubcategorynamesjson": "[\"Vegetables\"]",
                        "piescategorynamesjson": "[]",
                      }
                    }
                  ]
                }
              ]
            }
        );
        let category: Category = serde_json::from_value(json_data).unwrap();
        let mut products = category.into_products().unwrap();
        assert_eq!(products.len(), 1);
        let product = products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let product = product
            .try_into_snapshot_and_date(date)
            .expect("Expected conversion to succeed");
        assert_eq!(
            product.category().unwrap(),
            crate::category::Category::FruitAndVeg(FruitAndVeg::Veg)
        );
    }

    #[test]
    fn test_category_use_piescategory() {
        let json_data = json!(
            {
              "NodeId": "1-E5BEE36E",
              "Description": "Fruit & Veg",
              "IsSpecial": false,
              "Children": [{
                  "NodeId": "1-5931EE89",
                  "Description": "Fruit",
                  "IsSpecial": false,
              }, {
                  "NodeId": "1_AC17EDD",
                  "Description": "Vegetables",
                  "IsSpecial": false,
              }],
              "Products": [
                {
                  "Products": [
                    {
                      "Stockcode": 123,
                      "CupPrice": 2.07,
                      "CupMeasure": "100G",
                      "Price": 12.02,
                      "WasPrice": 12.02,
                      "IsInStock": true,
                      "Name": "product name",
                      "Description": "some long product description",
                      "Unit": "Each",
                      "PackageSize": "100g",
                      "AdditionalAttributes": {
                        "piessubcategorynamesjson": "[\"Apples & Pears\"]",
                        "piescategorynamesjson": "[\"Fruit\"]",
                      }
                    }
                  ]
                }
              ]
            }
        );
        let category: Category = serde_json::from_value(json_data).unwrap();
        let mut products = category.into_products().unwrap();
        assert_eq!(products.len(), 1);
        let product = products.pop().unwrap();
        let date = Date::from_calendar_date(2024, Month::January, 1).unwrap();
        let product = product
            .try_into_snapshot_and_date(date)
            .expect("Expected conversion to succeed");
        assert_eq!(
            product.category().unwrap(),
            crate::category::Category::FruitAndVeg(FruitAndVeg::Fruit)
        );
    }
}

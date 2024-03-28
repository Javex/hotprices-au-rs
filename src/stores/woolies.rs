use crate::cache::FsCache;
use crate::conversion::Category as CategoryTrait;
#[double]
use crate::stores::woolies::http::WooliesHttpClient;
use log::{debug, info};
use mockall_double::double;
use serde::Deserialize;

use self::category::Category;

mod category;
mod http;
pub mod product;

#[derive(Deserialize)]
struct CategoriesResponse {
    #[serde(rename = "Categories")]
    categories: Vec<Category>,
}

fn get_categories(client: &WooliesHttpClient) -> anyhow::Result<CategoriesResponse> {
    let resp = client.get_categories()?;
    let categories: CategoriesResponse = serde_json::from_str(&resp)?;
    Ok(categories)
}

pub fn fetch(cache: &FsCache, quick: bool) -> anyhow::Result<String> {
    info!("Starting fetch for woolies");
    let client = WooliesHttpClient::new();
    let categories = get_categories(&client)?;
    let mut categories: Vec<_> = categories
        .categories
        .into_iter()
        .filter(|c| !c.is_filtered())
        .collect();
    debug!("Loaded categories for Woolies, have {}", categories.len());
    for category in categories.iter_mut() {
        let product_count = category.fetch_products(&client, cache)?;
        debug!("Got category {} with {} products", category, product_count);
    }
    Ok(serde_json::to_string(&categories)?)
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_get_categories() {
        let mut client = WooliesHttpClient::default();
        client.expect_get_categories().returning(|| {
            Ok(json!({
                "Categories": [
                    {
                        "NodeId": "1-ABCDEF12",
                        "Description": "Category Description",
                        "SomeExtraField": "Extra value"
                    }
                ]
            })
            .to_string())
        });

        let categories = get_categories(&client).unwrap();
        let categories = categories.categories;
        assert_eq!(categories.len(), 1);
    }
}

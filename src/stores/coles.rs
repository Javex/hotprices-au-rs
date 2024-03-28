mod category;
mod http;
mod product;

pub(crate) use product::load_snapshot;

use crate::cache::FsCache;
use crate::conversion::Category as CategoryTrait;
use crate::stores::coles::category::Category;

use anyhow::bail;
#[double]
use http::ColesHttpClient;
use log::debug;
use mockall_double::double;
use scraper::Selector;
use serde::Deserialize;

#[derive(Deserialize)]
struct RuntimeConfig {
    #[serde(rename = "BFF_API_SUBSCRIPTION_KEY")]
    bff_api_subscription_key: String,
}

#[derive(Deserialize)]
struct NextData {
    #[serde(rename = "runtimeConfig")]
    runtime_config: RuntimeConfig,
    #[serde(rename = "buildId")]
    build_id: String,
}

#[derive(Deserialize)]
struct CategoriesResponse {
    #[serde(rename = "catalogGroupView")]
    catalog_group_view: Vec<Category>,
}

fn get_setup_data(client: &ColesHttpClient) -> anyhow::Result<(String, String)> {
    let resp = client.get_setup_data()?;
    let selector = Selector::parse("script#__NEXT_DATA__")
        .map_err(|err| anyhow::Error::msg(err.to_string()))?;
    let doc = scraper::Html::parse_document(&resp);
    let next_data_script = match doc.select(&selector).next() {
        Some(x) => x,
        None => bail!("couldn't find __NEXT_DATA__ script in HTML",),
    };
    let next_data_script = next_data_script.inner_html();
    let next_data: NextData = serde_json::from_str(&next_data_script)?;
    let api_key = next_data.runtime_config.bff_api_subscription_key;
    let version = next_data.build_id;

    Ok((api_key, version))
}

fn get_versioned_client(client: &ColesHttpClient) -> anyhow::Result<ColesHttpClient> {
    let (api_key, version) = get_setup_data(client)?;
    let client = ColesHttpClient::new_with_setup(&api_key, version)?;
    Ok(client)
}

fn get_categories(client: &ColesHttpClient) -> anyhow::Result<CategoriesResponse> {
    let resp = client.get_categories()?;
    let categories: CategoriesResponse = serde_json::from_str(&resp)?;
    Ok(categories)
}

pub(crate) fn fetch(cache: &FsCache, quick: bool) -> anyhow::Result<String> {
    log::info!("Starting fetch for coles");
    let client = ColesHttpClient::new()?;
    let client = get_versioned_client(&client)?;
    let categories = get_categories(&client)?;
    let mut categories: Vec<_> = categories
        .catalog_group_view
        .into_iter()
        .filter(|c| !c.is_filtered())
        .collect();
    debug!("Loaded categories for Coles, have {}", categories.len());
    for category in categories.iter_mut() {
        let product_count = category.fetch_products(&client, cache, quick)?;
        debug!("Got category {} with {} products", category, product_count);
        if quick {
            break;
        }
    }
    Ok(serde_json::to_string(&categories)?)
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    pub fn load_file(fname: &str) -> String {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("resources/test/coles");
        path.push(fname);
        fs::read_to_string(path).expect("Failed to load test file")
    }

    #[test]
    fn test_get_setup_data() {
        let mut client = ColesHttpClient::default();
        client.expect_get_setup_data().times(1).returning(|| {
            let file = load_file("index/index_good.html");
            Result::Ok(file)
        });
        let (api_key, version) = get_setup_data(&client).expect("Expected success");
        assert_eq!(version, "20240101.01_v1.01.0");
        assert_eq!(api_key, "testsubkey");
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
        let categories = categories.catalog_group_view;
        assert_eq!(categories.len(), 1);
    }
}

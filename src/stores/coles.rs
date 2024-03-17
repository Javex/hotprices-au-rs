pub mod category;
mod http;
pub mod product;

#[double]
use crate::cache::FsCache;
use crate::errors::Error;
use crate::{product::Product, stores::coles::product::SearchResult};
use flate2::write::GzEncoder;
use flate2::Compression;
#[double]
use http::ColesHttpClient;
use log::{self, info};
use mockall_double::double;
use scraper::Selector;
use serde::Deserialize;
use std::error::Error as StdError;
use std::fs::File;
use std::path::PathBuf;

const SKIP_CATEGORIES: [&str; 2] = ["down-down", "back-to-school"];

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
struct Categories {
    #[serde(rename = "catalogGroupView")]
    catalog_group_view: Vec<CategoryFields>,
}

impl Categories {
    fn get_items<'a>(
        &self,
        client: &'a ColesHttpClient,
        cache: &'a FsCache,
    ) -> Vec<category::Category<'a>> {
        self.catalog_group_view
            .iter()
            .map(|slug| slug.seo_token.clone())
            .filter(|slug| !SKIP_CATEGORIES.contains(&slug.as_str()))
            .map(|slug| category::Category::new(&slug, client, cache))
            .collect()
    }
}

#[derive(Deserialize)]
struct CategoryFields {
    #[serde(rename = "seoToken")]
    seo_token: String,
}

fn get_setup_data(client: &ColesHttpClient) -> Result<(String, String), Box<dyn StdError>> {
    let resp = client.get_setup_data()?;
    let selector = Selector::parse("script#__NEXT_DATA__")?;
    let doc = scraper::Html::parse_document(&resp);
    let next_data_script = match doc.select(&selector).next() {
        Some(x) => x,
        None => return Err("couldn't find __NEXT_DATA__ script in HTML".into()),
    };
    let next_data_script = next_data_script.inner_html();
    let next_data: NextData = serde_json::from_str(&next_data_script)?;
    let api_key = next_data.runtime_config.bff_api_subscription_key;
    let version = next_data.build_id;

    Ok((api_key, version))
}

fn get_versioned_client(client: &ColesHttpClient) -> Result<ColesHttpClient, Box<dyn StdError>> {
    let (api_key, version) = get_setup_data(client)?;
    let client = ColesHttpClient::new_with_setup(&api_key, version)?;
    Ok(client)
}

fn get_categories<'a>(
    cache: &'a FsCache,
    client: &'a ColesHttpClient,
) -> Result<Vec<category::Category<'a>>, Box<dyn StdError>> {
    let resp = client.get_categories()?;
    let categories: Categories = serde_json::from_str(&resp)?;
    let categories = categories.get_items(client, cache);

    Ok(categories)
}

pub fn fetch(cache: &FsCache, quick: bool) {
    log::info!("Starting fetch for coles");
    let client = ColesHttpClient::new().unwrap();
    let client = get_versioned_client(&client).unwrap();
    let categories = get_categories(cache, &client).unwrap();
    println!("{}", categories.len());
    for category in categories {
        for prod in category {
            let prod = prod.unwrap();
            let prod: SearchResult = match SearchResult::from_json_value(prod) {
                Ok(product) => product,
                Err(error) => {
                    match error {
                        // just skip ads silently
                        Error::AdResult => continue,
                        _ => {
                            info!("Failed to convert json value to search result, skipping. Error was {}", error);
                            continue;
                        }
                    }
                }
            };

            let _prod: Product = match prod.try_into() {
                Ok(product) => product,
                Err(error) => {
                    info!(
                        "Failed to convert search result to product, skipping. Error was {}",
                        error
                    );
                    continue;
                }
            };
        }
        if quick {
            break;
        }
    }
}

pub fn compress(source: &PathBuf) {
    let mut file = source.clone();
    file.set_extension("tar.gz");
    info!("Saving results as {}", file.to_str().unwrap());
    let file = File::create(file).unwrap();
    let file = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(file);
    // saves everything relative to source
    archive.append_dir_all("", source).unwrap();
    archive.finish().unwrap();
}

#[cfg(test)]
mod test {
    use super::*;
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
}

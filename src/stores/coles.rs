mod category;
mod http;

#[double]
use crate::cache::FsCache;
#[double]
use http::ColesHttpClient;
use log;
use mockall_double::double;
use scraper::Selector;
use serde::Deserialize;
use std::error::Error;

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

fn get_cache_key(key: &str) -> String {
    format!("coles/{}", key)
}

fn get_setup_data(
    client: &ColesHttpClient,
    cache: &FsCache,
) -> Result<(String, String), Box<dyn Error>> {
    let path = get_cache_key("index.html");
    let resp = cache.get_or_fetch(path, &|| client.get_setup_data())?;
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

fn get_versioned_client<'a>(
    cache: &'a FsCache,
    client: &ColesHttpClient,
) -> Result<ColesHttpClient, Box<dyn Error>> {
    let (api_key, version) = get_setup_data(client, &cache)?;
    let client = ColesHttpClient::new_with_setup(&api_key, version)?;
    Ok(client)
}

fn get_categories<'a>(
    cache: &'a FsCache,
    client: &'a ColesHttpClient,
) -> Result<Vec<category::Category<'a>>, Box<dyn Error>> {
    let path = get_cache_key("categories.json");
    let resp = cache.get_or_fetch(path, &|| client.get_categories())?;
    let categories: Categories = serde_json::from_str(&resp)?;
    let categories = categories.get_items(client, cache);

    Ok(categories)
}

pub fn fetch(cache: &FsCache) {
    log::info!("Starting fetch for coles");
    let client = ColesHttpClient::new().unwrap();
    let client = get_versioned_client(cache, &client).unwrap();
    let categories = get_categories(cache, &client).unwrap();
    let mut counter = 0;
    println!("{}", categories.len());
    for category in categories {
        for prod in category {
            let _prod = prod.unwrap();
            counter += 1;
            if counter == 1 {
                // println!("{:#?}", _prod);
                return ();
            }
            println!("{}", counter);
            // break;
        }
    }
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
        let mut cache = FsCache::default();
        cache.expect_get_or_fetch().returning(|file, fetch| {
            Ok(fetch()?)
        });
        let (api_key, version) = get_setup_data(&client, &cache).expect("Expected success");
        assert_eq!(version, "20240101.01_v1.01.0");
        assert_eq!(api_key, "testsubkey");
    }
}

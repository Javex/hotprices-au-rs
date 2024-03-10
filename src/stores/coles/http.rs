use reqwest::header::{self, InvalidHeaderValue};
use reqwest::header::{HeaderMap, HeaderValue};
use std::error::Error;

const BASE_URL: &str = "https://www.coles.com.au";
const URL_HEADER: HeaderValue = HeaderValue::from_static(BASE_URL);
const USER_AGENT: HeaderValue = HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36");
const STORE_ID: &str = "0584";

pub trait HttpClient {
    fn get(self: &Self, url: &str) -> reqwest::Result<String>;
    fn get_setup_data(&self) -> reqwest::Result<String>;
    fn get_categories(&self) -> reqwest::Result<String>;
    fn get_category(&self, slug: &str, page: i32) -> reqwest::Result<String>;
}

pub struct ColesHttpClient {
    client: reqwest::blocking::Client,
    version: Option<String>,
}

impl ColesHttpClient {
    pub fn new(api_key: Option<&str>, version: Option<String>) -> Result<ColesHttpClient, Box<dyn Error>> {
        let headers = Self::get_headers(api_key)?;
        let builder = reqwest::blocking::Client::builder()
            .cookie_store(true)
            .default_headers(headers);
        let client = builder.build()?;
        Ok(ColesHttpClient {
            client,
            version,
        })
    }

    fn get_headers(api_key: Option<&str>) -> Result<HeaderMap, InvalidHeaderValue> {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, USER_AGENT);
        headers.insert(header::ORIGIN, URL_HEADER);
        headers.insert(header::REFERER, URL_HEADER);
        if let Some(api_key) = api_key {
            headers.insert("ocp-apim-subscription-key", HeaderValue::from_str(api_key)?);
        }
        Ok(headers)
    }
}

impl HttpClient for ColesHttpClient {
    fn get(&self, url: &str) -> reqwest::Result<String> {
        log::debug!("Loading url '{url}'");
        self.client.get(url).send()?.error_for_status()?.text()
    }

    fn get_setup_data(&self) -> reqwest::Result<String> {
        self.get(BASE_URL)
    }

    fn get_categories(&self) -> reqwest::Result<String> {
        let cat_url = format!("{BASE_URL}/api/bff/products/categories?storeId={STORE_ID}");
        self.get(&cat_url)
    }

    fn get_category(&self, slug: &str, page: i32) -> reqwest::Result<String> {
        let version = &self.version.as_ref().expect("Must set version");
        let url = format!(
            "{BASE_URL}/_next/data/{version}/en/browse/{slug}.json?page={page}&slug={slug}"
        );
        self.get(&url)
    }
}

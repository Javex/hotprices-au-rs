use crate::errors::{Error, Result};
use mockall::automock;
use reqwest::header::{self, HeaderMap, HeaderValue};

const BASE_URL: &str = "https://www.coles.com.au";
const URL_HEADER: HeaderValue = HeaderValue::from_static(BASE_URL);
const USER_AGENT: HeaderValue = HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36");
const STORE_ID: &str = "0584";

pub struct ColesHttpClient {
    client: reqwest::blocking::Client,
    version: Option<String>,
}

#[automock]
impl ColesHttpClient {
    pub fn new() -> Result<Self> {
        let headers = Self::get_headers(None)?;
        let builder = reqwest::blocking::Client::builder()
            .cookie_store(true)
            .default_headers(headers);
        let client = builder.build()?;
        Ok(ColesHttpClient {
            client,
            version: None,
        })
    }

    pub fn new_with_setup(api_key: &str, version: String) -> Result<Self> {
        let headers = Self::get_headers(Some(api_key))?;
        let builder = reqwest::blocking::Client::builder()
            .cookie_store(true)
            .default_headers(headers);
        let client = builder.build()?;
        Ok(ColesHttpClient {
            client,
            version: Some(version),
        })
    }

    fn get_headers<'a>(api_key: Option<&'a str>) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(header::USER_AGENT, USER_AGENT);
        headers.insert(header::ORIGIN, URL_HEADER);
        headers.insert(header::REFERER, URL_HEADER);
        if let Some(api_key) = api_key {
            headers.insert("ocp-apim-subscription-key", HeaderValue::from_str(api_key)?);
        }
        Ok(headers)
    }

    fn get(&self, url: &str) -> Result<String> {
        log::debug!("Loading url '{url}'");
        Ok(self.client.get(url).send()?.error_for_status()?.text()?)
    }

    pub fn get_setup_data(&self) -> Result<String> {
        self.get(BASE_URL)
    }

    pub fn get_categories(&self) -> Result<String> {
        let cat_url = format!("{BASE_URL}/api/bff/products/categories?storeId={STORE_ID}");
        self.get(&cat_url)
    }

    pub fn get_category(&self, slug: &str, page: i32) -> Result<String> {
        let version = &self
            .version
            .as_ref()
            .ok_or(Error::Message("Must set version".to_string()))?;
        let url = format!(
            "{BASE_URL}/_next/data/{version}/en/browse/{slug}.json?page={page}&slug={slug}"
        );
        self.get(&url)
    }
}

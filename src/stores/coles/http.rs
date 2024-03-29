use std::time::Duration;

use anyhow::anyhow;
use cookie_store::CookieStore;
#[cfg(test)]
use mockall::automock;

use crate::retry::RetryPolicy;

const BASE_URL: &str = "https://www.coles.com.au";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36";
const STORE_ID: &str = "0584";

pub(crate) struct ColesHttpClient {
    client: ureq::Agent,
    version: Option<String>,
    api_key: Option<String>,
    retry_policy: RetryPolicy,
}

#[cfg_attr(test, automock)]
#[allow(dead_code)]
impl ColesHttpClient {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Self::new_client(None, None)
    }

    pub(crate) fn new_with_setup(api_key: &str, version: String) -> anyhow::Result<Self> {
        Self::new_client(Some(String::from(api_key)), Some(version))
    }

    fn new_client(api_key: Option<String>, version: Option<String>) -> anyhow::Result<Self> {
        let cookie_store = CookieStore::new(None);
        let client = ureq::builder()
            .cookie_store(cookie_store)
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build();
        Ok(ColesHttpClient {
            client,
            version,
            api_key,
            retry_policy: RetryPolicy::default(),
        })
    }

    fn get(&self, url: &str) -> anyhow::Result<String> {
        log::info!("Loading url '{url}'");
        let response = self.retry_policy.retry(|| {
            let request = self
                .client
                .get(url)
                .set("Origin", BASE_URL)
                .set("Referer", BASE_URL);
            let request = match &self.api_key {
                Some(api_key) => request.set("ocp-apim-subscription-key", api_key),
                None => request,
            };
            request.call()
        })?;

        Ok(response.into_string()?)
    }

    pub(crate) fn get_setup_data(&self) -> anyhow::Result<String> {
        self.get(BASE_URL)
    }

    pub(crate) fn get_categories(&self) -> anyhow::Result<String> {
        let cat_url = format!("{BASE_URL}/api/bff/products/categories?storeId={STORE_ID}");
        self.get(&cat_url)
    }

    pub(crate) fn get_category(&self, slug: &str, page: i32) -> anyhow::Result<String> {
        let version = &self
            .version
            .as_ref()
            .ok_or_else(|| anyhow!("Must set version"))?;
        let url = format!(
            "{BASE_URL}/_next/data/{version}/en/browse/{slug}.json?page={page}&slug={slug}"
        );
        self.get(&url)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new_unconfigured_fails_get_category() {
        let client = ColesHttpClient::new().unwrap();
        let res = client.get_category("", 0).unwrap_err();
        assert_eq!(res.to_string(), "Must set version");
    }

    #[test]
    fn new_with_setup() {
        ColesHttpClient::new_with_setup("", String::new()).unwrap();
    }
}

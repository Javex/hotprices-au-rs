use std::{thread, time::Duration};

use crate::errors::{Error, Result};
use cookie_store::CookieStore;
use log::{error, info};
use mockall::automock;

const BASE_URL: &str = "https://www.coles.com.au";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36";
const STORE_ID: &str = "0584";

struct RetryPolicy {
    total: u32,
    max_backoff: Duration,
}

impl RetryPolicy {
    fn get_backoff_time(&self, retry_count: u32) -> Duration {
        let backoff_value = Duration::from_secs(2u64.pow(retry_count));
        if backoff_value > self.max_backoff {
            self.max_backoff
        } else {
            backoff_value
        }
    }
}

pub struct ColesHttpClient {
    client: ureq::Agent,
    version: Option<String>,
    api_key: Option<String>,
    retry_policy: RetryPolicy,
}

#[automock]
#[allow(dead_code)]
impl ColesHttpClient {
    pub fn new() -> Result<Self> {
        Self::new_client(None, None)
    }

    pub fn new_with_setup(api_key: &str, version: String) -> Result<Self> {
        Self::new_client(Some(String::from(api_key)), Some(version))
    }

    fn new_client(api_key: Option<String>, version: Option<String>) -> Result<Self> {
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
            retry_policy: RetryPolicy {
                total: 10,
                max_backoff: Duration::from_secs(120),
            },
        })
    }

    fn get(&self, url: &str) -> Result<String> {
        log::info!("Loading url '{url}'");
        for retry_count in 0..self.retry_policy.total {
            let request = self
                .client
                .get(url)
                .set("Origin", BASE_URL)
                .set("Referer", BASE_URL);
            let request = match &self.api_key {
                Some(api_key) => request.set("ocp-apim-subscription-key", api_key),
                None => request,
            };

            let response = match request.call() {
                Ok(response) => response,
                Err(error) => {
                    if retry_count < self.retry_policy.total - 1 {
                        let sleep_time = self.retry_policy.get_backoff_time(retry_count);
                        info!(
                            "Retrying request after {} seconds due to error {}",
                            sleep_time.as_secs(),
                            error
                        );
                        thread::sleep(sleep_time);
                        continue;
                    }

                    error!(
                        "Failed request after {} retries, giving up due to error {}",
                        retry_count, error
                    );
                    return Err(Error::Http(Box::new(error)));
                }
            };

            return Ok(response.into_string()?);
        }
        panic!("Ended retry loop unexpectedly");
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

#[cfg(test)]
mod test {
    use core::panic;

    use super::*;

    #[test]
    fn new_unconfigured_fails_get_category() {
        let client = ColesHttpClient::new().unwrap();
        let res = client.get_category("", 0).unwrap_err();
        match res {
            Error::Message(m) => assert_eq!(m, "Must set version"),
            e => panic!("Unexpected error type: {}", e),
        }
    }

    #[test]
    fn new_with_setup() {
        ColesHttpClient::new_with_setup("", String::from("")).unwrap();
    }
}

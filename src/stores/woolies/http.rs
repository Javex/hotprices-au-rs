use std::num::NonZeroU32;
use std::result::Result as StdResult;
use std::{thread, time::Duration};

use cookie_store::CookieStore;
use log::{error, info};
#[cfg(test)]
use mockall::automock;

const BASE_URL: &str = "https://www.woolworths.com.au";
const REFERER: &str = "https://www.woolworths.com.au/shop/browse/fruit-veg";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36";

struct RetryPolicy {
    total: NonZeroU32,
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

    fn retry<F>(&self, request: F) -> StdResult<ureq::Response, anyhow::Error>
    where
        F: Fn() -> StdResult<ureq::Response, ureq::Error>,
    {
        for retry_count in 0..self.total.get() {
            let response = match request() {
                Ok(response) => response,
                Err(error) => {
                    if retry_count < self.total.get() - 1 {
                        let sleep_time = self.get_backoff_time(retry_count);
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
                    return Err(anyhow::Error::new(error)
                        .context(format!("Failed request after {retry_count} retries")));
                }
            };

            return Ok(response);
        }
        panic!("Ended retry loop unexpectedly");
    }
}

#[cfg(test)]
mod test_retry {
    use super::*;

    #[test]
    fn test_no_retry() {
        let policy = RetryPolicy {
            total: NonZeroU32::new(1).unwrap(),
            max_backoff: Duration::from_secs(0),
        };

        let retry_counter = std::cell::RefCell::new(0);
        let result = policy
            .retry(|| {
                *retry_counter.borrow_mut() += 1;
                Ok(ureq::Response::new(200, "OK", "")?)
            })
            .unwrap();
        assert_eq!(result.status(), 200);
        assert_eq!(retry_counter.into_inner(), 1);
    }

    #[test]
    fn test_retry_once() {
        let policy = RetryPolicy {
            total: NonZeroU32::new(2).unwrap(),
            max_backoff: Duration::from_secs(0),
        };

        let retry_counter = std::cell::RefCell::new(0);
        let result = policy
            .retry(|| {
                let mut retry_counter_ref = retry_counter.borrow_mut();
                *retry_counter_ref += 1;
                if *retry_counter_ref == 1 {
                    Err(ureq::Error::Status(
                        500,
                        ureq::Response::new(500, "Internal Server Error", "")?,
                    ))
                } else {
                    Ok(ureq::Response::new(200, "OK", "")?)
                }
            })
            .unwrap();
        assert_eq!(result.status(), 200);
        assert_eq!(retry_counter.into_inner(), 2);
    }

    #[test]
    fn test_retry_fail() {
        let policy = RetryPolicy {
            total: NonZeroU32::new(2).unwrap(),
            max_backoff: Duration::from_secs(0),
        };

        let retry_counter = std::cell::RefCell::new(0);
        let err = policy
            .retry(|| {
                let mut retry_counter_ref = retry_counter.borrow_mut();
                *retry_counter_ref += 1;
                Err(ureq::Error::Status(
                    500,
                    ureq::Response::new(500, "Internal Server Error", "")?,
                ))
            })
            .unwrap_err();
        let err: ureq::Error = err.downcast().unwrap();
        let status = match err {
            ureq::Error::Status(status, _) => status,
            _ => panic!("Unexpected error invariant"),
        };
        assert_eq!(status, 500);
        assert_eq!(retry_counter.into_inner(), 2);
    }
}

pub(crate) struct WooliesHttpClient {
    client: ureq::Agent,
    retry_policy: RetryPolicy,
}

#[cfg_attr(test, automock)]
#[allow(dead_code)]
impl WooliesHttpClient {
    pub(crate) fn new() -> Self {
        let cookie_store = CookieStore::new(None);
        let client = ureq::builder()
            .cookie_store(cookie_store)
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build();
        WooliesHttpClient {
            client,
            retry_policy: RetryPolicy {
                total: NonZeroU32::new(10).unwrap(),
                max_backoff: Duration::from_secs(120),
            },
        }
    }

    pub(crate) fn start(&self) -> anyhow::Result<()> {
        self.get(BASE_URL)?;
        Ok(())
    }

    fn get(&self, url: &str) -> anyhow::Result<String> {
        log::info!("Loading url '{url}'");
        let response = self.retry_policy.retry(|| {
            let request = self
                .client
                .get(url)
                .set("Origin", BASE_URL)
                .set("Referer", REFERER);
            request.call()
        })?;
        return Ok(response.into_string()?);
    }

    pub(crate) fn get_categories(&self) -> anyhow::Result<String> {
        let cat_url = format!("{BASE_URL}/apis/ui/PiesCategoriesWithSpecials");
        self.get(&cat_url)
    }

    pub(crate) fn get_category(&self, id: &str, page: i32) -> anyhow::Result<String> {
        let url = format!("{BASE_URL}/apis/ui/browse/category");
        log::info!("Loading url '{url}'");
        let response = self.retry_policy.retry(|| {
            self.client
                .post(&url)
                .set("Origin", BASE_URL)
                .set("Referer", REFERER)
                .send_json(ureq::json!({
                    "categoryId": id,
                    "pageNumber": page,
                    "pageSize": 36,
                    "sortType": "Name",
                    "url": "/shop/browse/fruit-veg",
                    "location": "/shop/browse/fruit-veg",
                    "formatObject": "{\"name\":\"Fruit & Veg\"}",
                    "isSpecial": false,
                    "isBundle": false,
                    "isMobile": false,
                    "filters": [
                        {
                            "Items": [
                                {
                                    "Term": "Woolworths"
                                }
                            ],
                            "Key": "SoldBy"
                        }
                    ],
                    "token": "",
                    "gpBoost": 0,
                    "isHideUnavailableProducts": false,
                    "enableAdReRanking": false,
                    "groupEdmVariants": true,
                    "categoryVersion": "v2"
                }))
        })?;

        return Ok(response.into_string()?);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_new() {
        WooliesHttpClient::new();
    }
}

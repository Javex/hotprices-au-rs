use std::time::Duration;

use cookie_store::CookieStore;
#[cfg(test)]
use mockall::automock;

use crate::retry::RetryPolicy;

const BASE_URL: &str = "https://www.woolworths.com.au";
const REFERER: &str = "https://www.woolworths.com.au/shop/browse/fruit-veg";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36";

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
            retry_policy: RetryPolicy::default(),
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
        Ok(response.into_string()?)
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

        Ok(response.into_string()?)
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

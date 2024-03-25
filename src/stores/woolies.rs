use crate::cache::FsCache;
#[double]
use crate::stores::woolies::http::WooliesHttpClient;
use log::{debug, info};
use mockall_double::double;
use serde::Deserialize;

mod category;
mod http;
pub mod product;

#[derive(Deserialize)]
struct CategoryListItem {
    #[serde(rename = "NodeId")]
    node_id: String,
    #[serde(rename = "Description")]
    description: String,
}

impl CategoryListItem {
    fn is_filtered(&self) -> bool {
        if self.node_id == "specialsgroup" {
            return true;
        }

        if self.description == "Front of Store" {
            return true;
        }

        if self.description == "Beer, Wine & Spirits" || self.node_id == "1_8E4DA6F" {
            return true;
        }

        false
    }
}

#[derive(Deserialize)]
struct CategoriesResponse {
    #[serde(rename = "Categories")]
    categories: Vec<CategoryListItem>,
}

impl CategoriesResponse {
    pub fn get_items<'a>(
        &self,
        client: &'a WooliesHttpClient,
        cache: &'a FsCache,
    ) -> Vec<category::Category<'a>> {
        self.categories
            .iter()
            .filter(|c| !c.is_filtered())
            .map(|c| category::Category::new(c.node_id.clone(), client, cache))
            .collect()
    }
}

fn get_categories<'a>(
    cache: &'a FsCache,
    client: &'a WooliesHttpClient,
) -> anyhow::Result<Vec<category::Category<'a>>> {
    let resp = client.get_categories()?;
    let categories: CategoriesResponse = serde_json::from_str(&resp)?;
    let categories = categories.get_items(client, cache);
    Ok(categories)
}

pub fn fetch(cache: &FsCache, quick: bool) -> anyhow::Result<()> {
    info!("Starting fetch for woolies");
    let client = WooliesHttpClient::new();
    // client.start()?;
    let categories = get_categories(cache, &client)?;
    debug!("Loaded categories for Woolies, have {}", categories.len());
    for category in categories {
        debug!("Got category: {}", category);
        for prod in category {
            let _prod = prod?;
            // debug!("Got product: {:?}", prod);
        }
    }
    Ok(())
}

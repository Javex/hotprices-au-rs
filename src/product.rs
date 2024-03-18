use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::errors::Result;
use crate::{stores::Store, unit::Unit};

#[derive(Debug, Serialize, Deserialize)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub price: f64,
    pub price_history: Vec<PriceHistory>,
    pub is_weighted: bool,
    pub unit: Unit,
    pub quantity: f64,
    pub store: Store,
}

impl Product {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i64,
        name: String,
        description: String,
        price: f64,
        is_weighted: Option<bool>,
        unit: Unit,
        quantity: f64,
        store: Store,
    ) -> Self {
        let price_history = vec![PriceHistory {
            date: OffsetDateTime::now_utc().date(),
            price,
        }];

        Self {
            id,
            name,
            description,
            price,
            price_history,
            is_weighted: is_weighted.unwrap_or(false),
            unit,
            quantity,
            store,
        }
    }
}

use crate::date::date_serde;

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceHistory {
    #[serde(with = "date_serde")]
    pub date: Date,
    pub price: f64,
}

pub fn merge_price_history(
    old_items: Vec<Product>,
    mut new_items: Vec<Product>,
    store_filter: Option<Store>,
) -> Result<Vec<Product>> {
    if store_filter.is_some() {
        todo!("Not implemented");
    }

    let mut old_map = HashMap::with_capacity(old_items.len());
    for item in old_items.into_iter() {
        old_map.insert(item.id, item);
    }

    for new in new_items.iter_mut() {
        if let Some(old) = old_map.remove(&new.id) {
            let new_price = new
                .price_history
                .first()
                .expect("new item should have price")
                .price;
            let last_price = old
                .price_history
                .first()
                .expect("old item should have price")
                .price;
            if new_price != last_price {
                new.price_history.extend(old.price_history);
            } else {
                new.price_history = old.price_history;
            }
        }
    }

    // Add old items that didn't have new ites to result
    new_items.extend(old_map.into_values());

    Ok(new_items)
}

#[cfg(test)]
mod test {
    use time::{Date, Month};

    use crate::{stores::Store, unit::Unit};

    use super::{merge_price_history, PriceHistory, Product};

    impl Default for Product {
        fn default() -> Self {
            Self {
                id: 1,
                name: String::from("test name"),
                description: String::from("test description"),
                price: 1.0,
                price_history: vec![PriceHistory::default()],
                is_weighted: false,
                unit: Unit::Grams,
                quantity: 1.0,
                store: Store::Coles,
            }
        }
    }

    impl Default for PriceHistory {
        fn default() -> Self {
            Self {
                date: Date::from_calendar_date(2024, Month::January, 10).expect("valid date"),
                price: 1.0,
            }
        }
    }

    #[test]
    fn it_merges() {
        let old = vec![Product {
            price_history: vec![PriceHistory {
                date: Date::from_calendar_date(2024, Month::January, 10)
                    .expect("should be valid date"),
                price: 1.0,
            }],
            ..Default::default()
        }];

        let new = vec![Product {
            price_history: vec![PriceHistory {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 0.5,
            }],
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None).expect("should succeed");
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(merged.price_history.len(), 2);
        let [ref newest_price, ref oldest_price] = merged.price_history[..] else {
            panic!("unexpected price history size")
        };
        assert_eq!(newest_price.price, 0.5);
        assert_eq!(oldest_price.price, 1.0);
    }

    #[test]
    fn it_matches_products() {
        let old = vec![Product {
            id: 1,
            ..Default::default()
        }];

        let new = vec![Product {
            id: 2,
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None).expect("should succeed");
        let product_ids: Vec<i64> = merged.iter().map(|p| p.id).collect();
        assert!(product_ids.contains(&1));
        assert!(product_ids.contains(&2));
    }

    #[test]
    fn it_skips_unchanged_price() {
        let old = vec![Product {
            price_history: vec![PriceHistory {
                date: Date::from_calendar_date(2024, Month::January, 10)
                    .expect("should be valid date"),
                price: 1.0,
            }],
            ..Default::default()
        }];

        let new = vec![Product {
            price_history: vec![PriceHistory {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 1.0,
            }],
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None).expect("should succeed");
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(merged.price_history.len(), 1);
        let [ref price_history] = merged.price_history[..] else {
            panic!("unexpected price history size")
        };
        assert_eq!(price_history.price, 1.0);
    }

    #[test]
    fn it_preserves_history_on_unchanged_price() {
        let old = vec![Product {
            price_history: vec![
                PriceHistory {
                    date: Date::from_calendar_date(2024, Month::January, 10)
                        .expect("should be valid date"),
                    price: 1.0,
                },
                PriceHistory {
                    date: Date::from_calendar_date(2024, Month::January, 9)
                        .expect("should be valid date"),
                    price: 0.5,
                },
            ],
            ..Default::default()
        }];

        let new = vec![Product {
            price_history: vec![PriceHistory {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 1.0,
            }],
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None).expect("should succeed");
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(merged.price_history.len(), 2);
        let [ref latest_price, ref old_price] = merged.price_history[..] else {
            panic!("unexpected price history size")
        };
        assert_eq!(latest_price.price, 1.0);
        assert_eq!(old_price.price, 0.5);
    }
}

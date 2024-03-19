use std::cmp::Ordering;
use std::collections::HashMap;

use log::info;
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

    pub fn add_history(&mut self, extra_history: Vec<PriceHistory>) -> bool {
        // if this assertion triggers it might be worth considering a different design that
        // enforces a difference between a new product snapshot (with history length of one) and
        // old product snapshots with real history at compile time using the type system.
        assert_eq!(
            self.price_history.len(),
            1,
            "To append history the current product should have only one \"history\""
        );

        let last_price = extra_history
            .first()
            .expect("need at least one price in extra_history")
            .price;

        let new_price = self
            .price_history
            .first()
            .expect("new item should have price")
            .price;

        let has_new_price = if new_price != last_price {
            // Append history
            self.price_history.extend(extra_history);
            true
        } else {
            self.price_history = extra_history;
            false
        };

        // Make sure elements are sorted
        self.price_history.sort();

        // return whether prices were updated
        has_new_price
    }
}

use crate::date::date_serde;

#[derive(Debug, Serialize, Deserialize)]
pub struct PriceHistory {
    #[serde(with = "date_serde")]
    pub date: Date,
    pub price: f64,
}

impl Ord for PriceHistory {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.date.cmp(&other.date) {
            Ordering::Less => Ordering::Greater,
            Ordering::Greater => Ordering::Less,
            Ordering::Equal => Ordering::Equal,
        }
    }
}

impl PartialOrd for PriceHistory {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for PriceHistory {
    fn eq(&self, other: &Self) -> bool {
        self.date.eq(&other.date)
    }
}

impl Eq for PriceHistory {}

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
        old_map.insert((item.store, item.id), item);
    }

    let mut store_price_count: HashMap<&Store, u64> = HashMap::new();
    for new in new_items.iter_mut() {
        if let Some(old) = old_map.remove(&(new.store, new.id)) {
            let has_new_price = new.add_history(old.price_history);

            // Track new prices
            if has_new_price {
                *store_price_count.entry(&new.store).or_insert(0) += 1;
            }
        }
    }

    if !old_map.is_empty() {
        info!("{} products not in latest product list", old_map.len());
    }

    for (store, count) in store_price_count {
        info!("Store '{store}' has {count} new prices");
    }

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
        assert!(!product_ids.contains(&1));
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

    #[test]
    fn it_updates() {
        let old = vec![Product {
            price_history: vec![PriceHistory {
                date: Date::from_calendar_date(2024, Month::January, 10)
                    .expect("should be valid date"),
                price: 1.0,
            }],
            ..Default::default()
        }];

        let new = vec![Product {
            name: String::from("New name"),
            price_history: vec![PriceHistory {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 0.5,
            }],
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None).expect("should return one product");
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(merged.name, "New name");
    }

    #[test]
    fn it_has_no_old_products() {
        let old: Vec<Product> = Vec::new();
        let new = vec![Product::default()];
        let merged = merge_price_history(old, new, None).expect("should just return new");
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn it_removes_old_missing_products() {
        let old: Vec<Product> = vec![Product {
            id: 1,
            ..Default::default()
        }];
        let new = vec![Product {
            id: 2,
            ..Default::default()
        }];
        let merged = merge_price_history(old, new, None).expect("should just return new");
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };
        assert_eq!(merged.id, 2);
    }

    #[test]
    fn price_history_order() {
        let old_date =
            Date::from_calendar_date(2024, Month::January, 10).expect("should be valid date");
        let old = PriceHistory {
            date: old_date,
            ..Default::default()
        };
        let new_date =
            Date::from_calendar_date(2024, Month::January, 11).expect("should be valid date");
        let new = PriceHistory {
            date: new_date,
            ..Default::default()
        };

        // Newer elements are *smaller* than older because we want the latest price as the first
        // elements and that's the largest date
        assert!(new < old);

        let mut items = vec![old, new];
        items.sort();
        let [ref first, ref second] = items[..] else {
            panic!("unexpected")
        };
        assert_eq!(first.date, new_date);
        assert_eq!(second.date, old_date);
    }
}

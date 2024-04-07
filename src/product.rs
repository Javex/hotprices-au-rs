use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use log::info;
use nonempty::{nonempty, NonEmpty};
use serde::{self, Deserialize, Serialize};
use time::Date;

use crate::category::Category;

use crate::category::CategoryCode;
use crate::{stores::Store, unit::Unit};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProductInfo {
    id: i64,
    name: String,
    description: String,
    #[serde(rename = "isWeighted")]
    is_weighted: bool,
    unit: Unit,
    quantity: f64,
    store: Store,
    #[serde(flatten)]
    category: Option<CategoryCode>,
}

impl ProductInfo {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: i64,
        name: String,
        description: String,
        is_weighted: Option<bool>,
        unit: Unit,
        quantity: f64,
        store: Store,
        category: Option<CategoryCode>,
    ) -> Self {
        Self {
            id,
            name,
            description,
            is_weighted: is_weighted.unwrap_or(false),
            unit,
            quantity,
            store,
            category,
        }
    }
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug)]
pub(crate) struct ProductSnapshot {
    product_info: ProductInfo,
    price_snapshot: PriceSnapshot,
}

impl ProductSnapshot {
    pub(crate) fn new(product_info: ProductInfo, price: Price, date: Date) -> Self {
        Self {
            product_info,
            price_snapshot: PriceSnapshot { date, price },
        }
    }

    pub(crate) fn id(&self) -> i64 {
        self.product_info.id
    }

    #[cfg(test)]
    pub(crate) fn name(&self) -> &str {
        self.product_info.name.as_str()
    }

    #[cfg(test)]
    pub(crate) fn description(&self) -> &str {
        self.product_info.description.as_str()
    }

    #[cfg(test)]
    pub(crate) fn is_weighted(&self) -> bool {
        self.product_info.is_weighted
    }

    pub(crate) fn store(&self) -> Store {
        self.product_info.store
    }

    #[cfg(test)]
    pub(crate) fn unit(&self) -> Unit {
        self.product_info.unit
    }

    #[cfg(test)]
    pub(crate) fn quantity(&self) -> f64 {
        self.product_info.quantity
    }

    pub(crate) fn category(&self) -> Option<Category> {
        self.product_info.category.as_ref().map(|v| v.category)
    }

    pub(crate) fn price(&self) -> Price {
        self.price_snapshot.price
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProductHistory {
    #[serde(flatten)]
    product_info: ProductInfo,
    #[serde(rename = "priceHistory")]
    price_history: NonEmpty<PriceSnapshot>,
}

impl ProductHistory {
    pub(crate) fn update_from_snapshot(&mut self, snapshot: ProductSnapshot) -> bool {
        let new_price = snapshot.price();
        self.product_info = snapshot.product_info;

        let has_new_price = if self.price_history.first().price != new_price {
            self.price_history.insert(0, snapshot.price_snapshot);
            true
        } else {
            false
        };

        // Make sure elements are sorted
        self.price_history.sort();

        // return whether prices were updated
        has_new_price
    }

    pub(crate) fn id(&self) -> i64 {
        self.product_info.id
    }

    pub(crate) fn store(&self) -> Store {
        self.product_info.store
    }
}

impl From<ProductSnapshot> for ProductHistory {
    fn from(product_snapshot: ProductSnapshot) -> Self {
        Self {
            product_info: product_snapshot.product_info,
            price_history: nonempty![product_snapshot.price_snapshot],
        }
    }
}

use crate::date::date_serde;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PriceSnapshot {
    #[serde(with = "date_serde")]
    date: Date,
    #[serde(with = "price_serde")]
    price: Price,
}

impl Ord for PriceSnapshot {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.date.cmp(&other.date) {
            Ordering::Less => Ordering::Greater,
            Ordering::Greater => Ordering::Less,
            Ordering::Equal => Ordering::Equal,
        }
    }
}

impl PartialOrd for PriceSnapshot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for PriceSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.date.eq(&other.date)
    }
}

impl Eq for PriceSnapshot {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Price {
    price: i32,
}

impl From<f64> for Price {
    fn from(price: f64) -> Self {
        Self {
            price: (price * 100.0).round() as i32,
        }
    }
}

pub(crate) mod price_serde {
    use super::Price;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S>(price: &Price, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let price = f64::from(price.price) / 100.0;
        serializer.serialize_f64(price)
    }

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Price, D::Error>
    where
        D: Deserializer<'de>,
    {
        let price = f64::deserialize(deserializer)?;
        Ok(price.into())
    }
}

pub(crate) fn merge_price_history(
    old_items: Vec<ProductHistory>,
    new_items: Vec<ProductSnapshot>,
    store_filter: Option<Store>,
) -> Vec<ProductHistory> {
    let mut result: Vec<ProductHistory> = Vec::with_capacity(new_items.len());

    let mut old_map = HashMap::with_capacity(old_items.len());
    for item in old_items {
        if store_filter.is_some_and(|s| s != item.store()) {
            result.push(item);
        } else {
            old_map.insert((item.store(), item.id()), item);
        }
    }

    let mut store_price_count: HashMap<Store, u64> = HashMap::new();
    for new in new_items {
        if let Some(mut old) = old_map.remove(&(new.store(), new.id())) {
            let has_new_price = old.update_from_snapshot(new);

            // Track new prices
            if has_new_price {
                *store_price_count.entry(old.store()).or_insert(0) += 1;
            }
            result.push(old);
        } else {
            result.push(new.into());
        }
    }

    if !old_map.is_empty() {
        info!("{} products not in latest product list", old_map.len());
    }

    for (store, count) in store_price_count {
        info!("Store '{store}' has {count} new prices");
    }

    result
}

#[cfg(test)]
mod test_merge_price_history {
    use nonempty::nonempty;
    use time::{Date, Month};

    use crate::{stores::Store, unit::Unit};

    use super::{merge_price_history, PriceSnapshot, ProductHistory, ProductInfo, ProductSnapshot};

    impl Default for ProductInfo {
        fn default() -> Self {
            Self {
                id: 1,
                name: String::from("test name"),
                description: String::from("test description"),
                is_weighted: false,
                unit: Unit::Grams,
                quantity: 1.0,
                store: Store::Coles,
                category: None,
            }
        }
    }

    impl ProductInfo {
        pub(crate) fn with_store(store: Store) -> Self {
            Self {
                store,
                ..Default::default()
            }
        }
    }

    impl Default for PriceSnapshot {
        fn default() -> Self {
            Self {
                date: Date::from_calendar_date(2024, Month::January, 10).expect("valid date"),
                price: 1.0.into(),
            }
        }
    }

    impl Default for ProductHistory {
        fn default() -> Self {
            Self {
                product_info: ProductInfo::default(),
                price_history: nonempty![PriceSnapshot::default()],
            }
        }
    }

    impl ProductHistory {
        pub(crate) fn with_info(product_info: ProductInfo) -> Self {
            Self {
                product_info,
                ..Default::default()
            }
        }
    }

    #[test]
    fn it_merges() {
        let old = vec![ProductHistory {
            price_history: nonempty![PriceSnapshot {
                date: Date::from_calendar_date(2024, Month::January, 10)
                    .expect("should be valid date"),
                price: 1.0.into(),
            }],
            ..Default::default()
        }];

        let new = vec![ProductSnapshot {
            price_snapshot: PriceSnapshot {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 0.5.into(),
            },
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None);
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(
            merged.price_history.len(),
            2,
            "should have merged price history of two snapshots"
        );
        let newest_price = merged.price_history.first();
        let oldest_price = merged.price_history.get(1).unwrap();
        assert_eq!(newest_price.price, 0.5.into());
        assert_eq!(oldest_price.price, 1.0.into());
    }

    #[test]
    fn it_matches_products() {
        let old = vec![ProductHistory {
            product_info: ProductInfo {
                id: 1,
                ..Default::default()
            },
            ..Default::default()
        }];

        let new = vec![ProductSnapshot {
            product_info: ProductInfo {
                id: 2,
                ..Default::default()
            },
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None);
        let product_ids: Vec<i64> = merged.iter().map(|p| p.id()).collect();
        assert!(!product_ids.contains(&1));
        assert!(product_ids.contains(&2));
    }

    #[test]
    fn it_skips_unchanged_price() {
        let old = vec![ProductHistory {
            price_history: nonempty![PriceSnapshot {
                date: Date::from_calendar_date(2024, Month::January, 10)
                    .expect("should be valid date"),
                price: 1.0.into(),
            }],
            ..Default::default()
        }];

        let new = vec![ProductSnapshot {
            price_snapshot: PriceSnapshot {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 1.0.into(),
            },
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None);
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(merged.price_history.len(), 1);
        assert_eq!(merged.price_history.first().price, 1.0.into());
    }

    #[test]
    fn it_preserves_history_on_unchanged_price() {
        let old = vec![ProductHistory {
            price_history: nonempty![
                PriceSnapshot {
                    date: Date::from_calendar_date(2024, Month::January, 10)
                        .expect("should be valid date"),
                    price: 1.0.into(),
                },
                PriceSnapshot {
                    date: Date::from_calendar_date(2024, Month::January, 9)
                        .expect("should be valid date"),
                    price: 0.5.into(),
                },
            ],
            ..Default::default()
        }];

        let new = vec![ProductSnapshot {
            price_snapshot: PriceSnapshot {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 1.0.into(),
            },
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, None);
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(merged.price_history.len(), 2);
        let latest_price = merged.price_history.first();
        let old_price = merged.price_history.get(1).unwrap();
        assert_eq!(latest_price.price, 1.0.into());
        assert_eq!(old_price.price, 0.5.into());
    }

    #[test]
    fn it_updates() {
        let old = vec![ProductHistory {
            price_history: nonempty![PriceSnapshot {
                date: Date::from_calendar_date(2024, Month::January, 10)
                    .expect("should be valid date"),
                price: 1.0.into(),
            }],
            ..Default::default()
        }];

        let new = vec![ProductSnapshot {
            product_info: ProductInfo {
                name: String::from("New name"),
                ..Default::default()
            },
            price_snapshot: PriceSnapshot {
                date: Date::from_calendar_date(2024, Month::January, 11)
                    .expect("should be valid date"),
                price: 0.5.into(),
            },
        }];

        let merged = merge_price_history(old, new, None);
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };

        assert_eq!(merged.product_info.name, "New name");
    }

    #[test]
    fn it_has_no_old_products() {
        let old: Vec<ProductHistory> = Vec::new();
        let new = vec![ProductSnapshot::default()];
        let merged = merge_price_history(old, new, None);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn it_removes_old_missing_products() {
        let old: Vec<ProductHistory> = vec![ProductHistory {
            product_info: ProductInfo {
                id: 1,
                ..Default::default()
            },
            ..Default::default()
        }];
        let new = vec![ProductSnapshot {
            product_info: ProductInfo {
                id: 2,
                ..Default::default()
            },
            ..Default::default()
        }];
        let merged = merge_price_history(old, new, None);
        let [ref merged] = merged[..] else {
            panic!("unexpected result size")
        };
        assert_eq!(merged.id(), 2);
    }

    #[test]
    fn merge_with_store_filter() {
        let old = vec![
            ProductHistory {
                product_info: ProductInfo {
                    store: Store::Coles,
                    ..Default::default()
                },
                ..Default::default()
            },
            ProductHistory {
                product_info: ProductInfo {
                    store: Store::Woolies,
                    ..Default::default()
                },
                ..Default::default()
            },
        ];

        let new = vec![ProductSnapshot {
            product_info: ProductInfo {
                store: Store::Coles,
                ..Default::default()
            },
            ..Default::default()
        }];

        let merged = merge_price_history(old, new, Some(Store::Coles));
        assert_eq!(
            merged.len(),
            2,
            "Should return two products for a store each"
        );
        let merged_stores: Vec<Store> = merged.into_iter().map(|p| p.store()).collect();
        assert!(
            merged_stores.contains(&Store::Woolies),
            "Should retain Woolies items when only merging Coles store"
        );
    }

    #[test]
    fn price_history_order() {
        let old_date =
            Date::from_calendar_date(2024, Month::January, 10).expect("should be valid date");
        let old = PriceSnapshot {
            date: old_date,
            ..Default::default()
        };
        let new_date =
            Date::from_calendar_date(2024, Month::January, 11).expect("should be valid date");
        let new = PriceSnapshot {
            date: new_date,
            ..Default::default()
        };

        // Newer elements are *smaller* than older because we want the latest price as the first
        // elements and that's the largest date
        assert!(new < old);

        let mut items = [old, new];
        items.sort();
        let [ref first, ref second] = items[..] else {
            panic!("unexpected")
        };
        assert_eq!(first.date, new_date);
        assert_eq!(second.date, old_date);
    }
}

pub(crate) fn deduplicate_products(products: Vec<ProductSnapshot>) -> Vec<ProductSnapshot> {
    let mut lookup = HashSet::new();
    let mut dedup_products = Vec::new();
    let mut duplicates = HashMap::new();
    for product in products {
        let product_key = (product.store(), product.id());
        if lookup.contains(&product_key) {
            *duplicates.entry(product.store()).or_insert(0) += 1;
        } else {
            lookup.insert(product_key);
            dedup_products.push(product);
        }
    }

    if !duplicates.is_empty() {
        info!("Deduplicated products: {:?}", duplicates);
    }
    dedup_products
}

#[cfg(test)]
mod test_deduplicate_products {
    use super::deduplicate_products;
    use crate::product::ProductSnapshot;

    #[test]
    fn test_deduplicate() {
        let products = vec![ProductSnapshot::default(), ProductSnapshot::default()];
        assert_eq!(deduplicate_products(products).len(), 1);
    }
}

#[cfg(test)]
mod test_price {
    use super::Price;

    #[test]
    fn test_into() {
        let price: Price = 1.0.into();
        assert_eq!(price.price, 100);
        let price: Price = 0.5.into();
        assert_eq!(price.price, 50);
    }
}

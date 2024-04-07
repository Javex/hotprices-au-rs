use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Category {
    FruitAndVeg(FruitAndVeg),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FruitAndVeg {
    Fruit,
    Veg,
    SaladAndHerbs,
    NutsAndDriedFruits,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CategoryCode {
    #[serde(with = "cat_code_serde")]
    pub category: Category,
}

impl CategoryCode {
    pub(crate) fn from_category(category: Category) -> Self {
        Self { category }
    }
}

mod cat_code_serde {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(category: &Category, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = match category {
            Category::FruitAndVeg(sub) => match sub {
                FruitAndVeg::Fruit => "00",
                FruitAndVeg::Veg => "01",
                FruitAndVeg::SaladAndHerbs => "02",
                FruitAndVeg::NutsAndDriedFruits => "03",
            },
        };
        serializer.serialize_str(s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Category, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        if s.len() != 2 {
            return Err(D::Error::invalid_length(
                s.len(),
                &"exactly two single digit numbers",
            ));
        }

        Ok(match s.as_str() {
            "00" => Category::FruitAndVeg(FruitAndVeg::Fruit),
            "01" => Category::FruitAndVeg(FruitAndVeg::Veg),
            "02" => Category::FruitAndVeg(FruitAndVeg::SaladAndHerbs),
            "03" => Category::FruitAndVeg(FruitAndVeg::NutsAndDriedFruits),
            _ => todo!(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserialize() {
        let c: CategoryCode = serde_json::from_str(
            &serde_json::json!({
                "category": "02",
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            c.category,
            Category::FruitAndVeg(FruitAndVeg::SaladAndHerbs)
        );
    }

    #[test]
    fn deserialize_too_short() {
        let e = serde_json::from_str::<CategoryCode>(
            &serde_json::json!({
                "category": "0",
            })
            .to_string(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("invalid length"));
    }

    #[test]
    fn deserialize_too_long() {
        let e = serde_json::from_str::<CategoryCode>(
            &serde_json::json!({
                "category": "001",
            })
            .to_string(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("invalid length"));
    }

    #[test]
    fn serialize() {
        let c = CategoryCode {
            category: Category::FruitAndVeg(FruitAndVeg::SaladAndHerbs),
        };
        let s = serde_json::to_string(&c).unwrap();
        let exp = serde_json::json!({
            "category": "02"
        })
        .to_string();
        assert_eq!(s, exp);
    }
}

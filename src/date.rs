pub(crate) mod date_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::result::Result as StdResult;
    use time::{macros::format_description, Date};

    pub(crate) fn serialize<S>(date: &Date, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let format = format_description!("[year]-[month]-[day]");
        let date = date
            .to_owned()
            .format(format)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&date)
    }

    pub(crate) fn deserialize<'de, D>(deserializer: D) -> StdResult<Date, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let format = format_description!("[year]-[month]-[day]");
        let date = Date::parse(&s, &format).map_err(serde::de::Error::custom)?;
        Ok(date)
    }
}

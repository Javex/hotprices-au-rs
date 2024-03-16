use time::Date;


#[derive(Debug)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub description: String,
    // this should probably be a function that looks in price history
    // price: f64,
    pub price_history: Vec<PriceHistory>,
    pub is_weighted: bool,
    pub unit: Unit,
    pub quantity: f64,
}

#[derive(Debug)]
pub struct PriceHistory {
    pub date: Date,
    pub price: f64,
}

#[derive(Debug, PartialEq)]
pub enum Unit {
    Each,
    Grams,
    Millilitre,
    Centimetre,
}


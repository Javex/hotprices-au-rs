//! hotprices.org Crate
//!
//! A Rust implementation of the hotprices.org scraper for Australian grocery stores. The library
//! provides all the tools to both scrape prices from Woolworths & Coles as well as normalise them
//! into a canonical format. Inspired by [heisse-preise.io](https://heisse-preise.io/).
//!
//! The library is unlikely to be all the useful to you. Its documentation exists mostly as an
//! exercise for me, but you're welcome to experiment with it.
pub mod analysis;
mod cache;
mod category;
mod conversion;
mod date;
mod errors;
mod product;
mod retry;
mod storage;
pub mod stores;
pub mod sync;
mod unit;

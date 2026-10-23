//! Clipboard history storage: SQLite rows plus retention cleanup.

pub mod db;
pub mod retention;

pub use db::{hash_bytes, Item, Kind, NewItem, Store};

mod actions;
pub mod instance;
pub mod model;
mod platform;
mod search;
#[cfg(feature = "semantic")]
mod semantic;
#[cfg(not(feature = "semantic"))]
#[path = "semantic_disabled.rs"]
mod semantic;
pub mod updates;
mod usage;

pub use search::SearchEngine;

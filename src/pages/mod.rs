//! The three routes this SPA serves — see `main.rs` for how the right one is
//! picked. Each page owns its own data fetching end to end (manifest ->
//! document -> parsed model), same "no shared page state" shape `ontologies`
//! and `crosswalk` already had as ports of the two Vue pages. `catalog` is
//! new (there is no Vue source to port): it fetches `catalog.jsonld` instead.

pub mod catalog;
pub mod crosswalk;
pub mod ontologies;

//! The two routes this SPA serves — see `main.rs` for how the right one is
//! picked. Each page owns its own data fetching end to end (manifest ->
//! document -> parsed model), same "no shared page state" shape the Vue
//! source's two pages already had.

pub mod crosswalk;
pub mod ontologies;

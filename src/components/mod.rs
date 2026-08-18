//! Component tree for the ontology browser UI.
//!
//! `atoms` holds the leaf presentational components; `molecules` composes
//! them into small self-contained fragments. Later stages add `organisms`
//! here as the Vue components under `containers/prez-ui/theme/app/components/`
//! are ported to Yew.

pub mod atoms;
pub mod molecules;

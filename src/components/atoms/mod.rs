//! Leaf presentational components ("atoms") for the ontology browser UI.
//!
//! Mirrors `containers/prez-ui/theme/app/components/ontology/atoms/` from the
//! Vue source — small, dependency-free building blocks with no children of
//! their own, composed by molecules/organisms added in later stages.
//!
//! Not yet wired into the app (later stage composes these in) — allow
//! dead_code and unused_imports until then so the build stays warning-clean,
//! same convention as `src/ontology.rs`.
#![allow(dead_code, unused_imports)]

mod badge;
mod iri;

pub use badge::{BadgeVariant, OntoBadge, OntoBadgeProps};
pub use iri::{OntoIri, OntoIriProps};

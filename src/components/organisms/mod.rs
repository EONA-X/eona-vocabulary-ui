//! Organism-level components for the ontology browser UI — larger composites
//! built from `molecules`, each a self-contained page section (one term's
//! full entry, a collapsible group of terms).
//!
//! Mirrors `containers/prez-ui/theme/app/components/ontology/organisms/` from
//! the Vue source. A later stage assembles these into the page itself.
//!
//! Not yet wired into the app (later stage composes these in) — allow
//! dead_code and unused_imports until then so the build stays warning-clean,
//! same convention as `components::atoms` / `components::molecules`.
#![allow(dead_code, unused_imports)]

mod browser;
mod section;
mod selector;
mod term_card;

pub use browser::{OntologyBrowser, OntologyBrowserProps};
pub use section::{OntoSection, OntoSectionProps};
pub use selector::{OntologySelector, OntologySelectorProps};
pub use term_card::{OntoTermCard, OntoTermCardProps};

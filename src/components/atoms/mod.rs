//! Leaf components for the ontology browser UI.
//!
//! Both of these — an IRI/CURIE display with a copy button, and a theme
//! toggle — came back from `eona-ui-toolkit`'s `interactive` tier, which was
//! retired because this app was its only consumer. They hold state and touch
//! `localStorage`/the clipboard, so they never suited a crate whose other
//! components are SSR-renderable by contract.
//!
//! The term-kind badge that used to sit here did *not* come back: it is
//! presentational, so it stayed in the toolkit as `OntoBadge` and is one of the
//! components that crate publishes to React.
//!
//! The `*Props` types and `apply_theme`/`STORAGE_KEY` are re-exported although
//! nothing in this crate names them: they are part of each component's public
//! surface, and `apply_theme` in particular is the one function index.html's
//! pre-mount script duplicates in JS. Same `allow` as the sibling modules.
#![allow(dead_code, unused_imports)]

mod iri;
mod theme_toggle;

pub use iri::{OntoIri, OntoIriProps};
pub use theme_toggle::{apply_theme, ThemeToggle, ThemeToggleProps, STORAGE_KEY};

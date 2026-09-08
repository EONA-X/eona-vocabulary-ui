//! Leaf components for the ontology browser UI.
//!
//! All three that lived here — a theme toggle, an IRI/CURIE display, a
//! term-kind badge — were app-agnostic with no counterpart in
//! `eona-ui-toolkit`, so they moved to its `interactive` tier.
//!
//! Only `ThemeToggle` is still reached through this path (by
//! `organisms::navbar`); the other two are used by the ontology components,
//! which moved with them. Import from
//! `eona_ui_toolkit::interactive::atoms` directly if this app needs them again.

pub use eona_ui_toolkit::interactive::atoms::ThemeToggle;

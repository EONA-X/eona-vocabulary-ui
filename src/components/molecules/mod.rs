//! Molecule-level components for the ontology browser UI.
//!
//! The term molecules came back from `eona-ui-toolkit`'s `interactive` tier
//! with the rest of the ontology browser; they embed `OntoIri`'s copy button,
//! which is what kept them out of the toolkit's presentational tier in the
//! first place. The UML molecules never left: they belong to this app's diagram
//! renderer, which is a layout engine rather than a design-system component.
#![allow(dead_code, unused_imports)]

mod term_header;
mod term_ref;
mod uml_card;
mod uml_class;
mod uml_edge;
mod uml_tree_node;

// OntoAnnotation holds no state, so it stays in the toolkit with the
// presentational components. `term_card.rs:14` and `browser.rs:36` render it
// from `NamedValues::values`, whose element type is the toolkit's own
// `LiteralValue` — see the note at src/ontology.rs:26.
pub use eona_ui_toolkit::molecules::{OntoAnnotation, OntoAnnotationProps};
pub use term_header::{OntoTermHeader, OntoTermHeaderProps};
pub use term_ref::{OntoTermRef, OntoTermRefProps};
pub use uml_card::{OntoUmlCard, OntoUmlCardProps};
pub use uml_class::{OntoUmlClass, OntoUmlClassProps, UmlClassState};
pub use uml_edge::{OntoUmlEdge, OntoUmlEdgeProps, UmlEdgeState};
pub use uml_tree_node::{OntoUmlTreeNode, OntoUmlTreeNodeProps};

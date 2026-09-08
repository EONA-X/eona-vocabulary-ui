//! Molecule-level components for the ontology browser UI.
//!
//! The ontology-generic ones (annotation rows, term headers, term references)
//! moved to `eona-ui-toolkit`'s `interactive` tier and are re-exported here.
//! The UML molecules stay: they belong to this app's diagram renderer, which
//! is a layout engine rather than a design-system component.
#![allow(dead_code, unused_imports)]

mod uml_card;
mod uml_class;
mod uml_edge;
mod uml_tree_node;

// OntoAnnotation holds no state, so the toolkit keeps it with the presentational
// components rather than in `interactive`; the term molecules embed OntoIri's
// copy button and stay behind `csr`.
pub use eona_ui_toolkit::interactive::molecules::{
    OntoTermHeader, OntoTermHeaderProps, OntoTermRef, OntoTermRefProps,
};
pub use eona_ui_toolkit::molecules::{OntoAnnotation, OntoAnnotationProps};
pub use uml_card::{OntoUmlCard, OntoUmlCardProps};
pub use uml_class::{OntoUmlClass, OntoUmlClassProps, UmlClassState};
pub use uml_edge::{OntoUmlEdge, OntoUmlEdgeProps, UmlEdgeState};
pub use uml_tree_node::{OntoUmlTreeNode, OntoUmlTreeNodeProps};

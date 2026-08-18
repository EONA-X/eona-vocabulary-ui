//! Molecule-level components for the ontology browser UI — small composites
//! built from the leaf `atoms`, each a self-contained fragment (a resource
//! reference, a labelled annotation row, a term card's heading block).
//!
//! Mirrors `containers/prez-ui/theme/app/components/ontology/molecules/` from
//! the Vue source. Organisms added in a later stage assemble these.
//!
//! Not yet wired into the app (later stage composes these in) — allow
//! dead_code and unused_imports until then so the build stays warning-clean,
//! same convention as `components::atoms`.
#![allow(dead_code, unused_imports)]

mod annotation;
mod term_header;
mod term_ref;
mod uml_card;
mod uml_class;
mod uml_edge;
mod uml_tree_node;

pub use annotation::{OntoAnnotation, OntoAnnotationProps};
pub use term_header::{OntoTermHeader, OntoTermHeaderProps};
pub use term_ref::{OntoTermRef, OntoTermRefProps};
pub use uml_card::{OntoUmlCard, OntoUmlCardProps};
pub use uml_class::{OntoUmlClass, OntoUmlClassProps, UmlClassState};
pub use uml_edge::{OntoUmlEdge, OntoUmlEdgeProps, UmlEdgeState};
pub use uml_tree_node::{OntoUmlTreeNode, OntoUmlTreeNodeProps};

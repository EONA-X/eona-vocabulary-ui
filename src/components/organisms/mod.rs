//! Organism-level components for the ontology browser UI.
//!
//! The ontology-generic ones (term cards, sections, the ontology selector and
//! the browser itself) moved to `eona-ui-toolkit`'s `interactive` tier and are
//! re-exported here. What stays is what is specific to this app: the crosswalk
//! screens, the UML diagram, and the navbar.
//!
//! `OntologyBrowser` no longer renders the UML diagram itself — it takes a
//! `diagram` slot, and `pages::ontologies` passes `OntoUmlDiagram` into it. See
//! that prop's docs for why the diagram did not move with the browser.
#![allow(dead_code, unused_imports)]

mod crosswalk_scene;
mod crosswalk_selector;
mod crosswalk_transform_form;
mod navbar;
mod uml_diagram;

pub use eona_ui_toolkit::interactive::organisms::{
    OntologyBrowser, OntologyBrowserProps, OntologySelector, OntologySelectorProps, OntoSection,
    OntoSectionProps, OntoTermCard, OntoTermCardProps,
};

pub use crosswalk_scene::{CrosswalkScene, CrosswalkSceneProps};
pub use crosswalk_selector::{CrosswalkSelector, CrosswalkSelectorProps};
pub use crosswalk_transform_form::{CrosswalkTransformForm, CrosswalkTransformFormProps};
pub use navbar::{NavRoute, Navbar};
pub use uml_diagram::{OntoUmlDiagram, OntoUmlDiagramProps};

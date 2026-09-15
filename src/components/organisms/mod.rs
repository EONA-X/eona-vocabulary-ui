//! Organism-level components for the ontology browser UI.
//!
//! The ontology-generic ones (term cards, sections, the ontology selector and
//! the browser itself) came back from `eona-ui-toolkit`'s `interactive` tier:
//! this app was that tier's only consumer, and a crate on private GitLab is a
//! poor place for the core of a GitHub-hosted app. What was already here stays:
//! the crosswalk screens, the UML diagram, and the navbar.
//!
//! `OntologyBrowser` keeps the `diagram` slot it grew while it lived in the
//! toolkit rather than reverting to hard-wiring `OntoUmlDiagram` — see that
//! prop's docs.
#![allow(dead_code, unused_imports)]

mod browser;
mod crosswalk_scene;
mod crosswalk_selector;
mod crosswalk_transform_form;
mod navbar;
mod section;
mod selector;
mod term_card;
mod uml_diagram;

pub use browser::{OntologyBrowser, OntologyBrowserProps};
pub use crosswalk_scene::{CrosswalkScene, CrosswalkSceneProps};
pub use crosswalk_selector::{CrosswalkSelector, CrosswalkSelectorProps};
pub use crosswalk_transform_form::{CrosswalkTransformForm, CrosswalkTransformFormProps};
pub use navbar::{NavRoute, Navbar};
pub use section::{OntoSection, OntoSectionProps};
pub use selector::{OntologySelector, OntologySelectorProps};
pub use term_card::{OntoTermCard, OntoTermCardProps};
pub use uml_diagram::{OntoUmlDiagram, OntoUmlDiagramProps};

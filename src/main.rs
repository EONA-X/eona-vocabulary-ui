//! EOVoc UI entry point.
//!
//! Three routes, picked by plain `window.location.pathname` at render time
//! (no `yew-router`: this SPA has a handful of independent pages, each with
//! its own manifest-driven data fetching and no shared state between them,
//! and every cross-page link is a plain `<a href>` full navigation — since
//! each page already fetches everything it needs on mount regardless of how
//! it was reached, a client-side transition would buy nothing here). nginx's
//! SPA fallback (`try_files $uri /index.html`) serves this same bundle for
//! every path, including the fallback branch below (an unrecognised path
//! still needs *some* page rendered client-side).
//!
//! "/" is the DCAT vocabulary catalog (`pages::catalog`, new); the
//! single-ontology term browser that used to live at "/" is now at
//! "/ontologies" (`pages::ontologies`) — its own JSON-LD manifest fetch and
//! `?ontology=<slug>` deep-linking are unchanged, only the path moved.
mod components;
mod crosswalk;
mod crosswalk3d;
mod dcat;
mod net;
mod ontology;
mod pages;
mod uml;

use web_sys::window;
use yew::prelude::*;

use pages::catalog::CatalogPage;
use pages::crosswalk::CrosswalkPage;
use pages::ontologies::OntologiesPage;

#[function_component(App)]
fn app() -> Html {
    let path = window().and_then(|w| w.location().pathname().ok()).unwrap_or_default();
    if path == "/crosswalk" || path.starts_with("/crosswalk/") {
        html! { <CrosswalkPage /> }
    } else if path == "/ontologies" || path.starts_with("/ontologies/") {
        html! { <OntologiesPage /> }
    } else {
        html! { <CatalogPage /> }
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<App>::new().render();
}

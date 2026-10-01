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
//!
//! Matched on the pathname's last segment, not the full pathname: this site
//! isn't always mounted at the domain root (a GitHub Pages project site
//! serves it under `/<repo-name>/`, e.g. `/eona-vocabulary-ui/crosswalk`) —
//! see index.html's `<base data-trunk-public-url>` for the same concern on
//! the asset/fetch side.
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
    let route = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if route == "crosswalk" {
        html! { <CrosswalkPage /> }
    } else if route == "ontologies" {
        html! { <OntologiesPage /> }
    } else {
        html! { <CatalogPage /> }
    }
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    yew::Renderer::<App>::new().render();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crosswalk_page_is_at_crosswalks_and_still_at_crosswalk() {
        // "crosswalk" is also an asset type of the published IRIs
        // (https://<host>/crosswalk/<slug>/<version>), whose bare path the
        // hosting answers itself: the page needs a name of its own.
        assert_eq!(Page::from_path("/crosswalks"), Page::Crosswalk);
        assert_eq!(Page::from_path("/eona-vocabulary-ui/crosswalks/"), Page::Crosswalk);
        assert_eq!(Page::from_path("/crosswalk"), Page::Crosswalk);
        assert_eq!(Page::from_path("/ontologies"), Page::Ontologies);
        assert_eq!(Page::from_path("/"), Page::Catalog);
        assert_eq!(Page::from_path("/anything"), Page::Catalog);
    }
}

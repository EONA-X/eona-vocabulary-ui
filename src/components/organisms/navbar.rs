//! ORGANISM · Navbar
//!
//! Shared top bar for all three routes (see `main.rs`) — brand, cross-page
//! links (previously duplicated per-page as a bare `<nav
//! class="crosswalk-page__nav">`), and the [`ThemeToggle`](crate::components::atoms::ThemeToggle).
//! Every link is a plain `<a href>` full navigation, matching this SPA's
//! no-router design (each page fetches everything it needs on mount
//! regardless of how it was reached).
//!
//! hrefs are relative to index.html's `<base data-trunk-public-url>`, not
//! root-absolute — see main.rs.

use yew::prelude::*;

use crate::components::atoms::ThemeToggle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavRoute {
    Catalog,
    Ontologies,
    Crosswalk,
}

#[derive(Properties, PartialEq, Clone)]
pub struct NavbarProps {
    pub current: NavRoute,
}

#[function_component(Navbar)]
pub fn navbar(props: &NavbarProps) -> Html {
    let link = |route: NavRoute, href: &'static str, label: &'static str| {
        let active = props.current == route;
        let class = classes!("eovoc-navbar__link", active.then_some("eovoc-navbar__link--active"));
        let aria_current = active.then_some("page");
        html! { <a class={class} href={href} aria-current={aria_current}>{ label }</a> }
    };

    html! {
        <header class="eovoc-navbar">
            <a class="eovoc-navbar__brand" href=".">{ "EONA-X Vocabularies" }</a>
            <nav class="eovoc-navbar__links">
                { link(NavRoute::Catalog, ".", "Catalog") }
                { link(NavRoute::Ontologies, "ontologies", "Ontology Browser") }
                { link(NavRoute::Crosswalk, "crosswalk", "Crosswalk 3D") }
            </nav>
            <ThemeToggle />
        </header>
    }
}

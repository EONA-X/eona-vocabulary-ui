//! PAGE · Ontology Browser  (route "/")
//!
//! Ports `containers/prez-ui/theme/app/pages/ontologies.vue`. The data is
//! fetched in the browser because the ontology serialisations are produced
//! by a separate pipeline and mounted under `/docs` only at deploy time:
//!
//!   1. GET /docs/ontologies.json        — the static manifest driving the selector
//!   2. GET /docs/<slug>/ontology.jsonld — the expanded JSON-LD for the chosen ontology
//!
//! The JSON-LD is turned into a view model by `crate::ontology::parse_ontology`
//! and rendered by the atoms -> molecules -> organisms components under
//! `crate::components`.
use std::collections::HashMap;

use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use yew::prelude::*;

use crate::components::organisms::{OntologyBrowser, OntologySelector};
use crate::net::{fetch_json, query_param, sync_url_slug};
use crate::ontology::{namespace_prefixes, parse_ontology, OntologyEntry, OntologyModel};

/// Site-absolute path where the ontology-docs pipeline output is served —
/// mirrors the Vue source's `DOCS_BASE` constant. Independent of wherever
/// this SPA itself is hosted.
const DOCS_BASE: &str = "/docs";

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
struct Manifest {
    #[allow(dead_code)]
    #[serde(rename = "generatedAt")]
    generated_at: Option<String>,
    #[serde(default)]
    ontologies: Vec<OntologyEntry>,
}

/// prefix -> slug, built from every manifest entry that declares a prefix —
/// mirrors the Vue source's `prefixLinks` computed, threaded into
/// `OntologyBrowser` for cross-ontology term-ref resolution.
fn prefix_links(ontologies: &[OntologyEntry]) -> HashMap<String, String> {
    ontologies.iter().filter_map(|o| Some((o.prefix.clone()?, o.slug.clone()))).collect()
}

#[function_component(OntologiesPage)]
pub fn ontologies_page() -> Html {
    let ontologies = use_state(Vec::<OntologyEntry>::new);
    let selected_slug = use_state(String::new);
    let model = use_state(|| None::<OntologyModel>);
    let loading_manifest = use_state(|| true);
    let loading_model = use_state(|| false);
    let error = use_state(|| None::<String>);

    // Set the document title once on mount.
    use_effect_with((), |_| {
        if let Some(doc) = window().and_then(|w| w.document()) {
            doc.set_title("Ontology Browser - EONA-X Vocabularies");
        }
        || ()
    });

    // Load the manifest once on mount, then pick the initial selection from
    // `?ontology=<slug>` when it names a known entry, else the first entry.
    // Assigning `selected_slug` triggers the model-loading effect below.
    {
        let ontologies = ontologies.clone();
        let selected_slug = selected_slug.clone();
        let loading_manifest = loading_manifest.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match fetch_json::<Manifest>(&format!("{DOCS_BASE}/ontologies.json")).await {
                    Ok(manifest) if !manifest.ontologies.is_empty() => {
                        let requested = query_param("ontology");
                        let initial = requested
                            .as_deref()
                            .and_then(|r| manifest.ontologies.iter().find(|o| o.slug == r))
                            .or_else(|| manifest.ontologies.first())
                            .map(|o| o.slug.clone())
                            .unwrap_or_default();
                        ontologies.set(manifest.ontologies);
                        selected_slug.set(initial);
                    }
                    Ok(_) => {
                        error.set(Some("No ontologies have been published yet.".to_string()));
                    }
                    Err(e) => {
                        log::error!("failed to load /docs/ontologies.json: {e}");
                        error.set(Some(
                            "Could not load the ontology manifest (/docs/ontologies.json). Has the ontology documentation been built?"
                                .to_string(),
                        ));
                    }
                }
                loading_manifest.set(false);
            });
            || ()
        });
    }

    // (Re)load the selected ontology document whenever the slug changes
    // (initial pick, or the user changing the OntologySelector).
    {
        let ontologies = ontologies.clone();
        let model = model.clone();
        let loading_model = loading_model.clone();
        let error = error.clone();
        use_effect_with((*selected_slug).clone(), move |slug| {
            let slug = slug.clone();
            if let Some(entry) = (!slug.is_empty())
                .then(|| ontologies.iter().find(|o| o.slug == slug).cloned())
                .flatten()
            {
                let ns_prefixes = namespace_prefixes(&ontologies);
                sync_url_slug("ontology", &slug);
                loading_model.set(true);
                error.set(None);
                model.set(None);
                spawn_local(async move {
                    let url = format!("{DOCS_BASE}/{}", entry.jsonld);
                    match fetch_json::<serde_json::Value>(&url).await {
                        Ok(doc) => model.set(Some(parse_ontology(&doc, Some(&ns_prefixes)))),
                        Err(e) => {
                            log::error!("failed to load {url}: {e}");
                            error.set(Some(format!(
                                "Could not load \u{201c}{}\u{201d}. The ontology document may not have been published yet.",
                                entry.title
                            )));
                        }
                    }
                    loading_model.set(false);
                });
            }
            || ()
        });
    }

    let loading_manifest_val = *loading_manifest;
    let fatal_error = !loading_manifest_val && error.is_some() && ontologies.is_empty();
    let on_slug_change = {
        let selected_slug = selected_slug.clone();
        Callback::from(move |slug: String| selected_slug.set(slug))
    };

    html! {
        <main class="eovoc-page">
            <nav class="crosswalk-page__nav">
                <a href="/">{ "\u{2190} Catalog" }</a>
                <a href="/crosswalk">{ "Crosswalk 3D \u{2192}" }</a>
            </nav>
            <div class="eovoc-page__header">
                <h1 class="eovoc-page__title">{ "Ontology Browser" }</h1>
                <p class="eovoc-page__lede">
                    { "Browse the classes, properties and individuals of the EONA-X vocabularies, read directly from the generated JSON-LD. Pick an ontology to explore, or download its RDF serialisation." }
                </p>
            </div>

            if loading_manifest_val {
                <div class="eovoc-state">{ "Loading ontologies\u{2026}" }</div>
            } else if fatal_error {
                <div class="eovoc-state eovoc-state--error">
                    <p class="eovoc-state__title">{ "Unable to load the ontologies" }</p>
                    <p class="eovoc-state__message">{ (*error).clone().unwrap_or_default() }</p>
                </div>
            } else {
                <>
                    <div class="eovoc-page__selector">
                        <OntologySelector
                            ontologies={(*ontologies).clone()}
                            model_value={AttrValue::from((*selected_slug).clone())}
                            on_change={on_slug_change}
                            docs_base={AttrValue::from(DOCS_BASE)}
                        />
                    </div>

                    if *loading_model {
                        <div class="eovoc-state">{ "Loading ontology\u{2026}" }</div>
                    } else if let Some(msg) = (*error).clone() {
                        <div class="eovoc-state eovoc-state--error">
                            <p class="eovoc-state__title">{ "Could not load this ontology" }</p>
                            <p class="eovoc-state__message">{ msg }</p>
                        </div>
                    } else if let Some(m) = (*model).clone() {
                        <OntologyBrowser model={m} prefix_links={Some(prefix_links(&ontologies))} />
                    }
                </>
            }
        </main>
    }
}

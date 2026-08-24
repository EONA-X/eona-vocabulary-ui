//! PAGE · Crosswalk 3D  (route "/crosswalk")
//!
//! Ports `containers/prez-ui/theme/app/pages/crosswalk.vue`: a 3D view of an
//! ontology crosswalk, picked from the static alignments manifest — exactly
//! the "/" page's own pattern, generalised to a second static manifest:
//!
//!   1. GET /docs/alignments.json        — which crosswalks exist, and which
//!                                          ontology slugs ("sides") each one
//!                                          bridges (resolved server-side by
//!                                          generate.py's find_alignment_sides
//!                                          — never hardcoded here)
//!   2. GET /docs/ontologies.json        — resolve each side's own entry
//!   3. GET /docs/<side>/ontology.jsonld — once per side
//!   4. GET /docs/<slug>/alignment.jsonld — the crosswalk's own mapping graph
//!
//! No ontology or crosswalk slug is hardcoded anywhere below.
use futures::future::join_all;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use yew::prelude::*;

use crate::components::organisms::{CrosswalkScene, CrosswalkSelector, CrosswalkTransformForm, NavRoute, Navbar};
use crate::crosswalk::{build_crosswalk_graph, AlignmentEntry, XwalkGraph};
use crate::crosswalk3d::layout_crosswalk_3d;
use crate::net::{fetch_json, fetch_text, query_param, sync_url_slug};
use crate::ontology::{namespace_prefixes, parse_ontology, OntologyEntry};

const DOCS_BASE: &str = "/docs";

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
struct AlignmentsManifest {
    #[allow(dead_code)]
    #[serde(rename = "generatedAt")]
    generated_at: Option<String>,
    #[serde(default)]
    alignments: Vec<AlignmentEntry>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Default)]
struct OntologiesManifest {
    #[allow(dead_code)]
    #[serde(rename = "generatedAt")]
    generated_at: Option<String>,
    #[serde(default)]
    ontologies: Vec<OntologyEntry>,
}

#[function_component(CrosswalkPage)]
pub fn crosswalk_page() -> Html {
    let alignments = use_state(Vec::<AlignmentEntry>::new);
    let ontologies = use_state(Vec::<OntologyEntry>::new);
    let selected_slug = use_state(String::new);
    let graph = use_state(|| None::<XwalkGraph>);
    // The alignment document's raw JSON-LD text, kept alongside the parsed
    // `graph` — `eona_crosswalk_transform::build_mapping` wants the
    // reified-SKOS document itself, not the `OntologyModel` `parse_ontology`
    // builds from it for the 3D view, and re-fetching it a second time for
    // that purpose alone would be wasteful.
    let alignment_text = use_state(|| None::<String>);
    let loading_manifest = use_state(|| true);
    let loading_graph = use_state(|| false);
    let error = use_state(|| None::<String>);

    use_effect_with((), |_| {
        if let Some(doc) = window().and_then(|w| w.document()) {
            doc.set_title("Crosswalk 3D - EONA-X Vocabularies");
        }
        || ()
    });

    // Load both manifests once on mount (in parallel, mirroring the Vue
    // source's Promise.all), then pick the initial selection from
    // `?crosswalk=<slug>` when it names a known entry, else the first entry.
    {
        let alignments = alignments.clone();
        let ontologies = ontologies.clone();
        let selected_slug = selected_slug.clone();
        let loading_manifest = loading_manifest.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                let alignments_url = format!("{DOCS_BASE}/alignments.json");
                let ontologies_url = format!("{DOCS_BASE}/ontologies.json");
                let (alignments_res, ontologies_res) = futures::join!(
                    fetch_json::<AlignmentsManifest>(&alignments_url),
                    fetch_json::<OntologiesManifest>(&ontologies_url),
                );
                match (alignments_res, ontologies_res) {
                    (Ok(a), Ok(o)) if !a.alignments.is_empty() => {
                        let requested = query_param("crosswalk");
                        let initial = requested
                            .as_deref()
                            .and_then(|r| a.alignments.iter().find(|x| x.slug == r))
                            .or_else(|| a.alignments.first())
                            .map(|x| x.slug.clone())
                            .unwrap_or_default();
                        alignments.set(a.alignments);
                        ontologies.set(o.ontologies);
                        selected_slug.set(initial);
                    }
                    (Ok(_), _) => {
                        error.set(Some("No ontology crosswalks have been published yet.".to_string()));
                    }
                    (Err(e), _) => {
                        log::error!("failed to load /docs/alignments.json: {e}");
                        error.set(Some("Could not load the crosswalk manifest (/docs/alignments.json). Has it been built?".to_string()));
                    }
                }
                loading_manifest.set(false);
            });
            || ()
        });
    }

    // (Re)load the selected crosswalk whenever the slug changes.
    {
        let alignments = alignments.clone();
        let ontologies = ontologies.clone();
        let graph = graph.clone();
        let alignment_text = alignment_text.clone();
        let loading_graph = loading_graph.clone();
        let error = error.clone();
        use_effect_with((*selected_slug).clone(), move |slug| {
            let slug = slug.clone();
            let entry = (!slug.is_empty()).then(|| alignments.iter().find(|a| a.slug == slug).cloned()).flatten();
            if let Some(entry) = entry {
                sync_url_slug("crosswalk", &slug);
                loading_graph.set(true);
                error.set(None);
                graph.set(None);
                alignment_text.set(None);
                let ns_prefixes = namespace_prefixes(&ontologies);
                let side_entries: Vec<OntologyEntry> =
                    entry.sides.iter().filter_map(|s| ontologies.iter().find(|o| &o.slug == s).cloned()).collect();
                spawn_local(async move {
                    let fail = || {
                        format!("Could not load \u{201c}{}\u{201d}. The crosswalk may not have been published yet.", entry.title)
                    };
                    if side_entries.len() < 2 {
                        log::error!("'{}' resolves to {} published side(s) (need 2+)", slug, side_entries.len());
                        error.set(Some(fail()));
                        loading_graph.set(false);
                        return;
                    }
                    let side_urls: Vec<String> = side_entries.iter().map(|o| format!("{DOCS_BASE}/{}", o.jsonld)).collect();
                    let alignment_url = format!("{DOCS_BASE}/{}", entry.jsonld);
                    let side_docs = join_all(side_urls.iter().map(|u| fetch_json::<serde_json::Value>(u)));
                    // Text, not fetch_json: build_mapping wants the raw
                    // reified-SKOS document, parse_ontology (below) wants it
                    // as a parsed Value — fetch once, use both ways.
                    let alignment_raw = fetch_text(&alignment_url);
                    let (side_docs, alignment_raw) = futures::join!(side_docs, alignment_raw);

                    let mut sides = Vec::with_capacity(side_entries.len());
                    let mut ok = true;
                    for (o, doc) in side_entries.iter().zip(side_docs) {
                        match doc {
                            Ok(doc) => sides.push((o.slug.clone(), o.title.clone(), parse_ontology(&doc, Some(&ns_prefixes)))),
                            Err(e) => {
                                log::error!("failed to load {}/{}: {e}", DOCS_BASE, o.jsonld);
                                ok = false;
                            }
                        }
                    }
                    let alignment_model = match &alignment_raw {
                        Ok(text) => match serde_json::from_str::<serde_json::Value>(text) {
                            Ok(doc) => Some(parse_ontology(&doc, Some(&ns_prefixes))),
                            Err(e) => {
                                log::error!("failed to parse {}/{}: {e}", DOCS_BASE, entry.jsonld);
                                ok = false;
                                None
                            }
                        },
                        Err(e) => {
                            log::error!("failed to load {}/{}: {e}", DOCS_BASE, entry.jsonld);
                            ok = false;
                            None
                        }
                    };

                    if ok {
                        if let Some(alignment_model) = alignment_model {
                            let mut built = build_crosswalk_graph(&sides, &alignment_model);
                            layout_crosswalk_3d(&mut built);
                            graph.set(Some(built));
                            alignment_text.set(alignment_raw.ok());
                        }
                    } else {
                        error.set(Some(fail()));
                    }
                    loading_graph.set(false);
                });
            }
            || ()
        });
    }

    let loading_manifest_val = *loading_manifest;
    let fatal_error = !loading_manifest_val && error.is_some() && alignments.is_empty();
    let on_slug_change = {
        let selected_slug = selected_slug.clone();
        Callback::from(move |slug: String| selected_slug.set(slug))
    };

    html! {
        <main class="crosswalk-page">
            <Navbar current={NavRoute::Crosswalk} />
            <div class="eovoc-page__header">
                <h1 class="eovoc-page__title">{ "Crosswalk 3D" }</h1>
                <p class="eovoc-page__lede">
                    { "A 3D projection of the mapping between two ontologies \u{2014} each one's own classes on its own plane, coloured by ontology, with the correspondences between them rendered as bridges." }
                </p>
            </div>

            if loading_manifest_val {
                <div class="eovoc-state">{ "Loading crosswalks\u{2026}" }</div>
            } else if fatal_error {
                <div class="eovoc-state eovoc-state--error">
                    <p class="eovoc-state__title">{ "Unable to load the crosswalks" }</p>
                    <p class="eovoc-state__message">{ (*error).clone().unwrap_or_default() }</p>
                </div>
            } else {
                <>
                    <div class="eovoc-page__selector">
                        <CrosswalkSelector
                            alignments={(*alignments).clone()}
                            ontologies={(*ontologies).clone()}
                            model_value={AttrValue::from((*selected_slug).clone())}
                            on_change={on_slug_change}
                            docs_base={AttrValue::from(DOCS_BASE)}
                        />
                    </div>

                    if *loading_graph {
                        <div class="eovoc-state">{ "Loading crosswalk\u{2026}" }</div>
                    } else if let Some(msg) = (*error).clone() {
                        <div class="eovoc-state eovoc-state--error">
                            <p class="eovoc-state__title">{ "Could not load this crosswalk" }</p>
                            <p class="eovoc-state__message">{ msg }</p>
                        </div>
                    } else if let Some(g) = (*graph).clone() {
                        <>
                            <CrosswalkScene graph={g.clone()} />
                            if let Some(text) = (*alignment_text).clone() {
                                <div class="crosswalk-page__form-wrap">
                                    <CrosswalkTransformForm graph={g} alignment_text={AttrValue::from(text)} />
                                </div>
                            }
                        </>
                    }
                </>
            }
        </main>
    }
}

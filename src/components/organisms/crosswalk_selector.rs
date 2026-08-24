//! ORGANISM · CrosswalkSelector
//!
//! The crosswalk picker, driven entirely by the static manifest JSON
//! (`/docs/alignments.json`) — same pattern as `OntologySelector`, one level
//! up: a styled native `<select>` (no runtime UI deps), the selected
//! crosswalk's description, which two (or more) ontologies it bridges
//! (resolved via `sides`, cross-referenced against `/docs/ontologies.json`
//! for their titles), and download links for its serialisations.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/organisms/CrosswalkSelector.vue`.

use web_sys::HtmlSelectElement;
use yew::prelude::*;

use crate::crosswalk::AlignmentEntry;
use crate::ontology::OntologyEntry;

/// Fixed label order for the download links, paired with their `downloads`
/// map key — mirrors the Vue source's `DOWNLOAD_FORMATS` table (same table
/// as `OntologySelector`'s).
const DOWNLOAD_FORMATS: [(&str, &str); 4] =
    [("ttl", "TTL"), ("jsonld", "JSON-LD"), ("owl", "OWL"), ("nt", "N-Triples")];

#[derive(Properties, PartialEq, Clone)]
pub struct CrosswalkSelectorProps {
    pub alignments: Vec<AlignmentEntry>,
    pub ontologies: Vec<OntologyEntry>,
    pub model_value: AttrValue,
    pub on_change: Callback<String>,
    pub docs_base: AttrValue,
}

#[function_component(CrosswalkSelector)]
pub fn crosswalk_selector(props: &CrosswalkSelectorProps) -> Html {
    let selected = props.alignments.iter().find(|a| a.slug == props.model_value.as_str());

    let side_titles: Vec<String> = selected
        .map(|a| {
            a.sides
                .iter()
                .map(|s| props.ontologies.iter().find(|o| &o.slug == s).map(|o| o.title.clone()).unwrap_or_else(|| s.clone()))
                .collect()
        })
        .unwrap_or_default();

    let download_links: Vec<(&str, &str, &str)> = selected
        .and_then(|a| a.downloads.as_ref())
        .map(|downloads| {
            DOWNLOAD_FORMATS
                .iter()
                .filter_map(|(key, label)| {
                    let path = match *key {
                        "ttl" => downloads.ttl.as_deref(),
                        "jsonld" => downloads.jsonld.as_deref(),
                        "owl" => downloads.owl.as_deref(),
                        "nt" => downloads.nt.as_deref(),
                        _ => None,
                    };
                    path.map(|p| (*key, *label, p))
                })
                .collect()
        })
        .unwrap_or_default();

    let onchange = {
        let on_change = props.on_change.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            on_change.emit(select.value());
        })
    };

    html! {
        <div class="ontology-selector">
            <label for="crosswalk-select" class="ontology-selector__label">{ "Crosswalk" }</label>
            <div class="ontology-selector__row">
                if props.alignments.len() > 1 {
                    <div class="ontology-selector__select-wrap">
                        <select id="crosswalk-select" class="ontology-selector__select" onchange={onchange}>
                            { for props.alignments.iter().map(|a| {
                                let is_selected = a.slug == props.model_value.as_str();
                                html! {
                                    <option value={a.slug.clone()} selected={is_selected}>
                                        { a.title.clone() }
                                    </option>
                                }
                            }) }
                        </select>
                        <svg
                            class="ontology-selector__chevron"
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            aria-hidden="true"
                        >
                            <path d="m6 9 6 6 6-6" />
                        </svg>
                    </div>
                } else if let Some(a) = selected {
                    <p class="crosswalk-selector__title">{ a.title.clone() }</p>
                }

                if !download_links.is_empty() {
                    <div class="ontology-selector__downloads">
                        { for download_links.iter().map(|(key, label, path)| html! {
                            <a
                                key={*key}
                                href={format!("{}/{}", props.docs_base, path)}
                                download=""
                                class="ontology-selector__download-link"
                            >
                                { *label }
                            </a>
                        }) }
                    </div>
                }
            </div>

            if !side_titles.is_empty() {
                <p class="crosswalk-selector__sides">{ side_titles.join(" \u{2194} ") }</p>
            }

            if let Some(description) = selected.and_then(|a| a.description.as_ref()) {
                <p class="ontology-selector__description">{ description.clone() }</p>
            }
        </div>
    }
}

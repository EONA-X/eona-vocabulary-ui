//! ORGANISM · OntologySelector
//!
//! The ontology picker, driven entirely by the static manifest JSON
//! (`/docs/ontologies.json`). A styled native `<select>` (no runtime UI
//! deps, works in the statically generated SPA), plus the selected entry's
//! description and download links for its serialisations
//! (TTL / JSON-LD / OWL / N-Triples).
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/organisms/OntologySelector.vue`.
//! The Vue source's `v-model` (`modelValue` prop + `update:modelValue`
//! emit) becomes an explicit `model_value` prop + `on_change` callback here
//! — Yew has no `v-model` sugar, so the parent owns the selected slug.

use web_sys::HtmlSelectElement;
use yew::prelude::*;

use crate::ontology::OntologyEntry;

/// Fixed label order for the download links, paired with their `downloads`
/// map key — mirrors the Vue source's `DOWNLOAD_FORMATS` table.
const DOWNLOAD_FORMATS: [(&str, &str); 4] =
    [("ttl", "TTL"), ("jsonld", "JSON-LD"), ("owl", "OWL"), ("nt", "N-Triples")];

#[derive(Properties, PartialEq, Clone)]
pub struct OntologySelectorProps {
    pub ontologies: Vec<OntologyEntry>,
    pub model_value: AttrValue,
    pub on_change: Callback<String>,
    pub docs_base: AttrValue,
}

#[function_component(OntologySelector)]
pub fn ontology_selector(props: &OntologySelectorProps) -> Html {
    let selected = props.ontologies.iter().find(|o| o.slug == props.model_value.as_str());

    // The selected entry's available downloads, in DOWNLOAD_FORMATS order —
    // mirrors the Vue source's `downloadLinks` computed.
    let download_links: Vec<(&str, &str, &str)> = selected
        .and_then(|o| o.downloads.as_ref())
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
            <label for="ontology-select" class="ontology-selector__label">{ "Ontology" }</label>
            <div class="ontology-selector__row">
                <div class="ontology-selector__select-wrap">
                    <select id="ontology-select" class="ontology-selector__select" onchange={onchange}>
                        { for props.ontologies.iter().map(|o| {
                            let is_selected = o.slug == props.model_value.as_str();
                            html! {
                                <option value={o.slug.clone()} selected={is_selected}>
                                    { o.title.clone() }
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

            if let Some(description) = selected.and_then(|o| o.description.as_ref()) {
                <p class="ontology-selector__description">{ description.clone() }</p>
            }
        </div>
    }
}

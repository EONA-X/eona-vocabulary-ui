//! ORGANISM · OntologyBrowser
//!
//! Renders a parsed `OntologyModel`: the ontology header (title,
//! descriptions, metadata + links), a live text filter with a
//! table-of-contents nav, and one collapsible `OntoSection` per entity kind.
//!
//! Owns each section's open/closed state (a `kind -> bool` map, reseeded
//! whenever a different `model` is loaded) so that (a) the text filter
//! force-expands every section with matches, and (b) navigating to a term
//! anchor (an in-page cross-reference, or the page's own URL hash on load)
//! auto-expands the section that contains it before scrolling. Composes:
//! `OntoSection`, `OntoAnnotation`, `OntoTermRef`, `OntoBadge`.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/organisms/OntologyBrowser.vue`.
//!
//! Deviations from the Vue source, both pragmatic simplifications of its
//! Vue-reactivity-driven wiring:
//! - The Vue source registers a single `hashchange` listener once at
//!   `onMounted` and removes it once at `onBeforeUnmount`; the handler reads
//!   `props.model`/`openMap` live through Vue's reactivity on every call. Yew
//!   has no equivalent "always-current" closure capture, so this port instead
//!   re-registers the listener inside the model-keyed effect (removing the
//!   previous one first) — there is still exactly one active listener at any
//!   time, and it always closes over the model that was current when it was
//!   (re)installed, which is the only model that can be current until the
//!   next re-registration.
//! - `nextTick` (wait for Vue's next DOM flush before `scrollIntoView`) is
//!   approximated with a zero-length `TimeoutFuture`, which likewise yields
//!   to the browser's event loop after Yew has applied the pending re-render.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{window, HtmlInputElement, ScrollBehavior, ScrollIntoViewOptions, ScrollLogicalPosition};
use yew::prelude::*;

use crate::components::atoms::{BadgeVariant, OntoBadge};
use crate::components::molecules::{OntoAnnotation, OntoTermRef};
use crate::components::organisms::section::OntoSection;
use crate::components::organisms::uml_diagram::OntoUmlDiagram;
use crate::ontology::{OntologyModel, OntologySection, Term, TermKind};

/// Expand modestly-sized sections by default; keep very large ones collapsed.
const DEFAULT_OPEN_MAX: usize = 40;

#[derive(Properties, PartialEq, Clone)]
pub struct OntologyBrowserProps {
    pub model: OntologyModel,
    /// prefix -> slug, for ontologies published in this browser. Threaded
    /// straight through to `OntoUmlDiagram`, same as the Vue source's
    /// `OntologyBrowser.vue` passing its own `prefixLinks` prop down to
    /// `OntoUmlDiagram.vue`.
    #[prop_or_default]
    pub prefix_links: Option<HashMap<String, String>>,
}

fn section_anchor_id(kind: TermKind) -> String {
    format!("section-{kind:?}")
}

/// anchor id -> owning section kind, for hash-target auto-expand.
fn build_anchor_kind(model: &OntologyModel) -> HashMap<String, TermKind> {
    let mut map = HashMap::new();
    for s in &model.sections {
        for t in &s.terms {
            map.insert(t.anchor.clone(), s.kind);
        }
    }
    map
}

fn seed_open_map(model: &OntologyModel) -> HashMap<TermKind, bool> {
    model.sections.iter().map(|s| (s.kind, s.terms.len() <= DEFAULT_OPEN_MAX)).collect()
}

/// Decode the current `window.location.hash` (sans leading `#`), mirroring
/// the Vue source's `decodeURIComponent(window.location.hash.replace(/^#/, ""))`.
fn current_hash_target() -> Option<String> {
    let win = window()?;
    let raw = win.location().hash().ok()?;
    let stripped = raw.strip_prefix('#').unwrap_or(&raw);
    if stripped.is_empty() {
        return None;
    }
    let decoded = js_sys::decode_uri_component(stripped)
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| stripped.to_string());
    Some(decoded)
}

fn scroll_into_view(anchor_id: String) {
    spawn_local(async move {
        // Approximates the Vue source's `nextTick` — let Yew's pending
        // re-render (from opening the section) apply before we query the DOM.
        TimeoutFuture::new(0).await;
        let Some(doc) = window().and_then(|w| w.document()) else { return };
        let Some(el) = doc.get_element_by_id(&anchor_id) else { return };
        let opts = ScrollIntoViewOptions::new();
        opts.set_behavior(ScrollBehavior::Smooth);
        opts.set_block(ScrollLogicalPosition::Start);
        el.scroll_into_view_with_scroll_into_view_options(&opts);
    });
}

/// If the current URL hash names a term anchor, open its owning section
/// (preserving every other section's open state) and scroll to it.
fn open_hash_target(
    anchor_kind: &HashMap<String, TermKind>,
    open_map_mirror: &Rc<RefCell<HashMap<TermKind, bool>>>,
    open_map: &UseStateHandle<HashMap<TermKind, bool>>,
) {
    let Some(hash) = current_hash_target() else { return };
    let Some(&kind) = anchor_kind.get(&hash) else { return };
    let mut merged = open_map_mirror.borrow().clone();
    merged.insert(kind, true);
    open_map.set(merged.clone());
    *open_map_mirror.borrow_mut() = merged;
    scroll_into_view(hash);
}

#[function_component(OntologyBrowser)]
pub fn ontology_browser(props: &OntologyBrowserProps) -> Html {
    let query = use_state(String::new);
    let open_map = use_state(HashMap::<TermKind, bool>::new);

    // Kept in sync with `open_map` on every render so the model-keyed effect
    // below (which only runs when `model` changes) can still read/merge the
    // *current* map when a hashchange fires later in that model's lifetime.
    let open_map_mirror = use_mut_ref(HashMap::<TermKind, bool>::new);
    *open_map_mirror.borrow_mut() = (*open_map).clone();

    // Re-seed on every fresh model load (matches the Vue source's
    // `watch(() => props.model, ..., { immediate: true })`), reset the
    // filter query, open whatever term anchor the URL currently names, and
    // (re-)install the `hashchange` listener for this model.
    {
        let query = query.clone();
        let open_map = open_map.clone();
        let open_map_mirror = open_map_mirror.clone();
        use_effect_with(props.model.clone(), move |model| {
            let seeded = seed_open_map(model);
            open_map.set(seeded.clone());
            *open_map_mirror.borrow_mut() = seeded;
            query.set(String::new());

            let anchor_kind = build_anchor_kind(model);
            open_hash_target(&anchor_kind, &open_map_mirror, &open_map);

            let listener = {
                let open_map = open_map.clone();
                let open_map_mirror = open_map_mirror.clone();
                Closure::wrap(Box::new(move || {
                    open_hash_target(&anchor_kind, &open_map_mirror, &open_map);
                }) as Box<dyn Fn()>)
            };
            if let Some(win) = window() {
                let _ = win.add_event_listener_with_callback(
                    "hashchange",
                    listener.as_ref().unchecked_ref(),
                );
            }

            move || {
                if let Some(win) = window() {
                    let _ = win.remove_event_listener_with_callback(
                        "hashchange",
                        listener.as_ref().unchecked_ref(),
                    );
                }
                drop(listener);
            }
        });
    }

    let normalized = query.trim().to_lowercase();
    let filtering = !normalized.is_empty();

    // Sections narrowed to the terms matching the current filter (empty
    // sections dropped) — mirrors the Vue source's `visibleSections`.
    let visible_sections: Vec<OntologySection> = if !filtering {
        props.model.sections.clone()
    } else {
        props
            .model
            .sections
            .iter()
            .map(|s| {
                let terms: Vec<Term> =
                    s.terms.iter().filter(|t| t.haystack.contains(&normalized)).cloned().collect();
                OntologySection { kind: s.kind, title: s.title.clone(), terms }
            })
            .filter(|s| !s.terms.is_empty())
            .collect()
    };

    let match_count: usize = visible_sections.iter().map(|s| s.terms.len()).sum();

    let oninput = {
        let query = query.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            query.set(input.value());
        })
    };

    html! {
        <div class="ontology-browser">
            if let Some(header) = &props.model.header {
                <header class="onto-header">
                    <div class="onto-header__title-row">
                        <h1 class="onto-header__title">{ header.title.clone() }</h1>
                        <OntoBadge label="Ontology" variant={BadgeVariant::Kind(TermKind::Ontology)} />
                    </div>
                    <a
                        href={header.iri.clone()}
                        target="_blank"
                        rel="noopener noreferrer"
                        class="onto-header__iri"
                    >{ header.iri.clone() }</a>

                    if !header.descriptions.is_empty() {
                        <div class="onto-header__descriptions">
                            { for header.descriptions.iter().map(|d| html! {
                                <p class="onto-header__description">{ d.value.clone() }</p>
                            }) }
                        </div>
                    }

                    if !header.metadata.is_empty() || !header.links.is_empty() {
                        <dl class="onto-header__meta">
                            { for header.metadata.iter().map(|m| html! {
                                <OntoAnnotation label={m.label.clone()} values={m.values.clone()} />
                            }) }
                            { for header.links.iter().map(|l| html! {
                                <div class="onto-header__link-row">
                                    <dt class="onto-header__link-label">{ l.label.clone() }</dt>
                                    <dd class="onto-header__link-refs">
                                        { for l.refs.iter().map(|r| html! {
                                            <OntoTermRef reference={r.clone()} />
                                        }) }
                                    </dd>
                                </div>
                            }) }
                        </dl>
                    }
                </header>
            }

            <OntoUmlDiagram model={props.model.clone()} prefix_links={props.prefix_links.clone()} />

            <div class="onto-filter">
                <div class="onto-filter__input-wrap">
                    <svg
                        class="onto-filter__icon"
                        width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                        stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
                    >
                        <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
                    </svg>
                    <input
                        type="search"
                        value={(*query).clone()}
                        oninput={oninput}
                        placeholder="Filter terms by name, IRI or description…"
                        class="onto-filter__input"
                        aria-label="Filter terms"
                    />
                </div>

                <nav class="onto-filter__toc">
                    { for visible_sections.iter().map(|s| html! {
                        <a href={format!("#{}", section_anchor_id(s.kind))} class="onto-filter__toc-link">
                            { s.title.clone() }
                            <span class="onto-filter__toc-count">{ s.terms.len() }</span>
                        </a>
                    }) }
                </nav>

                <p class="onto-filter__status">
                    if filtering {
                        { format!("{} of {} terms match \u{201c}{}\u{201d}", match_count, props.model.term_count, query.trim()) }
                    } else {
                        { format!("{} terms across {} categories", props.model.term_count, props.model.sections.len()) }
                    }
                </p>
            </div>

            if !visible_sections.is_empty() {
                <div class="onto-browser__sections">
                    { for visible_sections.iter().map(|s| {
                        let kind = s.kind;
                        let is_open = filtering || open_map.get(&kind).copied() == Some(true);
                        let on_toggle = {
                            let open_map = open_map.clone();
                            Callback::from(move |val: bool| {
                                let mut m = (*open_map).clone();
                                m.insert(kind, val);
                                open_map.set(m);
                            })
                        };
                        html! {
                            <OntoSection
                                id={section_anchor_id(kind)}
                                title={s.title.clone()}
                                terms={s.terms.clone()}
                                open={is_open}
                                on_toggle={on_toggle}
                            />
                        }
                    }) }
                </div>
            } else {
                <p class="onto-browser__empty">
                    { format!("No terms match \u{201c}{}\u{201d}.", query.trim()) }
                </p>
            }
        </div>
    }
}

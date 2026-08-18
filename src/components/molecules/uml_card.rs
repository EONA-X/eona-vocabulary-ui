//! MOLECULE · OntoUmlCard
//!
//! The info panel shown while a class is selected in the diagram: a drag
//! handle, a fixed header (colour swatch, name, monospace IRI), and a
//! scrollable body listing the description plus four independently
//! foldable sections — superclasses, subclasses, attributes, associations —
//! each starting open and hidden entirely when its underlying list is
//! empty. Clicking a superclass/subclass tag never navigates directly; it
//! asks the parent (via `on_focus`) to reveal and jump to that class in the
//! diagram itself (wired up in a later stage — see `OntoUmlClass`'s own
//! `on_toggle`/`on_expand` for the sibling pattern this mirrors).
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/molecules/OntoUmlCard.vue`.
//! Positioning (anchoring the card over the clicked box) is entirely the
//! caller's concern, same as the Vue source — this component only ever
//! applies its own drag offset on top of wherever the caller places it.
//!
//! `fill`: the Vue source stretches the card to a `<foreignObject>`'s full
//! height only for "Expand in layout" — set true by `OntoUmlDiagram` when
//! the selected node's box has been merged with the card (see
//! `OntoUmlDiagram`'s `expand_in_layout` state); `false` (the default) in
//! every other case, where the card floats over the box instead.
#![allow(dead_code)]

use std::collections::HashSet;

use wasm_bindgen::JsCast;
use web_sys::{Element, MouseEvent, PointerEvent};
use yew::prelude::*;

use crate::uml::UmlClassNode;

/// The four independently-foldable body sections. A small closed enum
/// instead of the source's `Set<string>` of section keys — the key set is
/// fixed and known at compile time here, so this is the pragmatic
/// Rust-idiomatic choice over stringly-typed keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum UmlCardSection {
    Superclasses,
    Subclasses,
    Attributes,
    Associations,
}

/// All four sections open — the source's `openSections` initial value.
fn all_sections_open() -> HashSet<UmlCardSection> {
    HashSet::from([
        UmlCardSection::Superclasses,
        UmlCardSection::Subclasses,
        UmlCardSection::Attributes,
        UmlCardSection::Associations,
    ])
}

/// Pointer position + drag offset captured on `pointerdown`, mirrors the
/// source's `dragStart` ref.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DragStart {
    x: f64,
    y: f64,
    ox: f64,
    oy: f64,
}

#[derive(Properties, PartialEq, Clone)]
pub struct OntoUmlCardProps {
    pub node: UmlClassNode,
    /// See the module doc comment above — always false/absent in practice
    /// in this port, accepted for structural parity with the Vue source.
    #[prop_or_default]
    pub fill: bool,
    /// Fires with a superclass/subclass's IRI when its tag is clicked.
    pub on_focus: Callback<String>,
}

#[function_component(OntoUmlCard)]
pub fn onto_uml_card(props: &OntoUmlCardProps) -> Html {
    let node = &props.node;

    let open_sections = use_state(all_sections_open);
    let toggle_section = {
        let open_sections = open_sections.clone();
        Callback::from(move |key: UmlCardSection| {
            let mut next = (*open_sections).clone();
            if !next.remove(&key) {
                next.insert(key);
            }
            open_sections.set(next);
        })
    };

    // Drag offset, applied as a translate on top of the caller's own anchor
    // positioning. Resets whenever a different class is selected (mirrors
    // the source's `watch(() => props.node.iri, ...)`); `openSections`
    // itself is NOT reset here, matching the source (folding is a per-card
    // convenience, not tied to which class is currently shown).
    let drag_x = use_state(|| 0.0_f64);
    let drag_y = use_state(|| 0.0_f64);
    let drag_start = use_state(|| None::<DragStart>);
    {
        let drag_x = drag_x.clone();
        let drag_y = drag_y.clone();
        use_effect_with(node.iri.clone(), move |_| {
            drag_x.set(0.0);
            drag_y.set(0.0);
            || ()
        });
    }

    // Pointer capture on the handle itself (set on pointerdown) means
    // move/up keep firing on it even once the pointer leaves its bounds —
    // no window-level listeners to add and clean up, same as the source.
    let ondragstart = {
        let drag_start = drag_start.clone();
        let drag_x = *drag_x;
        let drag_y = *drag_y;
        Callback::from(move |e: PointerEvent| {
            drag_start.set(Some(DragStart { x: e.client_x() as f64, y: e.client_y() as f64, ox: drag_x, oy: drag_y }));
            if let Some(el) = e.current_target().and_then(|t| t.dyn_into::<Element>().ok()) {
                let _ = el.set_pointer_capture(e.pointer_id());
            }
        })
    };
    let ondragmove = {
        let drag_start = drag_start.clone();
        let drag_x = drag_x.clone();
        let drag_y = drag_y.clone();
        Callback::from(move |e: PointerEvent| {
            let Some(start) = *drag_start else { return };
            drag_x.set(start.ox + (e.client_x() as f64 - start.x));
            drag_y.set(start.oy + (e.client_y() as f64 - start.y));
        })
    };
    let ondragend = {
        let drag_start = drag_start.clone();
        Callback::from(move |_: PointerEvent| drag_start.set(None))
    };

    let card_class = classes!("onto-uml-card", props.fill.then_some("onto-uml-card--fill"));
    let handle_class =
        classes!("onto-uml-card__handle", drag_start.is_some().then_some("onto-uml-card__handle--dragging"));
    let body_class = classes!("onto-uml-card__body", props.fill.then_some("onto-uml-card__body--fill"));
    let style = format!("transform: translate({}px, {}px)", *drag_x, *drag_y);

    let nothing_to_show = node.attributes.is_empty()
        && node.associations.is_empty()
        && node.subclasses.is_empty()
        && node.superclasses.is_empty();

    html! {
        <div class={card_class} role="tooltip" style={style}>
            <div
                class={handle_class}
                role="button"
                aria-label="Drag to move this card"
                onpointerdown={ondragstart}
                onpointermove={ondragmove}
                onpointerup={ondragend.clone()}
                onpointercancel={ondragend}
            >
                <span class="onto-uml-card__grip" aria-hidden="true" />
            </div>

            <div class="onto-uml-card__header">
                <div class="onto-uml-card__header-row">
                    <span
                        class="onto-uml-card__swatch"
                        style={format!("background-color: {}", node.color)}
                        aria-hidden="true"
                    />
                    <h4 class="onto-uml-card__name">{ node.label.clone() }</h4>
                </div>
                <p class="onto-uml-card__iri">{ node.iri.clone() }</p>
            </div>

            <div class={body_class}>
                if let Some(description) = &node.description {
                    <p class="onto-uml-card__description">{ description.clone() }</p>
                }

                <UmlCardTagSection
                    section={UmlCardSection::Superclasses}
                    title="Superclasses"
                    refs={node.superclasses.clone()}
                    open={open_sections.contains(&UmlCardSection::Superclasses)}
                    on_toggle={toggle_section.clone()}
                    on_focus={props.on_focus.clone()}
                />
                <UmlCardTagSection
                    section={UmlCardSection::Subclasses}
                    title="Subclasses"
                    refs={node.subclasses.clone()}
                    open={open_sections.contains(&UmlCardSection::Subclasses)}
                    on_toggle={toggle_section.clone()}
                    on_focus={props.on_focus.clone()}
                />

                if !node.attributes.is_empty() {
                    <div class="onto-uml-card__section">
                        { section_toggle_button(
                            "Attributes",
                            node.attributes.len(),
                            open_sections.contains(&UmlCardSection::Attributes),
                            {
                                let toggle_section = toggle_section.clone();
                                Callback::from(move |_| toggle_section.emit(UmlCardSection::Attributes))
                            },
                        ) }
                        if open_sections.contains(&UmlCardSection::Attributes) {
                            <ul class="onto-uml-card__list">
                                { for node.attributes.iter().enumerate().map(|(i, attr)| html! {
                                    <li key={i} class="onto-uml-card__list-item">
                                        <span class="onto-uml-card__list-name">{ attr.name.clone() }</span>
                                        if let Some(t) = &attr.attr_type {
                                            <span class="onto-uml-card__list-muted">{ format!(": {t}") }</span>
                                        }
                                    </li>
                                }) }
                            </ul>
                        }
                    </div>
                }

                if !node.associations.is_empty() {
                    <div class="onto-uml-card__section">
                        { section_toggle_button(
                            "Associations",
                            node.associations.len(),
                            open_sections.contains(&UmlCardSection::Associations),
                            {
                                let toggle_section = toggle_section.clone();
                                Callback::from(move |_| toggle_section.emit(UmlCardSection::Associations))
                            },
                        ) }
                        if open_sections.contains(&UmlCardSection::Associations) {
                            <ul class="onto-uml-card__list">
                                { for node.associations.iter().enumerate().map(|(i, rel)| html! {
                                    <li key={i} class="onto-uml-card__list-item">
                                        <span class="onto-uml-card__list-name">{ rel.name.clone() }</span>
                                        <span class="onto-uml-card__list-muted">{ format!("\u{2192} {}", rel.target) }</span>
                                    </li>
                                }) }
                            </ul>
                        }
                    </div>
                }

                if nothing_to_show {
                    <p class="onto-uml-card__empty">
                        { "No superclasses, subclasses, attributes or associations" }
                    </p>
                }
            </div>
        </div>
    }
}

/// A section header button shared by all four folds: rotating chevron,
/// label, and a "(N)" count — factored out since Attributes/Associations
/// render it inline (their body is a plain list) while Superclasses/
/// Subclasses render it inside `UmlCardTagSection` (their body is tags).
fn section_toggle_button(title: &str, count: usize, open: bool, onclick: Callback<MouseEvent>) -> Html {
    let chevron_class = classes!("onto-uml-card__chevron", open.then_some("onto-uml-card__chevron--open"));
    html! {
        <button
            type="button"
            class="onto-uml-card__section-toggle"
            aria-expanded={open.to_string()}
            onclick={onclick}
        >
            <svg
                width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                stroke-width="3" stroke-linecap="round" stroke-linejoin="round"
                class={chevron_class}
                aria-hidden="true"
            ><path d="m9 18 6-6-6-6" /></svg>
            { format!("{title} ({count})") }
        </button>
    }
}

#[derive(Properties, PartialEq, Clone)]
struct UmlCardTagSectionProps {
    section: UmlCardSection,
    title: &'static str,
    refs: Vec<crate::uml::UmlRef>,
    open: bool,
    on_toggle: Callback<UmlCardSection>,
    on_focus: Callback<String>,
}

/// The Superclasses/Subclasses sections: a fold of clickable tag pills
/// (`UmlRef { name, iri }`), each emitting `on_focus(iri)`. Hidden entirely
/// (not just its list) when `refs` is empty, same as the source's
/// `v-if="node.superclasses.length"` / `v-if="node.subclasses.length"`.
#[function_component(UmlCardTagSection)]
fn uml_card_tag_section(props: &UmlCardTagSectionProps) -> Html {
    if props.refs.is_empty() {
        return html! {};
    }
    let section = props.section;
    let onclick = {
        let on_toggle = props.on_toggle.clone();
        Callback::from(move |_| on_toggle.emit(section))
    };
    html! {
        <div class="onto-uml-card__section">
            { section_toggle_button(props.title, props.refs.len(), props.open, onclick) }
            if props.open {
                <div class="onto-uml-card__tags">
                    { for props.refs.iter().map(|r| {
                        let on_focus = props.on_focus.clone();
                        let iri = r.iri.clone();
                        html! {
                            <button
                                type="button"
                                key={r.iri.clone()}
                                class="onto-uml-card__tag"
                                onclick={Callback::from(move |_: MouseEvent| on_focus.emit(iri.clone()))}
                            >{ r.name.clone() }</button>
                        }
                    }) }
                </div>
            }
        </div>
    }
}

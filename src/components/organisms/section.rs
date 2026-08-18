//! ORGANISM · OntoSection
//!
//! A titled, collapsible group of term cards (one per entity kind). Open
//! state is *controlled* by the parent browser (`open` + `on_toggle`) so it
//! can auto-expand the section that owns a linked term anchor. The card list
//! is only mounted into the DOM while open — mirrors the Vue source's
//! `v-if="open"` — keeping large sections (e.g. the ~150 ODRL individuals)
//! cheap until expanded. Composes: OntoTermCard.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/organisms/OntoSection.vue`.
//! The Vue source's `update:open` emit becomes an explicit `on_toggle`
//! callback prop here — Yew has no `v-model` sugar, so the parent owns the
//! boolean `open` state and this component only ever asks to flip it.

use yew::prelude::*;

use crate::components::organisms::term_card::OntoTermCard;
use crate::ontology::Term;

#[derive(Properties, PartialEq, Clone)]
pub struct OntoSectionProps {
    pub id: AttrValue,
    pub title: AttrValue,
    pub terms: Vec<Term>,
    pub open: bool,
    pub on_toggle: Callback<bool>,
}

#[function_component(OntoSection)]
pub fn onto_section(props: &OntoSectionProps) -> Html {
    let open = props.open;
    let onclick = {
        let on_toggle = props.on_toggle.clone();
        Callback::from(move |_| on_toggle.emit(!open))
    };
    let chevron_class =
        classes!("onto-section__chevron", open.then_some("onto-section__chevron--open"));

    html! {
        <section id={props.id.clone()} class="onto-section">
            <button
                type="button"
                class="onto-section__toggle"
                aria-expanded={open.to_string()}
                onclick={onclick}
            >
                <span class="onto-section__toggle-left">
                    <svg
                        width="16"
                        height="16"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        class={chevron_class}
                        aria-hidden="true"
                    >
                        <path d="m9 18 6-6-6-6" />
                    </svg>
                    <h2 class="onto-section__title">{ props.title.clone() }</h2>
                </span>
                <span class="onto-section__count">{ props.terms.len() }</span>
            </button>

            if open {
                <div class="onto-section__list">
                    { for props.terms.iter().map(|term| html! {
                        <OntoTermCard term={term.clone()} />
                    }) }
                </div>
            }
        </section>
    }
}

//! ORGANISM · OntoTermCard
//!
//! A complete, anchorable entry for a single ontology term: header,
//! descriptions, typed relationships (as wrapped reference chips), a
//! "Properties" listing (classes only, from their `rdfs:domain` back-links),
//! notes and remaining annotations. Composes: OntoTermHeader, OntoAnnotation,
//! OntoTermRef.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/organisms/OntoTermCard.vue`.

use eona_ui_toolkit::molecules::OntoAnnotation;
use yew::prelude::*;

use crate::components::molecules::{OntoTermHeader, OntoTermRef};
use crate::ontology::Term;

#[derive(Properties, PartialEq, Clone)]
pub struct OntoTermCardProps {
    pub term: Term,
}

#[function_component(OntoTermCard)]
pub fn onto_term_card(props: &OntoTermCardProps) -> Html {
    let term = &props.term;

    html! {
        <article id={term.anchor.clone()} class="term-card">
            <OntoTermHeader term={term.clone()} />

            if !term.descriptions.is_empty() {
                <div class="term-card__descriptions">
                    { for term.descriptions.iter().map(|d| html! {
                        <p class="term-card__description">{ d.value.clone() }</p>
                    }) }
                </div>
            }

            if !term.relations.is_empty() {
                <dl class="term-card__relations">
                    { for term.relations.iter().map(|rel| html! {
                        <div class="term-card__row">
                            <dt class="term-card__row-label">{ rel.label.clone() }</dt>
                            <dd class="term-card__relation-refs">
                                { for rel.refs.iter().map(|r| html! {
                                    <OntoTermRef reference={r.clone()} />
                                }) }
                            </dd>
                        </div>
                    }) }
                </dl>
            }

            if !term.properties.is_empty() {
                <dl class="term-card__properties">
                    <div class="term-card__row">
                        <dt class="term-card__row-label">{ "Properties" }</dt>
                        <dd>
                            <ul class="term-card__properties-list">
                                { for term.properties.iter().map(|p| html! {
                                    <li class="term-card__property">
                                        <OntoTermRef reference={p.reference.clone()} />
                                        if let Some(desc) = &p.description {
                                            <span class="term-card__property-desc">{ desc.clone() }</span>
                                        }
                                    </li>
                                }) }
                            </ul>
                        </dd>
                    </div>
                </dl>
            }

            if !term.notes.is_empty() {
                <dl class="term-card__notes">
                    { for term.notes.iter().map(|note| html! {
                        <OntoAnnotation label={note.label.clone()} values={note.values.clone()} />
                    }) }
                </dl>
            }

            if !term.annotations.is_empty() {
                <dl class="term-card__annotations">
                    { for term.annotations.iter().map(|anno| html! {
                        <OntoAnnotation label={anno.label.clone()} values={anno.values.clone()} />
                    }) }
                </dl>
            }
        </article>
    }
}

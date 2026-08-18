//! MOLECULE · OntoTermRef
//!
//! A single reference to another resource. When it points at a term defined
//! on this page it becomes an in-page anchor link carrying a kind badge;
//! otherwise it renders the external IRI (linking out). Composes: OntoBadge,
//! OntoIri.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/molecules/OntoTermRef.vue`.

use yew::prelude::*;

use crate::components::atoms::{BadgeVariant, OntoBadge, OntoIri};
use crate::ontology::TermRef;

#[derive(Properties, PartialEq, Clone)]
pub struct OntoTermRefProps {
    pub reference: TermRef,
}

#[function_component(OntoTermRef)]
pub fn onto_term_ref(props: &OntoTermRefProps) -> Html {
    let reference = &props.reference;

    if reference.internal {
        let href = format!("#{}", reference.anchor.clone().unwrap_or_default());
        html! {
            <span class="term-ref">
                <a href={href} class="term-ref__link">
                    <span>{ reference.label.clone() }</span>
                    if let Some(kind) = reference.kind {
                        // `{kind:?}` on TermKind's unit variants renders exactly
                        // the TS `TermKind` string-union values ("ObjectProperty",
                        // "NamedIndividual", …) that the Vue source passes as the
                        // badge's `label` prop — no separate lookup table needed.
                        <OntoBadge label={format!("{kind:?}")} variant={BadgeVariant::from(kind)} />
                    }
                </a>
            </span>
        }
    } else {
        html! {
            <span class="term-ref">
                <OntoIri
                    iri={reference.iri.clone()}
                    curie={reference.label.clone()}
                    href={reference.iri.clone()}
                    copyable={false}
                />
            </span>
        }
    }
}

//! MOLECULE · OntoTermHeader
//!
//! The heading block of a term card: display label, a kind badge, an
//! optional "deprecated" badge, and the full IRI (copyable). Composes:
//! OntoBadge, OntoIri.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/molecules/OntoTermHeader.vue`.

use eona_ui_toolkit::atoms::{BadgeVariant, OntoBadge};
use yew::prelude::*;

use crate::components::atoms::OntoIri;
use crate::ontology::Term;

#[derive(Properties, PartialEq, Clone)]
pub struct OntoTermHeaderProps {
    pub term: Term,
}

#[function_component(OntoTermHeader)]
pub fn onto_term_header(props: &OntoTermHeaderProps) -> Html {
    let term = &props.term;
    let title_class = classes!(
        "term-header__title",
        term.deprecated.then_some("term-header__title--deprecated")
    );

    html! {
        <div class="term-header">
            <div class="term-header__row">
                <h3 class={title_class}>{ term.label.clone() }</h3>
                // See OntoTermRef for why `{kind:?}` doubles as the badge label
                // text (it matches the TS `TermKind` string union verbatim).
                <OntoBadge label={format!("{:?}", term.kind)} variant={BadgeVariant::from(term.kind)} />
                if term.deprecated {
                    <OntoBadge label="deprecated" variant={BadgeVariant::Deprecated} />
                }
            </div>
            <OntoIri iri={term.iri.clone()} curie={term.curie.clone()} href={term.iri.clone()} />
        </div>
    }
}

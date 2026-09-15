//! ATOM · OntoIri
//!
//! Renders an IRI as a compact, monospaced CURIE with the full IRI on hover
//! and a click-to-copy affordance. Optionally links out to the IRI itself.
//! Leaf atom — no dependencies on other components.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/atoms/OntoIri.vue`.
//! The clipboard write is async in both source and port: the Vue version
//! awaits `navigator.clipboard.writeText`; this port drives the equivalent
//! `web_sys` Promise through `wasm_bindgen_futures::JsFuture` inside a
//! `spawn_local` future, flipping a `copied` flag back off after ~1200ms via
//! `gloo_timers::future::TimeoutFuture` (in place of the source's
//! `setTimeout`).

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{window, MouseEvent};
use yew::prelude::*;

#[derive(Properties, PartialEq, Clone)]
pub struct OntoIriProps {
    pub iri: AttrValue,
    #[prop_or_default]
    pub curie: Option<AttrValue>,
    /// When set, the CURIE becomes an external link to this href.
    #[prop_or_default]
    pub href: Option<AttrValue>,
    #[prop_or(true)]
    pub copyable: bool,
}

#[function_component(OntoIri)]
pub fn onto_iri(props: &OntoIriProps) -> Html {
    let copied = use_state(|| false);
    let display: AttrValue = props.curie.clone().unwrap_or_else(|| props.iri.clone());

    let onclick = {
        let copied = copied.clone();
        let iri = props.iri.clone();
        Callback::from(move |event: MouseEvent| {
            event.prevent_default();
            let copied = copied.clone();
            let iri = iri.to_string();
            spawn_local(async move {
                let Some(win) = window() else { return };
                let promise = win.navigator().clipboard().write_text(&iri);
                if JsFuture::from(promise).await.is_err() {
                    // clipboard unavailable/denied — silent no-op, matching
                    // the Vue source's empty `catch`.
                    return;
                }
                copied.set(true);
                TimeoutFuture::new(1200).await;
                copied.set(false);
            });
        })
    };

    let text = if let Some(href) = &props.href {
        html! {
            <a
                href={href.clone()}
                target="_blank"
                rel="noopener noreferrer"
                title={props.iri.clone()}
                class="onto-iri__text onto-iri__text--link"
            >
                { display.clone() }
            </a>
        }
    } else {
        html! {
            <code title={props.iri.clone()} class="onto-iri__text">
                { display.clone() }
            </code>
        }
    };

    let copy_label = if *copied { "Copied" } else { "Copy IRI" };

    html! {
        <span class="onto-iri">
            { text }
            if props.copyable {
                <button
                    type="button"
                    class="onto-iri__copy"
                    aria-label={copy_label}
                    title={copy_label}
                    onclick={onclick}
                >
                    if *copied {
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M20 6 9 17l-5-5" />
                        </svg>
                    } else {
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <rect x="9" y="9" width="13" height="13" rx="2" />
                            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                        </svg>
                    }
                </button>
            }
        </span>
    }
}

//! MOLECULE · OntoUmlEdge
//!
//! One relationship in the diagram SVG: a generalization (hollow-triangle
//! head, pointing at the superclass) or a directed association (open arrow
//! head with the property name as its label). Marker ids are defined once
//! by the diagram organism (a later stage) — this component only
//! references `url(#onto-uml-inherit{-active})` / `url(#onto-uml-arrow{-active})`
//! by kind/state, it never defines the `<marker>`s itself.
//!
//! Interaction: an invisible wide "hit" path makes the thin line easy to
//! hover; `state` drives highlight ("active") / de-emphasis ("dimmed").
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/molecules/OntoUmlEdge.vue`.

use web_sys::MouseEvent;
use yew::prelude::*;

use crate::uml::{UmlEdge, UmlEdgeKind};

/// Mirrors the Vue source's `state?: "normal" | "active" | "dimmed"` prop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmlEdgeState {
    Normal,
    Active,
    Dimmed,
}

impl Default for UmlEdgeState {
    fn default() -> Self {
        UmlEdgeState::Normal
    }
}

impl UmlEdgeState {
    fn css_class(self) -> &'static str {
        match self {
            UmlEdgeState::Normal => "is-normal",
            UmlEdgeState::Active => "is-active",
            UmlEdgeState::Dimmed => "is-dimmed",
        }
    }
}

#[derive(Properties, PartialEq, Clone)]
pub struct OntoUmlEdgeProps {
    pub edge: UmlEdge,
    #[prop_or_default]
    pub state: UmlEdgeState,
    pub on_enter: Callback<()>,
    pub on_leave: Callback<()>,
}

#[function_component(OntoUmlEdge)]
pub fn onto_uml_edge(props: &OntoUmlEdgeProps) -> Html {
    let edge = &props.edge;
    // `v-if="edge.d"` in the source: an edge whose endpoints couldn't be
    // routed (e.g. a dangling reference) draws nothing at all.
    if edge.d.is_empty() {
        return html! {};
    }
    let state = props.state;

    // Rough label box so the association name stays legible over crossing
    // lines. TS measures `label.length` (UTF-16 code units); this port uses
    // `chars().count()` instead, the same pragmatic simplification
    // `uml.rs`'s own `text_width` makes.
    let label_w = edge
        .label
        .as_ref()
        .map(|l| l.chars().count() as f64 * 6.3 + 8.0)
        .unwrap_or(0.0);

    let marker_base = match edge.kind {
        UmlEdgeKind::Generalization => "onto-uml-inherit",
        UmlEdgeKind::Association => "onto-uml-arrow",
    };
    let marker_end = format!(
        "url(#{}{})",
        marker_base,
        if state == UmlEdgeState::Active { "-active" } else { "" }
    );

    let onmouseenter = {
        let on_enter = props.on_enter.clone();
        Callback::from(move |_: MouseEvent| on_enter.emit(()))
    };
    let onmouseleave = {
        let on_leave = props.on_leave.clone();
        Callback::from(move |_: MouseEvent| on_leave.emit(()))
    };

    let class = classes!("onto-uml-edge", state.css_class());

    html! {
        <g class={class} onmouseenter={onmouseenter} onmouseleave={onmouseleave}>
            // transparent hit area widens the hover target
            <path d={edge.d.clone()} fill="none" stroke="transparent" stroke-width="14" pointer-events="stroke" />
            <path d={edge.d.clone()} fill="none" class="edge-line" stroke-width="1.25" marker-end={marker_end} />
            if let Some(label) = &edge.label {
                <rect
                    x={(edge.label_x - label_w / 2.0).to_string()}
                    y={(edge.label_y - 8.0).to_string()}
                    width={label_w.to_string()}
                    height="16"
                    rx="3"
                    class="edge-label-bg"
                    opacity="0.9"
                    pointer-events="none"
                />
                <text
                    x={edge.label_x.to_string()}
                    y={(edge.label_y + 4.0).to_string()}
                    text-anchor="middle"
                    class="edge-label"
                    font-size="11"
                    pointer-events="none"
                >{ label.clone() }</text>
            }
        </g>
    }
}

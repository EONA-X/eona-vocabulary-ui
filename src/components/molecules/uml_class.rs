//! MOLECULE · OntoUmlClass
//!
//! One UML class box inside the diagram SVG: a name compartment and an
//! attribute compartment ("name: type" per line, clipped to the box). The
//! box is filled with the pastel assigned to the class's prefix; because
//! that fill is always light, the box uses dark text so it reads on both
//! the light and dark canvas.
//!
//! Clicking the box toggles a highlight of the class and its links
//! (parent-handled); the small "open" icon jumps to the class's term card
//! instead. `state` drives the active / related / dimmed styling. Rendered
//! inside an `<svg>`, so it is a `<g>`.
//!
//! In group_hierarchy mode (see `crate::uml` and the diagram organism, a
//! later stage), a class with collapsed subclasses carries a "+N" badge
//! straddling its bottom-right corner that reveals them; an already-expanded
//! class shows "−" there to collapse them back. Deliberately the corner, not
//! bottom-centre: a class's children are laid out below it, so their
//! generalization edges (aimed at this box's centre) converge right at
//! bottom-centre — a badge sitting there would overlap the very arrowheads
//! it's next to, at exactly the point they should read as touching the
//! box's edge.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/molecules/OntoUmlClass.vue`.

use web_sys::{KeyboardEvent, MouseEvent};
use yew::prelude::*;

use crate::uml::{attr_text, box_attributes, UmlClassNode};

/// Mirrors the Vue source's `state?: "normal" | "active" | "related" |
/// "dimmed"` prop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmlClassState {
    Normal,
    Active,
    Related,
    Dimmed,
}

impl Default for UmlClassState {
    fn default() -> Self {
        UmlClassState::Normal
    }
}

impl UmlClassState {
    fn css_class(self) -> &'static str {
        match self {
            UmlClassState::Normal => "is-normal",
            UmlClassState::Active => "is-active",
            UmlClassState::Related => "is-related",
            UmlClassState::Dimmed => "is-dimmed",
        }
    }
}

#[derive(Properties, PartialEq, Clone)]
pub struct OntoUmlClassProps {
    pub node: UmlClassNode,
    #[prop_or_default]
    pub state: UmlClassState,
    /// Fires on click/Enter/Space anywhere on the box (highlight toggle).
    pub on_toggle: Callback<()>,
    /// Fires on click/Enter/Space on the expand/collapse badge only
    /// (`node.child_count > 0`) — never bubbles up to also fire `on_toggle`.
    pub on_expand: Callback<()>,
}

/// A keyboard event's `.key()` is "Enter" or " " (Space) — the two keys the
/// Vue source's `@keydown.enter`/`@keydown.space` modifiers each match.
fn is_activation_key(e: &KeyboardEvent) -> bool {
    let key = e.key();
    key == "Enter" || key == " "
}

#[function_component(OntoUmlClass)]
pub fn onto_uml_class(props: &OntoUmlClassProps) -> Html {
    let node = &props.node;
    let state = props.state;

    let expand_label = if node.children_expanded {
        format!("Collapse {}'s subclasses", node.label)
    } else {
        format!(
            "Show {}'s {} subclass{}",
            node.label,
            node.child_count,
            if node.child_count == 1 { "" } else { "es" }
        )
    };

    let shown_attrs = box_attributes(node);

    let onclick = {
        let on_toggle = props.on_toggle.clone();
        Callback::from(move |_: MouseEvent| on_toggle.emit(()))
    };
    let onkeydown = {
        let on_toggle = props.on_toggle.clone();
        Callback::from(move |e: KeyboardEvent| {
            if is_activation_key(&e) {
                e.prevent_default();
                on_toggle.emit(());
            }
        })
    };

    // The "open term card" link stops the click/Enter from also bubbling up
    // to the box's own toggle handler (`@click.stop`/`@keydown.enter.stop`
    // in the source) — it only ever navigates, never re-fires `on_toggle`.
    let open_onclick = Callback::from(|e: MouseEvent| e.stop_propagation());
    let open_onkeydown = Callback::from(|e: KeyboardEvent| {
        if e.key() == "Enter" {
            e.stop_propagation();
        }
    });

    let expand_onclick = {
        let on_expand = props.on_expand.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_expand.emit(());
        })
    };
    let expand_onkeydown = {
        let on_expand = props.on_expand.clone();
        Callback::from(move |e: KeyboardEvent| {
            if is_activation_key(&e) {
                e.stop_propagation();
                e.prevent_default();
                on_expand.emit(());
            }
        })
    };

    let class = classes!("onto-uml-class", state.css_class());
    let name_x = node.x + node.w / 2.0;
    let name_y = node.y + 18.0;
    let divider_y = node.y + 28.0;
    let attr_x = node.x + 10.0;

    html! {
        <g
            class={class}
            role="button"
            tabindex="0"
            aria-pressed={(state == UmlClassState::Active).to_string()}
            aria-label={format!("Highlight {} and its links", node.label)}
            onclick={onclick}
            onkeydown={onkeydown}
        >
            <title>{ node.label.clone() }</title>
            <rect
                x={node.x.to_string()}
                y={node.y.to_string()}
                width={node.w.to_string()}
                height={node.h.to_string()}
                rx="6"
                class="uml-box"
                style={format!("fill: {}", node.color)}
            />
            // name compartment
            <text
                x={name_x.to_string()}
                y={name_y.to_string()}
                text-anchor="middle"
                class="uml-name"
                font-size="13"
                font-weight="600"
            >{ node.label_display.clone() }</text>
            <line
                x1={node.x.to_string()}
                y1={divider_y.to_string()}
                x2={(node.x + node.w).to_string()}
                y2={divider_y.to_string()}
                class="uml-divider"
                stroke-width="1"
            />
            // attributes
            { for shown_attrs.iter().enumerate().map(|(i, attr)| {
                let y = node.y + 28.0 + 14.0 + i as f64 * 18.0;
                html! {
                    <text x={attr_x.to_string()} y={y.to_string()} class="uml-attr" font-size="12">
                        <title>{ attr_text(attr) }</title>
                        { attr.display.clone() }
                    </text>
                }
            }) }
            if node.hidden_count > 0 {
                <text
                    x={attr_x.to_string()}
                    y={(node.y + 28.0 + 14.0 + shown_attrs.len() as f64 * 18.0).to_string()}
                    class="uml-attr uml-more"
                    font-size="12"
                >
                    <title>{ node.attributes.iter().skip(shown_attrs.len()).map(attr_text).collect::<Vec<_>>().join("\n") }</title>
                    { format!("+{} more…", node.hidden_count) }
                </text>
            }
            // open the class's term card (does not toggle the highlight)
            if !node.anchor.is_empty() {
                <a
                    href={format!("#{}", node.anchor)}
                    class="uml-open"
                    aria-label={format!("Open {} details", node.label)}
                    onclick={open_onclick}
                    onkeydown={open_onkeydown}
                >
                    <title>{ format!("Open {} details", node.label) }</title>
                    <rect
                        x={(node.x + node.w - 22.0).to_string()}
                        y={(node.y + 4.0).to_string()}
                        width="18"
                        height="18"
                        rx="3"
                        fill="transparent"
                        pointer-events="all"
                    />
                    <g
                        transform={format!("translate({}, {}) scale(0.5)", node.x + node.w - 20.0, node.y + 6.0)}
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M15 3h6v6" />
                        <path d="M10 14 21 3" />
                        <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h6" />
                    </g>
                </a>
            }
            // expand/collapse badge for subclasses (group_hierarchy mode only)
            if node.child_count > 0 {
                <g
                    class="uml-expand"
                    role="button"
                    tabindex="0"
                    aria-label={expand_label.clone()}
                    onclick={expand_onclick}
                    onkeydown={expand_onkeydown}
                >
                    <title>{ expand_label }</title>
                    <circle
                        cx={(node.x + node.w - 14.0).to_string()}
                        cy={(node.y + node.h).to_string()}
                        r="10"
                        class="uml-expand-badge"
                    />
                    <text
                        x={(node.x + node.w - 14.0).to_string()}
                        y={(node.y + node.h + 4.0).to_string()}
                        text-anchor="middle"
                        class="uml-expand-label"
                        font-size="12"
                        font-weight="700"
                    >{ if node.children_expanded { "−".to_string() } else { format!("+{}", node.child_count) } }</text>
                </g>
            }
        </g>
    }
}

//! MOLECULE · OntoUmlTreeNode
//!
//! One row of the "Class tree" sidebar panel (see `OntoUmlDiagram`'s "Class
//! tree" switch): a chevron (only when it has children) plus the class
//! name, recursing into its own children when expanded. Plain HTML, not
//! SVG — an outline/list view alongside the diagram, not a replacement for
//! it.
//!
//! Selection and expand state are lifted to the root `OntoUmlDiagram`
//! (`selected` and `tree_expanded`), not owned per-node, so a jump from
//! elsewhere in the UI (e.g. a subclass tag) can expand a whole ancestor
//! chain and highlight the target in one go.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/molecules/OntoUmlTreeNode.vue`.

use std::collections::HashSet;

use yew::prelude::*;

use crate::uml::UmlTreeNode;

#[derive(Properties, PartialEq, Clone)]
pub struct OntoUmlTreeNodeProps {
    pub node: UmlTreeNode,
    pub selected: Option<String>,
    pub expanded: HashSet<String>,
    pub on_select: Callback<String>,
    pub on_toggle: Callback<String>,
}

#[function_component(OntoUmlTreeNode)]
pub fn onto_uml_tree_node(props: &OntoUmlTreeNodeProps) -> Html {
    let node = &props.node;
    let is_open = props.expanded.contains(&node.iri);
    let has_children = !node.children.is_empty();

    let onselect = {
        let on_select = props.on_select.clone();
        let iri = node.iri.clone();
        Callback::from(move |_: MouseEvent| on_select.emit(iri.clone()))
    };
    let ontoggle = {
        let on_toggle = props.on_toggle.clone();
        let iri = node.iri.clone();
        Callback::from(move |e: MouseEvent| {
            e.stop_propagation();
            on_toggle.emit(iri.clone());
        })
    };

    let row_class = classes!(
        "onto-uml-tree-node__row",
        (props.selected.as_deref() == Some(node.iri.as_str())).then_some("onto-uml-tree-node__row--selected")
    );
    let chevron_class = classes!("onto-uml-tree-node__chevron", is_open.then_some("onto-uml-tree-node__chevron--open"));

    html! {
        <div>
            <div
                data-tree-iri={node.iri.clone()}
                title={node.label.clone()}
                class={row_class}
                role="button"
                tabindex="0"
                onclick={onselect}
            >
                if has_children {
                    <button
                        type="button"
                        class="onto-uml-tree-node__toggle"
                        aria-label={if is_open { format!("Collapse {}", node.label) } else { format!("Expand {}", node.label) }}
                        onclick={ontoggle}
                    >
                        <svg
                            width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                            stroke-width="3" stroke-linecap="round" stroke-linejoin="round"
                            class={chevron_class}
                            aria-hidden="true"
                        ><path d="m9 18 6-6-6-6" /></svg>
                    </button>
                } else {
                    <span class="onto-uml-tree-node__toggle-spacer" aria-hidden="true" />
                }
                <span class="onto-uml-tree-node__label">{ node.label.clone() }</span>
                if has_children {
                    <span class="onto-uml-tree-node__count">{ format!("({})", node.children.len()) }</span>
                }
            </div>
            if has_children && is_open {
                <div class="onto-uml-tree-node__children">
                    { for node.children.iter().map(|child| html! {
                        <OntoUmlTreeNode
                            key={child.iri.clone()}
                            node={child.clone()}
                            selected={props.selected.clone()}
                            expanded={props.expanded.clone()}
                            on_select={props.on_select.clone()}
                            on_toggle={props.on_toggle.clone()}
                        />
                    }) }
                </div>
            }
        </div>
    }
}

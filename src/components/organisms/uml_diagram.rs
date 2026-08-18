//! ORGANISM · OntoUmlDiagram
//!
//! A UML class diagram of the ontology, rendered as inline SVG: classes
//! become boxes (name + attributes), rdfs:subClassOf becomes generalizations
//! and object properties become directed associations (see `crate::uml`).
//! Collapsible, and horizontally/vertically scrollable so large ontologies
//! stay usable. Renders nothing when the ontology has no classes. Composes:
//! `OntoUmlClass`, `OntoUmlEdge`, `OntoUmlCard`.
//!
//! Ports `containers/prez-ui/theme/app/components/ontology/organisms/OntoUmlDiagram.vue`.
//!
//! Includes the "Class tree" sidebar (`tree_view`/`OntoUmlTreeNode`/
//! `class_tree`/`focus_tree`), "Expand in layout" (`expand_in_layout`, the
//! foreignObject card-merge render path), and fullscreen mode — all three
//! were deferred by an earlier migration stage and ported in later,
//! individually-committed follow-up stages.
//!
//! Wired into the page by `organisms::browser::OntologyBrowser`, between the
//! ontology header and the filter/TOC card — matching where the Vue source's
//! `OntologyBrowser.vue` places `<OntoUmlDiagram>`.

use std::collections::{HashMap, HashSet};

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, MouseEvent, ScrollIntoViewOptions, ScrollLogicalPosition, WheelEvent};
use yew::prelude::*;

use crate::components::molecules::{OntoUmlCard, OntoUmlClass, OntoUmlEdge, OntoUmlTreeNode, UmlClassState, UmlEdgeState};
use crate::ontology::{OntologyModel, PREFIXES};
use crate::uml::{ancestor_chain, build_uml_diagram, class_tree, expandable_iris, UmlDiagram, UmlEdge, UmlLayoutOptions};

const ZOOM_MIN: f64 = 0.25;
const ZOOM_MAX: f64 = 2.0;
const ZOOM_STEP: f64 = 0.25;
// Matches OntoUmlCard's own `w-[41.6em]` at the default 16px root, and a
// generous upper-bound estimate of its height (header + up to 13 lines +
// padding) — see `tooltip_pos` below, and the Vue source's own CARD_W/
// CARD_MAXH_ESTIMATE consts (kept here rather than in uml.rs since, with
// "Expand in layout" out of scope, they're only ever needed for the
// floating-card clamp, never for layout itself).
const CARD_W: f64 = 666.0;
const CARD_MAXH_ESTIMATE: f64 = 416.0;
// Large diagrams start zoomed out a bit so the initial view isn't a wall of
// boxes (see the on-mount effect below).
const AUTO_ZOOM_OUT_NODE_COUNT: usize = 50;
const AUTO_ZOOM_OUT_LEVEL: f64 = 0.5;

#[derive(Properties, PartialEq, Clone)]
pub struct OntoUmlDiagramProps {
    pub model: OntologyModel,
    /// prefix -> slug, for ontologies published in this browser (see
    /// `crate::components::organisms::selector`).
    #[prop_or_default]
    pub prefix_links: Option<HashMap<String, String>>,
}

/// Diagram derivation for the current `group_hierarchy`/`expanded` state —
/// factored out so both the render body and every interaction callback
/// (which each need to know a freshly-collapsed/expanded/regrouped layout's
/// coordinates, not the one from before their own state change) can call
/// the same pure computation `build_uml_diagram` already is.
fn diagram_for(model: &OntologyModel, group_hierarchy: bool, expanded: &HashSet<String>) -> UmlDiagram {
    build_uml_diagram(
        model,
        &UmlLayoutOptions {
            group_hierarchy,
            expanded: Some(expanded.clone()),
            expanded_card_iri: None,
            expanded_card_size: None,
        },
    )
}

/// Scroll `scroll_ref`'s element so unscaled diagram point `(x, y)` centres
/// in its viewport — mirrors the Vue source's `centerOn`. A no-op if the
/// ref isn't attached to a DOM element (e.g. the diagram is closed).
fn scroll_center(scroll_ref: &NodeRef, zoom: f64, x: f64, y: f64) {
    let Some(el) = scroll_ref.cast::<Element>() else { return };
    let client_w = el.client_width() as f64;
    let client_h = el.client_height() as f64;
    el.set_scroll_left((x * zoom - client_w / 2.0).round() as i32);
    el.set_scroll_top((y * zoom - client_h / 2.0).round() as i32);
}

fn scroll_center_on_graph(scroll_ref: &NodeRef, zoom: f64, diagram: &UmlDiagram) {
    scroll_center(scroll_ref, zoom, diagram.width / 2.0, diagram.height / 2.0);
}

/// Undirected adjacency, for "class + its links" highlighting (mirrors the
/// Vue source's `neighbours` computed).
fn compute_neighbours(edges: &[UmlEdge]) -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for e in edges {
        if e.source == e.target {
            continue;
        }
        map.entry(e.source.clone()).or_default().insert(e.target.clone());
        map.entry(e.target.clone()).or_default().insert(e.source.clone());
    }
    map
}

fn node_state(selected: Option<&str>, neighbours: &HashMap<String, HashSet<String>>, iri: &str) -> UmlClassState {
    let Some(sel) = selected else { return UmlClassState::Normal };
    if sel == iri {
        return UmlClassState::Active;
    }
    if neighbours.get(sel).is_some_and(|ns| ns.contains(iri)) {
        UmlClassState::Related
    } else {
        UmlClassState::Dimmed
    }
}

fn edge_state(hover_edge: Option<usize>, selected: Option<&str>, edge: &UmlEdge, index: usize) -> UmlEdgeState {
    if hover_edge == Some(index) {
        return UmlEdgeState::Active;
    }
    let Some(sel) = selected else { return UmlEdgeState::Normal };
    if edge.source == sel || edge.target == sel {
        UmlEdgeState::Active
    } else {
        UmlEdgeState::Dimmed
    }
}

/// One entry in the prefix colour legend (only rendered when there's more
/// than one distinct prefix — see the caller). Mirrors the Vue source's
/// `prefixLegend` computed.
struct PrefixLegendEntry {
    prefix: String,
    color: String,
    /// This app's own page ("/") for a prefix published in this browser.
    slug: Option<String>,
    /// Well-known external vocabulary namespace IRI, from `ontology::PREFIXES`.
    external: Option<&'static str>,
}

fn prefix_legend(diagram: &UmlDiagram, prefix_links: Option<&HashMap<String, String>>) -> Vec<PrefixLegendEntry> {
    let mut color_of: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for n in &diagram.nodes {
        if !color_of.contains_key(&n.prefix) {
            color_of.insert(n.prefix.clone(), n.color.clone());
            order.push(n.prefix.clone());
        }
    }
    let mut entries: Vec<PrefixLegendEntry> = order
        .into_iter()
        .map(|prefix| {
            let color = color_of[&prefix].clone();
            let slug = prefix_links.and_then(|m| m.get(&prefix).cloned());
            let external = PREFIXES.iter().find(|(p, _)| *p == prefix.as_str()).map(|(_, ns)| *ns);
            PrefixLegendEntry { prefix, color, slug, external }
        })
        .collect();
    // Case-insensitive, same pragmatic `localeCompare` stand-in uml.rs's own
    // `cmp_label` makes.
    entries.sort_by(|a, b| a.prefix.to_lowercase().cmp(&b.prefix.to_lowercase()));
    entries
}

/// The source's `CSS.escape` call on an IRI before interpolating it into a
/// `[data-tree-iri="..."]` attribute selector — escaping just the two
/// characters that would otherwise break out of the selector's quoted
/// string (IRIs are URLs, so anything fancier than a stray quote/backslash
/// is effectively never seen in practice).
fn css_attr_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Reveal `iri` in the class tree (expanding every collapsed ancestor
/// between it and its own root — a forest node with no listed ancestors is
/// already visible, so this is a no-op for a root) and scroll it into view
/// within the tree panel — mirrors the Vue source's `focusTree`. Whatever
/// becomes `selected` (a box click, a subclass tag jump, or the tree
/// itself) gets revealed and scrolled to here too, so the tree never drifts
/// out of sync with whatever the diagram is currently showing.
fn focus_tree(model: &OntologyModel, iri: String, tree_expanded: UseStateHandle<HashSet<String>>, tree_ref: NodeRef) {
    let chain = ancestor_chain(model, &iri);
    if !chain.is_empty() {
        let mut next = (*tree_expanded).clone();
        for a in chain {
            next.insert(a);
        }
        tree_expanded.set(next);
    }
    spawn_local(async move {
        TimeoutFuture::new(0).await;
        let Some(el) = tree_ref.cast::<Element>() else { return };
        let selector = format!("[data-tree-iri=\"{}\"]", css_attr_escape(&iri));
        let Ok(Some(target)) = el.query_selector(&selector) else { return };
        let opts = ScrollIntoViewOptions::new();
        opts.set_block(ScrollLogicalPosition::Nearest);
        target.scroll_into_view_with_scroll_into_view_options(&opts);
    });
}

fn switch_button(on: bool, onclick: Callback<MouseEvent>) -> Html {
    let switch_class = classes!("onto-uml-diagram__switch", on.then_some("onto-uml-diagram__switch--on"));
    html! {
        <button
            type="button"
            role="switch"
            aria-checked={on.to_string()}
            class={switch_class}
            onclick={onclick}
        >
            <span class="onto-uml-diagram__switch-thumb" />
        </button>
    }
}

#[function_component(OntoUmlDiagram)]
pub fn onto_uml_diagram(props: &OntoUmlDiagramProps) -> Html {
    let group_hierarchy = use_state(|| false);
    let expanded = use_state(HashSet::<String>::new);
    let expand_all = use_state(|| false);
    let selected = use_state(|| None::<String>);
    let hover_edge = use_state(|| None::<usize>);
    let open = use_state(|| true);
    let zoom = use_state(|| 1.0_f64);
    let scroll_ref = use_node_ref();

    // "Class tree" sidebar — its own expand state is independent of the
    // diagram's own `expanded` (group_hierarchy): a different view, starting
    // fully collapsed to just the roots, then growing as the viewer opens
    // branches or as `selected` changes (see the two watchers below) — never
    // tied to the diagram's own state.
    let tree_view = use_state(|| false);
    let tree_expanded = use_state(HashSet::<String>::new);
    let tree_ref = use_node_ref();

    let diagram = diagram_for(&props.model, *group_hierarchy, &expanded);

    // Reset selection/hover whenever the canvas reshapes for a reason OTHER
    // than the selection itself — grouping, an expand/collapse, or a new
    // ontology. Deliberately not keyed on `diagram` itself (which would
    // also fire from `expandInLayout`-driven changes in the Vue source);
    // with that feature out of scope here, `group_hierarchy`/`expanded`/
    // `model` are exactly the diagram's real dependencies anyway.
    {
        let selected = selected.clone();
        let hover_edge = hover_edge.clone();
        use_effect_with((props.model.clone(), *group_hierarchy, (*expanded).clone()), move |_| {
            selected.set(None);
            hover_edge.set(None);
            || ()
        });
    }

    // On first mount only: large diagrams start zoomed out a bit, then the
    // view centres on the whole (possibly rescaled) canvas once it has its
    // final on-screen size.
    {
        let zoom = zoom.clone();
        let scroll_ref = scroll_ref.clone();
        let model = props.model.clone();
        use_effect_with((), move |_| {
            let initial = build_uml_diagram(&model, &UmlLayoutOptions::default());
            let z = if initial.nodes.len() > AUTO_ZOOM_OUT_NODE_COUNT {
                zoom.set(AUTO_ZOOM_OUT_LEVEL);
                AUTO_ZOOM_OUT_LEVEL
            } else {
                *zoom
            };
            spawn_local(async move {
                TimeoutFuture::new(0).await;
                scroll_center_on_graph(&scroll_ref, z, &initial);
            });
            || ()
        });
    }

    // Whatever becomes `selected` gets revealed + scrolled to in the tree
    // too, while the tree is open — mirrors the Vue source's
    // `watch(selected, (iri) => { if (iri && treeView.value) focusTree(iri); })`.
    {
        let tree_view = tree_view.clone();
        let tree_expanded = tree_expanded.clone();
        let tree_ref = tree_ref.clone();
        let model = props.model.clone();
        use_effect_with((*selected).clone(), move |sel| {
            if let (Some(iri), true) = (sel.clone(), *tree_view) {
                focus_tree(&model, iri, tree_expanded, tree_ref);
            }
            || ()
        });
    }

    // Opening the tree view jumps straight to wherever the diagram's own
    // selection already is — mirrors the Vue source's
    // `watch(treeView, (on) => { if (on && selected.value) focusTree(selected.value); })`.
    {
        let selected = selected.clone();
        let tree_expanded = tree_expanded.clone();
        let tree_ref = tree_ref.clone();
        let model = props.model.clone();
        use_effect_with(*tree_view, move |on| {
            if *on {
                if let Some(iri) = (*selected).clone() {
                    focus_tree(&model, iri, tree_expanded, tree_ref);
                }
            }
            || ()
        });
    }

    if diagram.nodes.is_empty() {
        return Html::default();
    }

    let toggle_open = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(!*open))
    };

    let clear_selection = {
        let selected = selected.clone();
        Callback::from(move |_: MouseEvent| selected.set(None))
    };

    let select_class = {
        let selected = selected.clone();
        Callback::from(move |iri: String| {
            selected.set(if (*selected).as_deref() == Some(iri.as_str()) { None } else { Some(iri) });
        })
    };

    // "+N"/"−" badge click (group_hierarchy mode only) — re-centres on
    // either the collapsed node itself or its cluster's own circle centre,
    // same logic as the Vue source's `toggleExpand`.
    let toggle_expand = {
        let expanded = expanded.clone();
        let group_hierarchy = group_hierarchy.clone();
        let scroll_ref = scroll_ref.clone();
        let zoom = zoom.clone();
        let model = props.model.clone();
        Callback::from(move |iri: String| {
            let was_expanded = expanded.contains(&iri);
            let mut next = (*expanded).clone();
            if was_expanded {
                next.remove(&iri);
            } else {
                next.insert(iri.clone());
            }
            expanded.set(next.clone());

            let scroll_ref = scroll_ref.clone();
            let model = model.clone();
            let group_hierarchy_val = *group_hierarchy;
            let zoom_val = *zoom;
            spawn_local(async move {
                TimeoutFuture::new(0).await;
                let diagram = diagram_for(&model, group_hierarchy_val, &next);
                let Some(n) = diagram.nodes.iter().find(|x| x.iri == iri) else { return };
                if was_expanded {
                    scroll_center(&scroll_ref, zoom_val, n.x + n.w / 2.0, n.y + n.h / 2.0);
                } else {
                    let (cx, cy) = diagram
                        .clusters
                        .iter()
                        .find(|c| n.cluster_root.as_deref() == Some(c.root.as_str()))
                        .map(|c| (c.cx, c.cy))
                        .unwrap_or((n.x + n.w / 2.0, n.y + n.h / 2.0));
                    scroll_center(&scroll_ref, zoom_val, cx, cy);
                }
            });
        })
    };

    // "Group hierarchy" switch: turning it on (or off) starts/returns fully
    // collapsed and re-centres on the whole (reshaped) canvas — mirrors the
    // Vue source's `watch(groupHierarchy, ...)`.
    let on_group_hierarchy_click = {
        let group_hierarchy = group_hierarchy.clone();
        let expand_all = expand_all.clone();
        let expanded = expanded.clone();
        let scroll_ref = scroll_ref.clone();
        let zoom = zoom.clone();
        let model = props.model.clone();
        Callback::from(move |_: MouseEvent| {
            let new_on = !*group_hierarchy;
            group_hierarchy.set(new_on);
            let next_expanded = if new_on {
                expand_all.set(false);
                let empty = HashSet::new();
                expanded.set(empty.clone());
                empty
            } else {
                (*expanded).clone()
            };

            let scroll_ref = scroll_ref.clone();
            let model = model.clone();
            let zoom_val = *zoom;
            spawn_local(async move {
                TimeoutFuture::new(0).await;
                let diagram = diagram_for(&model, new_on, &next_expanded);
                scroll_center_on_graph(&scroll_ref, zoom_val, &diagram);
            });
        })
    };

    // "Expand all" / "Collapse all" — a bulk action, not a live-synced
    // indicator (see `crate::uml::expandable_iris`); individual "+N"/"−"
    // badges can still fine-tune from there afterwards.
    let on_expand_all_click = {
        let expand_all = expand_all.clone();
        let expanded = expanded.clone();
        let group_hierarchy = group_hierarchy.clone();
        let scroll_ref = scroll_ref.clone();
        let zoom = zoom.clone();
        let model = props.model.clone();
        Callback::from(move |_: MouseEvent| {
            let new_val = !*expand_all;
            expand_all.set(new_val);
            let next_expanded = if new_val { expandable_iris(&model) } else { HashSet::new() };
            expanded.set(next_expanded.clone());

            let scroll_ref = scroll_ref.clone();
            let model = model.clone();
            let group_hierarchy_val = *group_hierarchy;
            let zoom_val = *zoom;
            spawn_local(async move {
                TimeoutFuture::new(0).await;
                let diagram = diagram_for(&model, group_hierarchy_val, &next_expanded);
                scroll_center_on_graph(&scroll_ref, zoom_val, &diagram);
            });
        })
    };

    let zoom_out = {
        let zoom = zoom.clone();
        Callback::from(move |_: MouseEvent| {
            let z = ((*zoom - ZOOM_STEP) * 100.0).round() / 100.0;
            zoom.set(z.max(ZOOM_MIN));
        })
    };
    let zoom_in = {
        let zoom = zoom.clone();
        Callback::from(move |_: MouseEvent| {
            let z = ((*zoom + ZOOM_STEP) * 100.0).round() / 100.0;
            zoom.set(z.min(ZOOM_MAX));
        })
    };
    let zoom_reset = {
        let zoom = zoom.clone();
        Callback::from(move |_: MouseEvent| zoom.set(1.0))
    };
    // Trackpad pinch-zoom and Ctrl+mouse-wheel both arrive as the same
    // `wheel` event with `ctrlKey: true` — one handler covers both.
    // Exponential step so it feels smooth/proportional at any zoom level;
    // plain scrolling (no Ctrl, no pinch) is left untouched to pan the
    // canvas normally (the browser's native scroll, nothing custom needed).
    let onwheel = {
        let zoom = zoom.clone();
        Callback::from(move |e: WheelEvent| {
            if !e.ctrl_key() {
                return;
            }
            e.prevent_default();
            let z = (*zoom * (-e.delta_y() * 0.001).exp()).max(ZOOM_MIN).min(ZOOM_MAX);
            zoom.set(z);
        })
    };

    // A subclass/superclass tag in the floating card jumps straight to that
    // class in the diagram: reveal it (expanding every collapsed ancestor
    // between it and its cluster's root, a no-op outside group_hierarchy),
    // re-centre on it, and select it — same as clicking its own box would.
    // Selecting has to happen after the expand takes effect (the reset
    // effect above would otherwise immediately clear it again), hence the
    // same `TimeoutFuture(0)` "next tick" approximation `browser.rs` uses.
    let focus_class = {
        let open = open.clone();
        let group_hierarchy = group_hierarchy.clone();
        let expanded = expanded.clone();
        let selected = selected.clone();
        let scroll_ref = scroll_ref.clone();
        let zoom = zoom.clone();
        let model = props.model.clone();
        Callback::from(move |iri: String| {
            open.set(true);
            let group_hierarchy_val = *group_hierarchy;
            let mut next_expanded = (*expanded).clone();
            if group_hierarchy_val {
                let chain = ancestor_chain(&model, &iri);
                if !chain.is_empty() {
                    for a in chain {
                        next_expanded.insert(a);
                    }
                    expanded.set(next_expanded.clone());
                }
            }

            let selected = selected.clone();
            let scroll_ref = scroll_ref.clone();
            let model = model.clone();
            let zoom_val = *zoom;
            let iri = iri.clone();
            spawn_local(async move {
                TimeoutFuture::new(0).await;
                selected.set(Some(iri.clone()));
                let diagram = diagram_for(&model, group_hierarchy_val, &next_expanded);
                if let Some(n) = diagram.nodes.iter().find(|x| x.iri == iri) {
                    scroll_center(&scroll_ref, zoom_val, n.x + n.w / 2.0, n.y + n.h / 2.0);
                }
            });
        })
    };

    let toggle_tree_node = {
        let tree_expanded = tree_expanded.clone();
        Callback::from(move |iri: String| {
            let mut next = (*tree_expanded).clone();
            if !next.remove(&iri) {
                next.insert(iri);
            }
            tree_expanded.set(next);
        })
    };
    // Always the full, ungrouped forest (see `crate::uml::class_tree`) —
    // independent of `group_hierarchy`/`expanded` — only computed while the
    // panel is actually open.
    let tree = if *tree_view { class_tree(&props.model) } else { Vec::new() };

    let neighbours = compute_neighbours(&diagram.edges);
    let selected_iri: Option<&str> = (*selected).as_deref();
    let selected_node = diagram.nodes.iter().find(|n| Some(n.iri.as_str()) == selected_iri).cloned();
    // Anchored at the node's own top-left (clamped so it stays on-canvas),
    // in scaled coordinates — rendered outside the zoom-transformed wrapper
    // (see the template below) so the card itself always reads at 100%
    // regardless of diagram zoom; only its anchor point scales with `zoom`.
    let tooltip_pos = selected_node.as_ref().map(|n| {
        let scaled_w = diagram.width * *zoom;
        let scaled_h = diagram.height * *zoom;
        let left = (n.x * *zoom).min(scaled_w - CARD_W - 4.0).max(4.0);
        let top = (n.y * *zoom).min(scaled_h - CARD_MAXH_ESTIMATE).max(4.0);
        (left, top)
    });

    let prefixes = prefix_legend(&diagram, props.prefix_links.as_ref());

    let chevron_class = classes!("onto-uml-diagram__chevron", (*open).then_some("onto-uml-diagram__chevron--open"));
    let group_switch_label = {
        let group_hierarchy = group_hierarchy.clone();
        Callback::from(move |_: MouseEvent| group_hierarchy.set(!*group_hierarchy))
    };
    let expand_all_switch_label = {
        let cb = on_expand_all_click.clone();
        Callback::from(move |e: MouseEvent| cb.emit(e))
    };

    html! {
        <section class="onto-uml-diagram" aria-label="Class diagram">
            <button
                type="button"
                class="onto-uml-diagram__toggle"
                aria-expanded={(*open).to_string()}
                onclick={toggle_open}
            >
                <span class="onto-uml-diagram__toggle-left">
                    <svg
                        width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                        stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                        class={chevron_class}
                        aria-hidden="true"
                    >
                        <path d="m9 18 6-6-6-6" />
                    </svg>
                    <h2 class="onto-uml-diagram__title">{ "Class diagram" }</h2>
                </span>
                <span class="onto-uml-diagram__count">{ format!("{} classes", diagram.nodes.len()) }</span>
            </button>

            if *open {
                <div class="onto-uml-diagram__body">
                    <div class="onto-uml-diagram__legend">
                        <span class="onto-uml-diagram__legend-item">
                            <svg width="34" height="10" aria-hidden="true">
                                <line x1="0" y1="5" x2="26" y2="5" class="onto-uml-diagram__legend-line" stroke-width="1.25" />
                                <path d="M26,1 L33,5 L26,9 Z" class="onto-uml-diagram__legend-tri" stroke-width="1.25" />
                            </svg>
                            { "subclass of" }
                        </span>
                        <span class="onto-uml-diagram__legend-item">
                            <svg width="34" height="10" aria-hidden="true">
                                <line x1="0" y1="5" x2="28" y2="5" class="onto-uml-diagram__legend-line" stroke-width="1.25" />
                                <path d="M27,1 L33,5 L27,9" fill="none" class="onto-uml-diagram__legend-line" stroke-width="1.25" />
                            </svg>
                            { "association (object property)" }
                        </span>
                        <span>{ "attributes listed inside each class" }</span>
                        <span class="onto-uml-diagram__legend-muted">
                            { "\u{b7}  hover a link to highlight it, click a class to highlight its links" }
                        </span>
                    </div>

                    if prefixes.len() > 1 {
                        <div class="onto-uml-diagram__prefixes">
                            <span class="onto-uml-diagram__prefixes-label">
                                <span class="onto-uml-diagram__prefixes-title">{ "Prefixes:" }</span>
                                <span
                                    class="onto-uml-diagram__help"
                                    title="Namespace prefixes referenced by classes in this diagram. Prefixes for ontologies published in this browser link to their page; well-known external vocabularies link to their namespace."
                                    aria-label="Namespace prefixes referenced by classes in this diagram. Prefixes for ontologies published in this browser link to their page; well-known external vocabularies link to their namespace."
                                >{ "?" }</span>
                            </span>
                            { for prefixes.iter().map(|p| html! {
                                <span key={p.prefix.clone()} class="onto-uml-diagram__prefix">
                                    <span
                                        class="onto-uml-diagram__prefix-swatch"
                                        style={format!("background-color: {}", p.color)}
                                        aria-hidden="true"
                                    />
                                    if let Some(slug) = &p.slug {
                                        <a href={format!("/?ontology={slug}")} class="onto-uml-diagram__prefix-link">
                                            { p.prefix.clone() }
                                        </a>
                                    } else if let Some(external) = p.external {
                                        <a
                                            href={external}
                                            target="_blank"
                                            rel="noopener noreferrer"
                                            title={external}
                                            class="onto-uml-diagram__prefix-link"
                                        >{ p.prefix.clone() }</a>
                                    } else {
                                        <span>{ if p.prefix.is_empty() { "(no prefix)".to_string() } else { p.prefix.clone() } }</span>
                                    }
                                </span>
                            }) }
                        </div>
                    }

                    <div class="onto-uml-diagram__controls">
                        <div class="onto-uml-diagram__controls-left">
                            <label class="onto-uml-diagram__switch-label">
                                { switch_button(*group_hierarchy, on_group_hierarchy_click) }
                                <span onclick={group_switch_label}>{ "Group hierarchy" }</span>
                                <span
                                    class="onto-uml-diagram__help"
                                    title="Group each superclass and its subclasses into its own circular cluster, starting fully collapsed. Click a class's \u{201c}+N\u{201d} badge to reveal its subclasses, or \u{201c}\u{2212}\u{201d} to collapse them again."
                                    aria-label="Group each superclass and its subclasses into its own circular cluster, starting fully collapsed. Click a class's plus-N badge to reveal its subclasses, or minus to collapse them again."
                                >{ "?" }</span>
                            </label>

                            if *group_hierarchy {
                                <label class="onto-uml-diagram__switch-label">
                                    { switch_button(*expand_all, on_expand_all_click.clone()) }
                                    <span onclick={expand_all_switch_label}>
                                        { if *expand_all { "Collapse all" } else { "Expand all" } }
                                    </span>
                                </label>
                            }

                            <label class="onto-uml-diagram__switch-label">
                                { switch_button(*tree_view, {
                                    let tree_view = tree_view.clone();
                                    Callback::from(move |_: MouseEvent| tree_view.set(!*tree_view))
                                }) }
                                <span onclick={{
                                    let tree_view = tree_view.clone();
                                    Callback::from(move |_: MouseEvent| tree_view.set(!*tree_view))
                                }}>{ "Class tree" }</span>
                                <span
                                    class="onto-uml-diagram__help"
                                    title="Show every class as a plain outline alongside the diagram. Clicking a class there (or anywhere else) reveals and centres it in the diagram too, and expands the outline down to it."
                                    aria-label="Show every class as a plain outline alongside the diagram. Clicking a class there or anywhere else reveals and centres it in the diagram too, and expands the outline down to it."
                                >{ "?" }</span>
                            </label>
                        </div>

                        <div class="onto-uml-diagram__controls-right">
                            <div class="onto-uml-diagram__zoom">
                                <button
                                    type="button"
                                    class="onto-uml-diagram__zoom-btn onto-uml-diagram__zoom-btn--minus"
                                    disabled={*zoom <= ZOOM_MIN}
                                    aria-label="Zoom out"
                                    onclick={zoom_out}
                                >{ "\u{2212}" }</button>
                                <button
                                    type="button"
                                    class="onto-uml-diagram__zoom-pct"
                                    title="Reset zoom to 100%"
                                    onclick={zoom_reset}
                                >{ format!("{}%", (*zoom * 100.0).round() as i64) }</button>
                                <button
                                    type="button"
                                    class="onto-uml-diagram__zoom-btn onto-uml-diagram__zoom-btn--plus"
                                    disabled={*zoom >= ZOOM_MAX}
                                    aria-label="Zoom in"
                                    onclick={zoom_in}
                                >{ "+" }</button>
                            </div>
                        </div>
                    </div>

                    <div class="onto-uml-diagram__row">
                    if *tree_view {
                        <aside ref={tree_ref} class="onto-uml-diagram__tree" aria-label="Class tree">
                            { for tree.iter().map(|root| html! {
                                <OntoUmlTreeNode
                                    key={root.iri.clone()}
                                    node={root.clone()}
                                    selected={(*selected).clone()}
                                    expanded={(*tree_expanded).clone()}
                                    on_select={focus_class.clone()}
                                    on_toggle={toggle_tree_node.clone()}
                                />
                            }) }
                        </aside>
                    }
                    <div ref={scroll_ref} class="onto-uml-diagram__scroll" onwheel={onwheel}>
                        <div
                            class="onto-uml-diagram__stage"
                            style={format!("width: {}px; height: {}px", diagram.width * *zoom, diagram.height * *zoom)}
                        >
                            <div
                                class="onto-uml-diagram__zoomed"
                                style={format!(
                                    "width: {}px; height: {}px; transform: scale({}); transform-origin: top left",
                                    diagram.width, diagram.height, *zoom
                                )}
                            >
                                <svg
                                    viewBox={format!("0 0 {} {}", diagram.width, diagram.height)}
                                    width={diagram.width.to_string()}
                                    height={diagram.height.to_string()}
                                    class="onto-uml-diagram__svg"
                                    role="img"
                                    aria-label="UML class diagram of the ontology"
                                >
                                    <defs>
                                        // generalization: hollow triangle pointing at the superclass
                                        <marker
                                            id="onto-uml-inherit" markerWidth="16" markerHeight="16"
                                            refX="14" refY="8" orient="auto" markerUnits="userSpaceOnUse"
                                        >
                                            <path d="M1,1 L15,8 L1,15 Z" class="onto-uml-diagram__marker-inherit" stroke-width="1.25" />
                                        </marker>
                                        <marker
                                            id="onto-uml-inherit-active" markerWidth="16" markerHeight="16"
                                            refX="14" refY="8" orient="auto" markerUnits="userSpaceOnUse"
                                        >
                                            <path d="M1,1 L15,8 L1,15 Z" class="onto-uml-diagram__marker-inherit-active" stroke-width="1.5" />
                                        </marker>
                                        // association: open arrow head at the range class
                                        <marker
                                            id="onto-uml-arrow" markerWidth="13" markerHeight="13"
                                            refX="10" refY="6" orient="auto" markerUnits="userSpaceOnUse"
                                        >
                                            <path d="M1,1 L11,6 L1,11" fill="none" class="onto-uml-diagram__marker-arrow" stroke-width="1.5" />
                                        </marker>
                                        <marker
                                            id="onto-uml-arrow-active" markerWidth="13" markerHeight="13"
                                            refX="10" refY="6" orient="auto" markerUnits="userSpaceOnUse"
                                        >
                                            <path d="M1,1 L11,6 L1,11" fill="none" class="onto-uml-diagram__marker-arrow-active" stroke-width="2" />
                                        </marker>
                                    </defs>

                                    // click empty space to clear the selection
                                    <rect
                                        x="0" y="0"
                                        width={diagram.width.to_string()} height={diagram.height.to_string()}
                                        fill="transparent" pointer-events="all"
                                        onclick={clear_selection}
                                    />

                                    // group_hierarchy: a circular container behind each expanded
                                    // root + its revealed descendants.
                                    { for diagram.clusters.iter().enumerate().map(|(i, c)| html! {
                                        <circle
                                            key={format!("c{i}")}
                                            cx={c.cx.to_string()} cy={c.cy.to_string()} r={c.r.to_string()}
                                            class="onto-uml-diagram__cluster"
                                            stroke-width="1"
                                        />
                                    }) }

                                    // edges under the boxes
                                    { for diagram.edges.iter().enumerate().map(|(i, edge)| {
                                        let on_enter = {
                                            let hover_edge = hover_edge.clone();
                                            Callback::from(move |_: ()| hover_edge.set(Some(i)))
                                        };
                                        let on_leave = {
                                            let hover_edge = hover_edge.clone();
                                            Callback::from(move |_: ()| {
                                                hover_edge.set(if *hover_edge == Some(i) { None } else { *hover_edge })
                                            })
                                        };
                                        html! {
                                            <OntoUmlEdge
                                                key={format!("e{i}")}
                                                edge={edge.clone()}
                                                state={edge_state(*hover_edge, selected_iri, edge, i)}
                                                on_enter={on_enter}
                                                on_leave={on_leave}
                                            />
                                        }
                                    }) }

                                    { for diagram.nodes.iter().map(|node| {
                                        let on_toggle = {
                                            let select_class = select_class.clone();
                                            let iri = node.iri.clone();
                                            Callback::from(move |_: ()| select_class.emit(iri.clone()))
                                        };
                                        let on_expand = {
                                            let toggle_expand = toggle_expand.clone();
                                            let iri = node.iri.clone();
                                            Callback::from(move |_: ()| toggle_expand.emit(iri.clone()))
                                        };
                                        html! {
                                            <OntoUmlClass
                                                key={node.iri.clone()}
                                                node={node.clone()}
                                                state={node_state(selected_iri, &neighbours, &node.iri)}
                                                on_toggle={on_toggle}
                                                on_expand={on_expand}
                                            />
                                        }
                                    }) }
                                </svg>
                            </div>

                            // Info card shown while a class is highlighted: anchored over the
                            // clicked box (see `tooltip_pos`), rendered outside the
                            // zoom-scaled wrapper above so it always reads at 100%
                            // regardless of the diagram's zoom level. The Vue source wraps
                            // this in a `<Transition>` for an enter/leave scale+fade; Yew has
                            // no built-in equivalent, so this port omits that animation
                            // flourish and just mounts/unmounts the card directly.
                            if let (Some(node), Some((left, top))) = (&selected_node, &tooltip_pos) {
                                <div
                                    class="onto-uml-diagram__tooltip"
                                    style={format!("left: {left}px; top: {top}px")}
                                >
                                    <OntoUmlCard node={node.clone()} on_focus={focus_class.clone()} />
                                </div>
                            }
                        </div>
                    </div>
                    </div>
                </div>
            }
        </section>
    }
}

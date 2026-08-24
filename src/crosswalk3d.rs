//! Position every [`XwalkNode`] in 3D space, in place. Pure math, no
//! rendering dependency — mutates `graph.nodes[*].x/y/z` — so it's
//! unit-testable and swappable independently of the rendering layer, same
//! "layout is plain data, rendering is a separate concern" split `uml.rs`
//! already keeps for the 2D diagram.
//!
//! Ports `containers/prez-ui/theme/app/utils/crosswalk3d.ts`.
//!
//! Topology: one horizontal plane per side, stacked on Y (so a default
//! oblique camera sees every plane without one occluding another), each
//! side's own nodes arranged as a 3-level polar sunburst — entities on an
//! inner ring, their attributes on a middle ring, attributes' code values on
//! an outer ring — sized by subtree weight so a class with many
//! attributes/values gets proportionally more angular room. No force
//! simulation: the data is a strict tree per side, so a deterministic
//! recursive sector split produces identical output every render, same
//! "no client-side layout library" stance as `uml.rs`'s own `layout()`.
//!
//! For exactly two sides (the common case), a rigid-rotation alignment pass
//! follows: the circular mean of every mapping edge's angle delta rotates
//! the second side so mapped pairs land near the same angle on both planes —
//! the entity/attribute barycentre re-ordering a fuller alignment would add
//! are deliberately deferred, same call the Vue source's own MVP phase made.
#![allow(dead_code)]

use std::collections::HashMap;

use crate::crosswalk::{XwalkGraph, XwalkNode, XwalkTier};

const R_ENTITY: f64 = 20.0;
const R_ATTRIBUTE: f64 = 55.0;
const R_CODE: f64 = 105.0;
/// Total Y separation between the outermost two planes; intermediate planes
/// (3+ sides) are evenly spaced across the same span.
const PLANE_SEPARATION: f64 = 90.0;
const ENTITY_GUTTER: f64 = 0.08;
const CHILD_GUTTER: f64 = 0.12;

fn radius_for(tier: XwalkTier) -> f64 {
    match tier {
        XwalkTier::Entity => R_ENTITY,
        XwalkTier::Attribute => R_ATTRIBUTE,
        XwalkTier::Code => R_CODE,
    }
}

/// Subtree weight (self + every descendant), so a bushy entity gets more of
/// the circle than a sparse one. Small N (tens of nodes per side) - no
/// memoisation needed.
fn subtree_weight(iri: &str, by_iri: &HashMap<&str, &XwalkNode>) -> u32 {
    let Some(n) = by_iri.get(iri) else { return 0 };
    1 + n.children.iter().map(|c| subtree_weight(c, by_iri)).sum::<u32>()
}

/// Recursive angular sector split: `parent_iri`'s `[start, start+span)`
/// sector is divided among its children proportional to their own subtree
/// weight, each centred in its own (gutter-shrunk) sub-sector, then recurses.
fn allocate_children(
    parent_iri: &str,
    start: f64,
    span: f64,
    by_iri: &HashMap<&str, &XwalkNode>,
    angle: &mut HashMap<String, f64>,
) {
    let Some(parent) = by_iri.get(parent_iri) else { return };
    if parent.children.is_empty() {
        return;
    }
    let mut kids: Vec<&&XwalkNode> = parent.children.iter().filter_map(|iri| by_iri.get(iri.as_str())).collect();
    kids.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));

    let total_weight: u32 = kids.iter().map(|k| subtree_weight(&k.iri, by_iri)).sum::<u32>().max(1);
    let mut cursor = start;
    for kid in kids {
        let kid_span = (subtree_weight(&kid.iri, by_iri) as f64 / total_weight as f64) * span;
        let usable = kid_span * (1.0 - CHILD_GUTTER);
        angle.insert(kid.iri.clone(), cursor + usable / 2.0);
        allocate_children(&kid.iri, cursor, usable, by_iri, angle);
        cursor += kid_span;
    }
}

/// iri -> angle (radians) for one side's nodes, via the 3-level sunburst
/// described in the module doc comment. Entities (the roots — no parent
/// resolved on this side) split the full circle; each recurses into its own
/// attributes/code values via `allocate_children`.
fn ring_allocate(nodes: &[&XwalkNode]) -> HashMap<String, f64> {
    let by_iri: HashMap<&str, &XwalkNode> = nodes.iter().map(|n| (n.iri.as_str(), *n)).collect();
    let mut roots: Vec<&&XwalkNode> = nodes.iter().filter(|n| n.parent.is_none()).collect();
    roots.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));

    let mut angle = HashMap::new();
    let total_weight: u32 = roots.iter().map(|r| subtree_weight(&r.iri, &by_iri)).sum::<u32>().max(1);
    let mut cursor = 0.0;
    for root in roots {
        let span = (subtree_weight(&root.iri, &by_iri) as f64 / total_weight as f64) * std::f64::consts::TAU;
        let usable = span * (1.0 - ENTITY_GUTTER);
        angle.insert(root.iri.clone(), cursor + usable / 2.0);
        allocate_children(&root.iri, cursor, usable, &by_iri, &mut angle);
        cursor += span;
    }
    angle
}

/// Circular-mean rigid rotation of `side_b`'s nodes so its mapped pairs with
/// `side_a` land near the same angle on both planes — closed-form, no
/// iteration. A no-op if there are no cross-side mapping edges between
/// exactly these two sides.
fn align_sides(graph: &mut XwalkGraph, side_a: &str, side_b: &str) {
    let positions: HashMap<String, (f64, f64, String)> =
        graph.nodes.iter().map(|n| (n.iri.clone(), (n.x, n.z, n.side.clone()))).collect();

    let mut deltas = Vec::new();
    for e in &graph.edges {
        if e.kind != crate::crosswalk::XwalkEdgeKind::Mapping {
            continue;
        }
        let (Some(s), Some(t)) = (positions.get(&e.source), positions.get(&e.target)) else { continue };
        if s.2 == t.2 {
            continue;
        }
        let pair = if s.2 == side_a && t.2 == side_b {
            Some((s, t))
        } else if t.2 == side_a && s.2 == side_b {
            Some((t, s))
        } else {
            None
        };
        let Some((a, b)) = pair else { continue };
        deltas.push(a.1.atan2(a.0) - b.1.atan2(b.0));
    }
    if deltas.is_empty() {
        return;
    }
    let sin_sum: f64 = deltas.iter().map(|d| d.sin()).sum();
    let cos_sum: f64 = deltas.iter().map(|d| d.cos()).sum();
    let mean_delta = sin_sum.atan2(cos_sum);
    for n in &mut graph.nodes {
        if n.side != side_b {
            continue;
        }
        let r = n.x.hypot(n.z);
        let a = n.z.atan2(n.x) + mean_delta;
        n.x = a.cos() * r;
        n.z = a.sin() * r;
    }
}

/// Lay out every node in `graph` in place (mutates x/y/z). Call again after
/// filtering `graph.nodes` (e.g. "hide code values") to re-derive a clean
/// layout for the reduced set — this never assumes a previous layout.
pub fn layout_crosswalk_3d(graph: &mut XwalkGraph) {
    let slugs: Vec<String> = graph.sides.iter().map(|s| s.slug.clone()).collect();
    for (i, slug) in slugs.iter().enumerate() {
        let side_nodes: Vec<&XwalkNode> = graph.nodes.iter().filter(|n| &n.side == slug).collect();
        let angle = ring_allocate(&side_nodes);
        let y = if slugs.len() > 1 {
            (i as f64 - (slugs.len() - 1) as f64 / 2.0) * (PLANE_SEPARATION / (slugs.len() - 1).max(1) as f64)
        } else {
            0.0
        };
        let updates: HashMap<String, (f64, f64, f64)> = side_nodes
            .iter()
            .map(|n| {
                let a = angle.get(&n.iri).copied().unwrap_or(0.0);
                let r = radius_for(n.tier);
                (n.iri.clone(), (a.cos() * r, y, a.sin() * r))
            })
            .collect();
        for n in &mut graph.nodes {
            if &n.side == slug {
                if let Some((x, y, z)) = updates.get(&n.iri) {
                    n.x = *x;
                    n.y = *y;
                    n.z = *z;
                }
            }
        }
    }
    if slugs.len() == 2 {
        align_sides(graph, &slugs[0], &slugs[1]);
    }
}

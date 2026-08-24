//! Derive a crosswalk graph — nodes from N already-published ontologies plus
//! the mapping edges between them — from their already-parsed
//! [`OntologyModel`]s (see `crate::ontology::parse_ontology`). Pure and
//! dependency-free, same contract as `ontology.rs`/`uml.rs`: no WebGL/canvas
//! code here, this only ever emits plain data, consumed by
//! `crate::crosswalk3d` (layout) and `components::organisms::crosswalk_scene`
//! (rendering).
//!
//! Ports `containers/prez-ui/theme/app/utils/crosswalk.ts`.
//!
//! Generalised over N sides, not hardcoded to any two ontologies — a
//! crosswalk's "sides" are whichever ontology slugs
//! `pipelines/publish-widoco-docs/generate.py` resolved for it in
//! `alignments.json` (see `find_alignment_sides` there); this module just
//! takes whatever list of `{slug, title, model}` it's given.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::ontology::{OntologyModel, Term, TermKind};

// Well-known predicate IRIs (kept local; same convention as uml.ts / uml.rs).
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
const DCT: &str = "http://purl.org/dc/terms/";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum XwalkTier {
    Entity,
    Attribute,
    Code,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum XwalkEdgeKind {
    Structure,
    Inheritance,
    Association,
    Mapping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MatchType {
    Exact,
    Close,
    Related,
}

impl MatchType {
    /// `{match}Match` predicate-style label, e.g. "exact" -> "exactMatch"
    /// text used by the Vue source's tooltip/table bodies.
    pub fn label(self) -> &'static str {
        match self {
            MatchType::Exact => "exact",
            MatchType::Close => "close",
            MatchType::Related => "related",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct XwalkNode {
    pub iri: String,
    pub label: String,
    pub curie: String,
    /// ontology slug this node belongs to (an alignment "side")
    pub side: String,
    pub tier: XwalkTier,
    pub color: String,
    pub description: Option<String>,
    /// skos:broader target, when it resolves to another node on this side
    pub parent: Option<String>,
    pub children: Vec<String>,
    /// how many mapping edges touch this node — drives hover/label emphasis
    pub match_count: u32,
    // Filled by `layout_crosswalk_3d`; (0,0,0) until then.
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XwalkEdge {
    pub kind: XwalkEdgeKind,
    pub source: String,
    pub target: String,
    /// only set when `kind == Mapping`
    pub matched: Option<MatchType>,
    /// reified skos:note rationale, when the alignment graph carries one
    pub rationale: Option<String>,
    /// association/mapping label (property name / match predicate)
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XwalkSide {
    pub slug: String,
    pub title: String,
    pub color: String,
    pub node_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XwalkMatchStats {
    pub exact: u32,
    pub close: u32,
    pub related: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XwalkStats {
    pub nodes: usize,
    pub mapped: usize,
    pub by_match: XwalkMatchStats,
}

#[derive(Clone, Debug, PartialEq)]
pub struct XwalkGraph {
    pub nodes: Vec<XwalkNode>,
    pub edges: Vec<XwalkEdge>,
    pub sides: Vec<XwalkSide>,
    pub stats: XwalkStats,
}

/// One pastel per side, cycling past two — matches `uml.rs`'s `PASTELS`
/// (`PASTELS[0]` blue, `PASTELS[6]` orange are the first two here) so a
/// crosswalk reads as part of the same visual system as the 2D class diagram.
const SIDE_COLORS: [&str; 6] = ["#bfdbfe", "#fed7aa", "#bbf7d0", "#ddd6fe", "#99f6e4", "#fecaca"];
const MAPPING_COLOR: &str = "#f5d0fe"; // fuchsia - PASTELS[8]

fn match_predicate(pred: &str) -> Option<MatchType> {
    if pred == format!("{SKOS}exactMatch") {
        Some(MatchType::Exact)
    } else if pred == format!("{SKOS}closeMatch") {
        Some(MatchType::Close)
    } else if pred == format!("{SKOS}relatedMatch") {
        Some(MatchType::Related)
    } else {
        None
    }
}

/// refs of a term's relation for a given predicate IRI (first ref only,
/// mirroring the Vue source's `findRelation(t, pred)?.refs[0]`).
fn rel_first<'a>(term: &'a Term, predicate: &str) -> Option<&'a crate::ontology::TermRef> {
    term.relations.iter().find(|r| r.predicate == predicate).and_then(|r| r.refs.first())
}

/// Entity / attribute / code-value tier for a term. Prefers the hub's own
/// `dcterms:type` convention (see `pipelines/load-netex`,
/// `pipelines/load-datex-ii`) when present, since that's asserted directly by
/// the source data; falls back to the term's OWL kind so a differently
/// curated ontology (no `dcterms:type` at all) still gets a reasonable tier
/// instead of everything collapsing into "code".
fn term_tier(t: &Term) -> XwalkTier {
    let value = t
        .annotations
        .iter()
        .find(|a| a.predicate == format!("{DCT}type"))
        .and_then(|a| a.values.first())
        .map(|v| v.value.as_str());
    match value {
        Some("Entity") => return XwalkTier::Entity,
        Some("Attribute") => return XwalkTier::Attribute,
        Some("Code value") => return XwalkTier::Code,
        _ => {}
    }
    match t.kind {
        TermKind::Class => XwalkTier::Entity,
        TermKind::ObjectProperty | TermKind::DatatypeProperty | TermKind::Property => XwalkTier::Attribute,
        _ => XwalkTier::Code,
    }
}

/// One side's nodes + its own structure/inheritance/association edges, from
/// its already-parsed `OntologyModel`. Structural/container terms (the
/// `skos:ConceptScheme` itself, an `owl:Ontology` header — anything with no
/// `dcterms:type` and no recognised OWL kind) are excluded: they're not a
/// class/attribute/code-value concept, just the scheme wrapper around them,
/// and would otherwise show up as a stray disconnected node.
fn is_diagrammable(t: &Term) -> bool {
    if t.annotations.iter().any(|a| a.predicate == format!("{DCT}type")) {
        return true;
    }
    matches!(
        t.kind,
        TermKind::Class | TermKind::ObjectProperty | TermKind::DatatypeProperty | TermKind::Property
    )
}

fn build_side(model: &OntologyModel, slug: &str, color: &str) -> (Vec<XwalkNode>, Vec<XwalkEdge>) {
    let terms: Vec<&Term> = model.sections.iter().flat_map(|s| s.terms.iter()).filter(|t| is_diagrammable(t)).collect();
    let by_iri: HashMap<&str, &Term> = terms.iter().map(|t| (t.iri.as_str(), *t)).collect();

    let mut nodes: Vec<XwalkNode> = terms
        .iter()
        .map(|t| XwalkNode {
            iri: t.iri.clone(),
            label: t.label.clone(),
            curie: t.curie.clone(),
            side: slug.to_string(),
            tier: term_tier(t),
            color: color.to_string(),
            description: t.descriptions.first().map(|d| d.value.clone()),
            parent: rel_first(t, &format!("{SKOS}broader")).map(|r| r.iri.clone()),
            children: Vec::new(),
            match_count: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        })
        .collect();

    let node_iris: HashSet<String> = nodes.iter().map(|n| n.iri.clone()).collect();
    // Two passes: first resolve which parents are actually present (drop a
    // parent that points outside this side's diagrammed set, treating the
    // node as a root instead), then populate each parent's `children`.
    for n in &mut nodes {
        if let Some(p) = &n.parent {
            if !node_iris.contains(p) {
                n.parent = None;
            }
        }
    }
    let parent_of: HashMap<String, String> =
        nodes.iter().filter_map(|n| n.parent.clone().map(|p| (n.iri.clone(), p))).collect();
    for (child_iri, parent_iri) in &parent_of {
        if let Some(parent_node) = nodes.iter_mut().find(|n| &n.iri == parent_iri) {
            parent_node.children.push(child_iri.clone());
        }
    }

    let mut seen_edge = HashSet::new();
    let mut edges = Vec::new();
    let mut add_edge = |e: XwalkEdge| {
        let key = (e.kind, e.source.clone(), e.target.clone());
        if seen_edge.insert(key) {
            edges.push(e);
        }
    };

    for t in &terms {
        if let Some(parent_iri) = parent_of.get(&t.iri) {
            add_edge(XwalkEdge {
                kind: XwalkEdgeKind::Structure,
                source: t.iri.clone(),
                target: parent_iri.clone(),
                matched: None,
                rationale: None,
                label: None,
            });
        }
        if let Some(parent_class) = rel_first(t, &format!("{RDFS}subClassOf")) {
            if by_iri.contains_key(parent_class.iri.as_str()) && parent_class.iri != t.iri {
                add_edge(XwalkEdge {
                    kind: XwalkEdgeKind::Inheritance,
                    source: t.iri.clone(),
                    target: parent_class.iri.clone(),
                    matched: None,
                    rationale: None,
                    label: None,
                });
            }
        }
        let domain = rel_first(t, &format!("{RDFS}domain"));
        let range = rel_first(t, &format!("{RDFS}range"));
        if let (Some(domain), Some(range)) = (domain, range) {
            if domain.iri != range.iri {
                if let (Some(_), Some(range_term)) = (by_iri.get(domain.iri.as_str()), by_iri.get(range.iri.as_str())) {
                    if term_tier(range_term) == XwalkTier::Entity {
                        add_edge(XwalkEdge {
                            kind: XwalkEdgeKind::Association,
                            source: domain.iri.clone(),
                            target: range.iri.clone(),
                            matched: None,
                            rationale: None,
                            label: Some(t.label.clone()),
                        });
                    }
                }
            }
        }
    }

    (nodes, edges)
}

fn pair_key(a: &str, b: &str) -> String {
    let mut pair = [a, b];
    pair.sort_unstable();
    format!("{}|{}", pair[0], pair[1])
}

/// Mapping edges (one per unordered pair, deduped — the source TTL asserts
/// both directions) from an alignment's already-parsed `OntologyModel`. An
/// alignment graph carries no `owl:Class`/property typing of its own, so
/// every term lands in `parse_ontology`'s single "Other" section; only the
/// match-predicate relations and the reified `align:map-*` rationale nodes
/// matter here (see
/// `pipelines/load-crosswalk-netex-datex/crosswalk.ttl` for the reification
/// shape this reads).
fn build_mappings(alignment: &OntologyModel) -> Vec<XwalkEdge> {
    let terms: Vec<&Term> = alignment.sections.iter().flat_map(|s| s.terms.iter()).collect();

    let mut rationale_by_pair: HashMap<String, String> = HashMap::new();
    for t in &terms {
        let subject = rel_first(t, &format!("{RDF}subject"));
        let object = rel_first(t, &format!("{RDF}object"));
        let (Some(subject), Some(object)) = (subject, object) else { continue };
        if let Some(note) = t.notes.iter().find(|n| n.predicate == format!("{SKOS}note")).and_then(|n| n.values.first())
        {
            rationale_by_pair.insert(pair_key(&subject.iri, &object.iri), note.value.clone());
        }
    }

    let mut seen_pair = HashSet::new();
    let mut edges = Vec::new();
    for t in &terms {
        for rel in &t.relations {
            let Some(matched) = match_predicate(&rel.predicate) else { continue };
            for r in &rel.refs {
                let key = pair_key(&t.iri, &r.iri);
                if !seen_pair.insert(key.clone()) {
                    continue;
                }
                edges.push(XwalkEdge {
                    kind: XwalkEdgeKind::Mapping,
                    source: t.iri.clone(),
                    target: r.iri.clone(),
                    matched: Some(matched),
                    rationale: rationale_by_pair.get(&key).cloned(),
                    label: Some(matched.label().to_string()),
                });
            }
        }
    }
    edges
}

/// Build the full crosswalk graph: every side's own nodes/structure edges,
/// plus the mapping edges between them (dropped if either endpoint isn't a
/// diagrammed node on one of `sides` — e.g. a mapping to a code value that
/// didn't make it into the ontology's own published model).
pub fn build_crosswalk_graph(sides: &[(String, String, OntologyModel)], alignment: &OntologyModel) -> XwalkGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut side_info = Vec::new();

    for (i, (slug, title, model)) in sides.iter().enumerate() {
        let color = SIDE_COLORS[i % SIDE_COLORS.len()];
        let (side_nodes, side_edges) = build_side(model, slug, color);
        side_info.push(XwalkSide { slug: slug.clone(), title: title.clone(), color: color.to_string(), node_count: side_nodes.len() });
        nodes.extend(side_nodes);
        edges.extend(side_edges);
    }

    let node_iris: HashSet<String> = nodes.iter().map(|n| n.iri.clone()).collect();
    let mappings: Vec<XwalkEdge> =
        build_mappings(alignment).into_iter().filter(|e| node_iris.contains(&e.source) && node_iris.contains(&e.target)).collect();

    let mut match_count: HashMap<String, u32> = HashMap::new();
    let mut by_match = XwalkMatchStats::default();
    for e in &mappings {
        *match_count.entry(e.source.clone()).or_insert(0) += 1;
        *match_count.entry(e.target.clone()).or_insert(0) += 1;
        match e.matched {
            Some(MatchType::Exact) => by_match.exact += 1,
            Some(MatchType::Close) => by_match.close += 1,
            Some(MatchType::Related) => by_match.related += 1,
            None => {}
        }
    }
    for n in &mut nodes {
        n.match_count = match_count.get(&n.iri).copied().unwrap_or(0);
    }

    let mapped = mappings.len();
    edges.extend(mappings);

    XwalkGraph { stats: XwalkStats { nodes: nodes.len(), mapped, by_match }, nodes, edges, sides: side_info }
}

pub fn edge_color(edge: &XwalkEdge) -> &'static str {
    if edge.kind == XwalkEdgeKind::Mapping {
        MAPPING_COLOR
    } else {
        "#94a3b8" // slate-400 - neutral, recedes behind node/mapping colour
    }
}

//! Derive a laid-out UML class diagram from a parsed [`OntologyModel`].
//!
//! Mirrors `containers/prez-ui/theme/app/utils/uml.ts`. Pure and
//! dependency-free (same constraint as `ontology.rs`): it does its own
//! light-weight layout so the diagram needs no client-side graph library —
//! with one deliberate, narrow exception, ported here too rather than
//! pulled in as a crate: `group_hierarchy` mode packs its cluster
//! containers (see [`UmlCluster`]) as circles via the same two primitives
//! behind d3-hierarchy's `packSiblings`/`packEnclose` (in turn the engine
//! behind D3's "zoomable circle packing" example) — classes inside a
//! cluster stay plain rectangles, laid out by this file's own `layout()`,
//! same as always; only the container shape and inter-cluster arrangement
//! come from that circle-packing code, privately reimplemented at the
//! bottom of this file (see `pack_siblings`/`pack_enclose`).
//!
//! Mapping ontology -> UML:
//!   - each owl:Class / rdfs:Class becomes a class box,
//!   - a datatype property (or a property whose range is not a diagrammed
//!     class) whose rdfs:domain is a class becomes an attribute of that class,
//!   - an object property whose domain and range are both diagrammed classes
//!     becomes a directed association labelled with the property name,
//!   - rdfs:subClassOf between two diagrammed classes becomes a generalization.
//!
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::ontology::{OntologyModel, Term, TermKind, TermRef};

// Well-known predicate IRIs (kept local; these are stable standards).
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

/// Registry of external vocabularies a local class/individual can reference
/// (by `rdfs:subClassOf` or `rdf:type`) without this ontology defining the
/// target itself — an IRI outside `model`'s own namespace that would
/// otherwise just be a dropped edge. Each entry recognises a namespace,
/// builds the deep link into that vocabulary's own reference documentation
/// for a term IRI in it, and gives the pastel-colour prefix a synthesized
/// node should use (same role `prefix_of` plays for local CURIEs). Seeded
/// with just the W3C ODRL vocabulary (every ODRL profile ontology in this
/// hub extends it); add another `(namespace, spec_base, prefix)` triple here
/// to cover a further vocabulary the same way.
const EXTERNAL_VOCABS: [(&str, &str, &str); 1] =
    [("http://www.w3.org/ns/odrl/2/", "https://www.w3.org/TR/odrl-vocab/#term-", "odrl")];

/// The external reference documentation URL and CURIE prefix for `iri`, when
/// it falls in a vocabulary `EXTERNAL_VOCABS` recognises — e.g.
/// `http://www.w3.org/ns/odrl/2/LeftOperand` ->
/// (`https://www.w3.org/TR/odrl-vocab/#term-LeftOperand`, "odrl") (the ODRL
/// vocab spec uses one `#term-<Name>` anchor per class and per property
/// alike). `None` for anything else, including every term local to `model`
/// itself.
fn external_reference(iri: &str) -> Option<(String, &'static str)> {
    EXTERNAL_VOCABS.iter().find_map(|(ns, spec_base, prefix)| iri.strip_prefix(ns).map(|local| (format!("{spec_base}{local}"), *prefix)))
}

/// Display label for a synthesized external reference node: the last
/// non-empty path/fragment segment of its IRI (e.g. "LeftOperand" from
/// ".../odrl/2/LeftOperand").
fn external_reference_label(iri: &str) -> String {
    iri.rsplit(['#', '/']).find(|s| !s.is_empty()).unwrap_or(iri).to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub struct UmlAttribute {
    /// property display label
    pub name: String,
    /// range label (curie), when known. TS field name is `type`, a reserved
    /// word in Rust; renamed here (same pattern as ontology.rs's
    /// `PropertyRef.reference`, renamed from TS `ref`).
    pub attr_type: Option<String>,
    /// text drawn in the box (full string, clipped to the box width)
    pub display: String,
}

/// An object property leaving a class (an association arrow), for the card.
#[derive(Clone, Debug, PartialEq)]
pub struct UmlAssociation {
    pub name: String,
    pub target: String,
}

/// A named reference to another class (used for `subclasses`/`superclasses`).
#[derive(Clone, Debug, PartialEq)]
pub struct UmlRef {
    pub name: String,
    pub iri: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UmlClassNode {
    pub iri: String,
    pub label: String,
    /// `label` clipped to fit the box width (filled by `size_nodes()`), for
    /// the name compartment — SVG text doesn't wrap or truncate on its own,
    /// so an unusually long class name would otherwise overflow the
    /// (width-capped) box and overlap whatever's beside it. `label` itself
    /// stays full, for the title/aria-label and anywhere else that isn't
    /// box-width-constrained.
    pub label_display: String,
    /// In-page anchor of the term card, when the class is rendered on the
    /// page. TS declares this optional (`anchor?: string`) because its own
    /// `Term.anchor` is optional; this port's `ontology::Term.anchor` is
    /// always populated for a diagrammed class (see ontology.rs), so this
    /// mirrors that as a plain `String` rather than `Option<String>`.
    pub anchor: String,
    /// CURIE prefix (e.g. "odrl"), or "" when the IRI has no known prefix
    pub prefix: String,
    /// Set only for a synthesized *external reference* node — a class this
    /// ontology relates to (by `rdfs:subClassOf` or `rdf:type`) but doesn't
    /// define itself, in a vocabulary `external_reference_url` recognises
    /// (currently just the W3C ODRL vocabulary). Drawn with a dashed border
    /// and an outbound-link glyph instead of the term-card "open" icon
    /// (there's no local anchor to open), pointing here.
    pub external_url: Option<String>,
    /// pastel fill assigned per prefix (one colour per prefix)
    pub color: String,
    /// the class's description(s), for the highlight tooltip card
    pub description: Option<String>,
    /// full attribute list (the box shows at most MAX_ATTRS; the card shows all)
    pub attributes: Vec<UmlAttribute>,
    /// attributes beyond what the box renders, surfaced as a "+N more" line
    pub hidden_count: usize,
    /// object properties leaving this class (the diagram's arrows), for the card
    pub associations: Vec<UmlAssociation>,
    /// Terms that are members of this class without being diagrammed as
    /// classes themselves — an individual whose `rdf:type` is this class
    /// (e.g. an `odrl:LeftOperand` instance, once `odrl:LeftOperand` is a
    /// diagrammed external reference node), or any non-class term related to
    /// this class by some other object-valued predicate (e.g. this
    /// ontology's own `emds:group`). Shown as a compact list inside the box
    /// (the box caps at MAX_MEMBERS; the card shows all), same idea as
    /// `attributes` but for instance-of/grouped-into rather than
    /// domain-typed relations.
    pub members: Vec<UmlRef>,
    /// members beyond what the box renders, surfaced as a "+N more" line
    pub member_hidden_count: usize,
    /// every direct subclass (rdfs:subClassOf pointing at this class), for
    /// the card's Subclasses tab — unlike `child_count` below, this is
    /// independent of `group_hierarchy`/collapse state: it's the full set,
    /// always. Its `iri` is what a click passes back up to reveal + focus
    /// that class in the diagram.
    pub subclasses: Vec<UmlRef>,
    /// every direct superclass (this class's own rdfs:subClassOf targets),
    /// for the card's Superclasses section — the mirror of `subclasses`
    /// above, and likewise independent of group_hierarchy/collapse state.
    pub superclasses: Vec<UmlRef>,
    /// direct subclasses in the diagram, by their primary (first)
    /// superclass — 0 unless `group_hierarchy` is on. Drives the
    /// expand/collapse badge.
    pub child_count: usize,
    /// whether this node's children are currently shown (only meaningful
    /// when `child_count > 0`) — false collapses them behind the "+N" badge.
    pub children_expanded: bool,
    /// IRI of the root class whose cluster this node currently belongs to —
    /// only set in `group_hierarchy` mode (see `UmlCluster::root`); lets a
    /// caller find the circle a given node sits inside.
    pub cluster_root: Option<String>,
    // Layout box (filled by layout()).
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UmlEdgeKind {
    Generalization,
    Association,
}

impl UmlEdgeKind {
    fn as_str(self) -> &'static str {
        match self {
            UmlEdgeKind::Generalization => "generalization",
            UmlEdgeKind::Association => "association",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UmlEdge {
    pub kind: UmlEdgeKind,
    pub source: String, // class IRI
    pub target: String, // class IRI
    pub label: Option<String>, // association: property name
    // Geometry (filled by route_edges()).
    pub d: String,
    pub label_x: f64,
    pub label_y: f64,
}

/// A root class and its currently-revealed descendants, circled and laid
/// out independently of every other cluster — only populated in
/// `group_hierarchy` mode (see `build_uml_diagram`). The classes inside stay
/// plain rectangles, positioned by this file's own `layout()`; only the
/// *container* is a circle (its minimal enclosing circle), and multiple
/// clusters are packed against each other as circles.
#[derive(Clone, Debug, PartialEq)]
pub struct UmlCluster {
    /// IRI of the root class this cluster is built around — lets a caller
    /// (e.g. re-centring the viewport after an expand) find the circle that
    /// belongs to a given node via that node's `cluster_root`.
    pub root: String,
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UmlDiagram {
    pub nodes: Vec<UmlClassNode>,
    pub edges: Vec<UmlEdge>,
    pub width: f64,
    pub height: f64,
    pub clusters: Vec<UmlCluster>,
}

// ---- geometry constants ---------------------------------------------------
const PAD_X: f64 = 12.0;
const ATTR_FS: f64 = 12.0; // attribute font size
const CHAR_W: f64 = ATTR_FS * 0.6; // ~px per attribute char
const HEADER_H: f64 = 28.0;
const ATTR_H: f64 = 18.0;
const MIN_W: f64 = 128.0;
const MAX_W: f64 = 260.0;
const MAX_ATTRS: usize = 12; // beyond this a "+N more" line keeps boxes proportioned
const MAX_MEMBERS: usize = 8; // same idea for `UmlClassNode::members` (external reference boxes can collect a lot of instances)
// A hub with more direct subclasses than this auto-collapses behind its "+N"
// badge even in flat (non-group_hierarchy) mode — see `build_uml_diagram`'s
// `collapse_threshold`. Above this, one straight generalization edge per
// child converging on a single box visually crowds regardless of canvas
// size; below it, a typical small hierarchy stays exactly as fully-flat as
// before this existed.
const MAX_FLAT_FANOUT: usize = 8;
const H_GAP: f64 = 44.0;
const V_GAP: f64 = 72.0;
const MARGIN: f64 = 28.0;
const ORDER_SWEEPS: usize = 8; // barycentre ordering passes that pull linked classes together
const CLUSTER_PAD: f64 = 16.0; // padding between a cluster's own content and its container circle
// Draw cluster containers larger than the (safe, non-overlapping) radius used
// to pack them, so containers may visually overlap in their own empty corner
// space — content rectangles never do, since node positions come from the
// safe radius only; this factor never affects them.
const CLUSTER_DISPLAY_INFLATION: f64 = 1.18;
const CLUSTER_LINK_GAP: f64 = 16.0; // minimum gap kept between two cluster circles by relax_cluster_links
const CLUSTER_LINK_ITERATIONS: usize = 200;

// Light pastels (Tailwind ~200 level). Boxes always use dark text, so these read
// well on both the light and the dark diagram canvas. One is assigned per prefix.
const PASTELS: [&str; 12] = [
    "#bfdbfe", // blue
    "#bbf7d0", // green
    "#fde68a", // amber
    "#fbcfe8", // pink
    "#ddd6fe", // violet
    "#99f6e4", // teal
    "#fed7aa", // orange
    "#c7d2fe", // indigo
    "#f5d0fe", // fuchsia
    "#d9f99d", // lime
    "#fecaca", // red
    "#a5f3fc", // cyan
];

/// CURIE prefix, i.e. the text before the first colon ("" when unprefixed).
fn prefix_of(curie: &str) -> String {
    match curie.find(':') {
        Some(i) if i > 0 => curie[..i].to_string(),
        _ => String::new(),
    }
}

/// Rough text width in px; good enough to size boxes without a DOM. TS
/// measures `s.length` (UTF-16 code units); we use `chars().count()`
/// instead — a pragmatic simplification (same choice ontology.rs makes for
/// `brief_text`), fine here since this only sizes/clips boxes rather than
/// measuring exact pixels.
fn text_width(s: &str, font_size: f64, bold: bool) -> f64 {
    s.chars().count() as f64 * font_size * if bold { 0.64 } else { 0.6 }
}

/// Clip a label to fit `width` px of box (minus padding), with an ellipsis.
/// `pub`: attribute rows precompute their clipped `display` in `size_nodes`
/// (there's a name+type string to build first), but a member row is just
/// `name`, so `OntoUmlClass` clips it directly at render time instead.
pub fn clip(s: &str, width: f64) -> String {
    let max = ((width - 2.0 * 10.0) / CHAR_W).floor().max(4.0) as usize;
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}\u{2026}", truncated.trim_end())
    }
}

fn clamp(n: f64, lo: f64, hi: f64) -> f64 {
    n.max(lo).min(hi)
}

/// refs of a term's relation for a given predicate IRI.
fn rel_refs<'a>(term: &'a Term, predicate: &str) -> &'a [TermRef] {
    term.relations
        .iter()
        .find(|r| r.predicate == predicate)
        .map(|r| r.refs.as_slice())
        .unwrap_or(&[])
}

fn is_property_kind(t: &Term) -> bool {
    matches!(t.kind, TermKind::ObjectProperty | TermKind::DatatypeProperty | TermKind::Property)
}

/// The size a class box should be forced to when "Expand in layout" merges
/// the info card into it (see `UmlLayoutOptions::expanded_card_size`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UmlBoxSize {
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, Default)]
pub struct UmlLayoutOptions {
    /// Collapse subclasses behind their (primary) superclass by default —
    /// only root classes (no superclass) are shown — with each collapsed
    /// superclass carrying a "+N" badge (see `child_count`/`children_expanded`
    /// on `UmlClassNode`) that reveals its direct children when the caller
    /// adds its IRI to `expanded`. This is what actually keeps "extends"
    /// links short and related entities grouped: a collapsed subtree isn't
    /// laid out at all, so it can't be pulled apart or blow out the canvas —
    /// the ordinary `layout()` algorithm, unchanged, just never sees more
    /// nodes than the caller chose to reveal.
    ///
    /// Also restricts the x-relaxation pass to generalization edges only
    /// (see `layout()`), for whatever nodes *are* visible — a small,
    /// independently-safe improvement (identical output to the default when
    /// an ontology has no rendered associations at all). See uml.ts's own
    /// module for the fuller rationale and the alternatives measured worse
    /// against a real, large ontology.
    pub group_hierarchy: bool,
    /// IRIs of collapsed superclasses to reveal the direct children of.
    /// Ignored unless `group_hierarchy` is set.
    pub expanded: Option<HashSet<String>>,
    /// IRI of a class whose box should be sized to `expanded_card_size`
    /// instead of its own normal computed size, so the layout makes room
    /// for it in place — the "Expand in layout" switch merges the info card
    /// into the class box itself and recalculates positions around the
    /// larger box, rather than floating the card on top unconnected to
    /// layout. Ignored unless `expanded_card_size` is set.
    pub expanded_card_iri: Option<String>,
    pub expanded_card_size: Option<UmlBoxSize>,
}

fn edge_key(kind: UmlEdgeKind, source: &str, target: &str, label: Option<&str>) -> String {
    format!("{}|{}|{}|{}", kind.as_str(), source, target, label.unwrap_or(""))
}

fn add_edge(
    edges: &mut Vec<UmlEdge>,
    edge_set: &mut HashSet<String>,
    kind: UmlEdgeKind,
    source: String,
    target: String,
    label: Option<String>,
) {
    let key = edge_key(kind, &source, &target, label.as_deref());
    if edge_set.contains(&key) {
        return;
    }
    edge_set.insert(key);
    edges.push(UmlEdge { kind, source, target, label, d: String::new(), label_x: 0.0, label_y: 0.0 });
}

/// Case-insensitive comparator standing in for the source's plain
/// `String.localeCompare` — a pragmatic simplification, same choice
/// ontology.rs makes (as `cmp_ci`) for its own `Intl.Collator` usages.
fn cmp_label(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

pub fn build_uml_diagram(model: &OntologyModel, options: &UmlLayoutOptions) -> UmlDiagram {
    // Diagrammed classes, in section/term order (mirrors the JS `Map`'s
    // insertion order used later for `[...classes.values()]`); `index_of`
    // stands in for repeated `classes.get(iri)` lookups.
    let mut nodes: Vec<UmlClassNode> = Vec::new();
    let mut index_of: HashMap<String, usize> = HashMap::new();
    for section in &model.sections {
        if section.kind != TermKind::Class {
            continue;
        }
        for t in &section.terms {
            let description = if t.descriptions.is_empty() {
                None
            } else {
                Some(t.descriptions.iter().map(|d| d.value.clone()).collect::<Vec<_>>().join("\n\n"))
            };
            index_of.insert(t.iri.clone(), nodes.len());
            nodes.push(UmlClassNode {
                iri: t.iri.clone(),
                label: t.label.clone(),
                label_display: t.label.clone(),
                anchor: t.anchor.clone(),
                prefix: prefix_of(&t.curie),
                color: String::new(),
                description,
                attributes: Vec::new(),
                hidden_count: 0,
                associations: Vec::new(),
                subclasses: Vec::new(),
                superclasses: Vec::new(),
                child_count: 0,
                children_expanded: false,
                cluster_root: None,
                external_url: None,
                members: Vec::new(),
                member_hidden_count: 0,
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            });
        }
    }

    // External reference nodes: classes this ontology relates to (by
    // rdfs:subClassOf, on a local class, or rdf:type, on a local
    // individual) without defining locally, in a vocabulary
    // `external_reference` recognises. Synthesized before any edges are
    // built, purely by adding ordinary `index_of` entries for them — the
    // subClassOf/domain/range logic right below needs no special-casing to
    // pick these up, it just stops finding every such target missing.
    let mut external_iris: Vec<String> = Vec::new();
    let mut seen_external: HashSet<String> = HashSet::new();
    for section in &model.sections {
        if section.kind == TermKind::Class {
            for t in &section.terms {
                for r in rel_refs(t, SUBCLASS_OF) {
                    if !index_of.contains_key(&r.iri) && external_reference(&r.iri).is_some() && seen_external.insert(r.iri.clone()) {
                        external_iris.push(r.iri.clone());
                    }
                }
            }
        } else {
            for t in &section.terms {
                for type_iri in &t.types {
                    if !index_of.contains_key(type_iri) && external_reference(type_iri).is_some() && seen_external.insert(type_iri.clone()) {
                        external_iris.push(type_iri.clone());
                    }
                }
            }
        }
    }
    for iri in external_iris {
        let (url, prefix) = external_reference(&iri).expect("filtered to recognised external IRIs above");
        let label = external_reference_label(&iri);
        index_of.insert(iri.clone(), nodes.len());
        nodes.push(UmlClassNode {
            iri,
            label: label.clone(),
            label_display: label,
            anchor: String::new(),
            prefix: prefix.to_string(),
            color: String::new(),
            description: Some(format!("Defined by the W3C ODRL vocabulary, not by this ontology — see {url}")),
            attributes: Vec::new(),
            hidden_count: 0,
            associations: Vec::new(),
            subclasses: Vec::new(),
            superclasses: Vec::new(),
            child_count: 0,
            children_expanded: false,
            cluster_root: None,
            external_url: Some(url),
            members: Vec::new(),
            member_hidden_count: 0,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
    }

    let mut edge_set: HashSet<String> = HashSet::new();
    let mut edges: Vec<UmlEdge> = Vec::new();

    // Generalizations from each class's own subClassOf relations.
    for section in &model.sections {
        if section.kind != TermKind::Class {
            continue;
        }
        for t in &section.terms {
            for r in rel_refs(t, SUBCLASS_OF) {
                if r.iri == t.iri {
                    continue;
                }
                if index_of.contains_key(&r.iri) {
                    add_edge(&mut edges, &mut edge_set, UmlEdgeKind::Generalization, t.iri.clone(), r.iri.clone(), None);
                    let sup_idx = index_of[&r.iri];
                    let sup_label = nodes[sup_idx].label.clone();
                    nodes[sup_idx].subclasses.push(UmlRef { name: t.label.clone(), iri: t.iri.clone() });
                    let sub_idx = index_of[&t.iri];
                    nodes[sub_idx].superclasses.push(UmlRef { name: sup_label, iri: r.iri.clone() });
                }
            }
        }
    }

    // Properties -> attributes / associations.
    for section in &model.sections {
        for t in &section.terms {
            if !is_property_kind(t) {
                continue;
            }
            let domains: Vec<&TermRef> = rel_refs(t, DOMAIN).iter().filter(|r| index_of.contains_key(&r.iri)).collect();
            if domains.is_empty() {
                continue;
            }
            let ranges = rel_refs(t, RANGE);
            let class_ranges: Vec<&TermRef> = ranges.iter().filter(|r| index_of.contains_key(&r.iri)).collect();
            let type_label = ranges.first().map(|r| r.label.clone());

            for d in &domains {
                let d_idx = index_of[&d.iri];
                if t.kind != TermKind::DatatypeProperty && !class_ranges.is_empty() {
                    // Directed association to each diagrammed range class.
                    for r in &class_ranges {
                        add_edge(
                            &mut edges,
                            &mut edge_set,
                            UmlEdgeKind::Association,
                            d.iri.clone(),
                            r.iri.clone(),
                            Some(t.label.clone()),
                        );
                        nodes[d_idx].associations.push(UmlAssociation { name: t.label.clone(), target: r.label.clone() });
                    }
                } else {
                    nodes[d_idx].attributes.push(UmlAttribute {
                        name: t.label.clone(),
                        attr_type: type_label.clone(),
                        display: String::new(),
                    });
                }
            }
        }
    }

    // Membership: a non-class term related to a diagrammed class without
    // being diagrammed as a class itself — typed `rdf:type` that class (an
    // ODRL `LeftOperand`/`Action` instance, once that class is a
    // synthesized external reference node above) or connected to it by some
    // other object-valued predicate this ontology defines (e.g.
    // deployEMDS's own `emds:group`, an ungrouped-domain property linking a
    // claim term to the class it belongs to). Listed inside the target
    // class's box — the same idea `attributes` is for domain-typed
    // properties, but for "is one of these" rather than "has one of these".
    // Scoped to NamedIndividual/Other terms: Class terms get their own box,
    // and a Property's own domain/range refs would otherwise list it as a
    // "member" of its own domain/range classes, which isn't what this means.
    for section in &model.sections {
        if section.kind != TermKind::NamedIndividual && section.kind != TermKind::Other {
            continue;
        }
        for t in &section.terms {
            let mut target_iris: HashSet<String> = HashSet::new();
            for rel in &t.relations {
                for r in &rel.refs {
                    if index_of.contains_key(&r.iri) {
                        target_iris.insert(r.iri.clone());
                    }
                }
            }
            for type_iri in &t.types {
                if index_of.contains_key(type_iri) {
                    target_iris.insert(type_iri.clone());
                }
            }
            for target_iri in target_iris {
                nodes[index_of[&target_iri]].members.push(UmlRef { name: t.label.clone(), iri: t.iri.clone() });
            }
        }
    }

    for n in nodes.iter_mut() {
        n.attributes.sort_by(|a, b| cmp_label(&a.name, &b.name));
        // Keep the full list (the tooltip card shows it); the box caps its own.
        n.hidden_count = n.attributes.len().saturating_sub(MAX_ATTRS);
        let mut seen: HashSet<String> = HashSet::new();
        n.associations.retain(|a| seen.insert(format!("{}|{}", a.name, a.target)));
        n.associations.sort_by(|a, b| cmp_label(&a.name, &b.name));
        n.subclasses.sort_by(|a, b| cmp_label(&a.name, &b.name));
        n.superclasses.sort_by(|a, b| cmp_label(&a.name, &b.name));
        n.members.sort_by(|a, b| cmp_label(&a.name, &b.name));
        n.member_hidden_count = n.members.len().saturating_sub(MAX_MEMBERS);
    }

    let group_hierarchy = options.group_hierarchy;
    // With `group_hierarchy` on, every class with any subclasses collapses
    // behind its primary superclass by default (the toggle's original,
    // unchanged meaning: "only roots start visible"). Off, a typical small
    // hierarchy still renders fully flat as before — but a hub whose own
    // fan-out is large enough to visually crowd the diagram (see
    // MAX_FLAT_FANOUT: a straight generalization edge per child, all
    // converging on one box, becomes an unreadable tangle well before 33 —
    // this file's own scale test) auto-collapses the same way, unprompted.
    // Either way `options.expanded` reveals a specific collapsed hub's
    // children regardless of which of the two reasons collapsed it —
    // `OntoUmlClass`'s "+N" badge is already unconditional on `child_count`
    // (see uml_class.rs), so populating it here is the only change expand/
    // collapse needs to also work for an auto-collapsed flat-mode hub; the
    // organism's click handler already just toggles `expanded` regardless of
    // `group_hierarchy`'s value.
    let collapse_threshold = if group_hierarchy { 0 } else { MAX_FLAT_FANOUT };

    let primary_parent = primary_parent_map(&edges);
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    for n in nodes.iter() {
        if let Some(parent_iri) = primary_parent.get(&n.iri) {
            if index_of.contains_key(parent_iri) {
                children_of.entry(parent_iri.clone()).or_default().push(n.iri.clone());
            }
        }
    }
    for n in nodes.iter_mut() {
        // Zero (not the raw count) for a hub at or below `collapse_threshold`
        // in flat mode: its children stay always-visible below (`reveal`
        // recurses into any non-empty `children_of` entry regardless of this
        // field — see there), and `OntoUmlClass`'s badge is unconditional on
        // `child_count > 0`, so leaving it at the raw count here would draw
        // a "+N"/collapse badge on an ordinary small hierarchy that was
        // never actually collapsed, in flat mode's typical case.
        let raw_child_count = children_of.get(&n.iri).map(Vec::len).unwrap_or(0);
        n.child_count = if raw_child_count > collapse_threshold { raw_child_count } else { 0 };
    }

    let empty_expanded: HashSet<String> = HashSet::new();
    let expanded = options.expanded.as_ref().unwrap_or(&empty_expanded);
    let mut clusters: HashMap<String, String> = HashMap::new();
    let mut root_iris: Vec<String> = Vec::new();

    for n in nodes.iter() {
        let has_visible_parent = primary_parent.get(&n.iri).is_some_and(|p| index_of.contains_key(p));
        if !has_visible_parent {
            root_iris.push(n.iri.clone()); // roots start every cluster and are always visible
        }
    }
    for root_iri in root_iris.clone() {
        reveal(&root_iri, &root_iri, &mut clusters, &children_of, expanded, &index_of, &mut nodes);
    }
    root_iris.sort_by(|a, b| cmp_label(&nodes[index_of[a]].label, &nodes[index_of[b]].label));

    nodes.retain(|n| clusters.contains_key(&n.iri));

    // A hidden (collapsed) endpoint doesn't just drop its edges — it
    // redirects them to its nearest VISIBLE ancestor (walking up
    // primary_parent, recursively through as many collapsed levels as
    // needed; a root is always visible, so this always terminates at worst
    // at the root carrying the "+N" badge), so a relationship one of a
    // collapsed group's children has isn't silently lost, just shown at
    // whatever level is currently expanded — including associations, which
    // used to be dropped outright once either end fell inside a different
    // cluster than the other. Every node keeps an absolute (x, y) after
    // packing (see below), in one shared coordinate space, so route_edges
    // (called globally, once all clusters are placed) can draw a straight
    // line between any two visible nodes regardless of which cluster each
    // belongs to.
    let nearest_visible = |iri: &str| -> Option<String> {
        let mut cur = iri.to_string();
        let mut seen: HashSet<String> = HashSet::new();
        loop {
            if clusters.contains_key(&cur) {
                return Some(cur);
            }
            if seen.contains(&cur) {
                return None; // cycle guard
            }
            seen.insert(cur.clone());
            match primary_parent.get(&cur) {
                Some(p) => cur = p.clone(),
                None => return None,
            }
        }
    };
    let mut redirected_keys: HashSet<String> = HashSet::new();
    let mut redirected: Vec<UmlEdge> = Vec::new();
    for e in &edges {
        let (Some(s), Some(t)) = (nearest_visible(&e.source), nearest_visible(&e.target)) else { continue };
        if s == t {
            continue;
        }
        let key = edge_key(e.kind, &s, &t, e.label.as_deref());
        if redirected_keys.contains(&key) {
            continue;
        }
        redirected_keys.insert(key);
        redirected.push(UmlEdge { kind: e.kind, source: s, target: t, label: e.label.clone(), d: String::new(), label_x: 0.0, label_y: 0.0 });
    }
    let mut visible_edges = redirected;

    // One pastel per distinct prefix, assigned in stable (sorted) order.
    let mut prefixes: Vec<String> = nodes.iter().map(|n| n.prefix.clone()).collect::<HashSet<_>>().into_iter().collect();
    prefixes.sort();
    let mut color_by_prefix: HashMap<String, &'static str> = HashMap::new();
    for (i, p) in prefixes.iter().enumerate() {
        color_by_prefix.insert(p.clone(), PASTELS[i % PASTELS.len()]);
    }
    for n in nodes.iter_mut() {
        n.color = color_by_prefix[&n.prefix].to_string();
    }
    size_nodes(&mut nodes);

    // "Expand in layout": override the selected class's own computed size
    // AFTER size_nodes so layout() below (either path) makes room for it at
    // its real, larger footprint instead of squeezing a card-sized overlay
    // over a normal-sized box. A no-op if the IRI isn't currently a visible
    // node at all.
    if let (Some(iri), Some(size)) = (&options.expanded_card_iri, &options.expanded_card_size) {
        if let Some(n) = nodes.iter_mut().find(|n| &n.iri == iri) {
            n.w = size.w;
            n.h = size.h;
        }
    }

    // Layout style is strictly the manual toggle's concern, independent of
    // *why* anything above was collapsed: `group_hierarchy` gets the
    // circle-packed per-cluster treatment below (unchanged); flat mode
    // (including a hub that just auto-collapsed on fan-out alone) gets ONE
    // ordinary layout() call over whatever `nodes`/`visible_edges` survived
    // the filtering above — auto-collapsing never introduces cluster
    // circles into a diagram the user left in flat mode.
    if !group_hierarchy {
        let (width, height) = layout(&mut nodes, &visible_edges, options);
        route_edges(&nodes, &mut visible_edges);
        return UmlDiagram { nodes, edges: visible_edges, width, height, clusters: Vec::new() };
    }
    let cluster_of = clusters;

    // Independent per-cluster layout: each root + its currently-revealed
    // descendants gets its own layout() call (so a cluster's internal
    // coordinates can never be influenced by, or blow out the bounds of,
    // any other cluster). Classes inside stay the same rectangles layout()
    // always produces; only the cluster CONTAINER is a circle (its minimal
    // enclosing circle — exact for a rectangle: centre = rect centre,
    // radius = half the diagonal), and multiple clusters are packed against
    // each other as circles (`pack_siblings`, see the module doc comment)
    // instead of wrapped into rows — a tighter, more organic arrangement
    // for differently-sized clusters than a row grid.
    //
    // Nodes are reordered (a stable sort) so each cluster's members are
    // contiguous in `nodes`, letting each cluster's `layout()` call take a
    // genuinely independent `&mut` slice — the same aliasing-free
    // arrangement the JS achieves by giving each cluster its own array of
    // (shared-reference) node objects.
    let cluster_rank: HashMap<String, usize> = root_iris.iter().enumerate().map(|(i, r)| (r.clone(), i)).collect();
    nodes.sort_by_key(|n| cluster_rank[cluster_of.get(&n.iri).unwrap()]);

    // Only intra-cluster edges feed a cluster's own layout() call — its
    // nodes are all it knows about. An edge that (after redirecting) still
    // crosses cluster boundaries can't influence either cluster's internal
    // layout; route_edges draws it afterwards, once every cluster has final
    // absolute coordinates in the shared canvas.
    let mut edges_by_cluster: HashMap<String, Vec<UmlEdge>> = HashMap::new();
    for e in &visible_edges {
        let s_root = cluster_of.get(&e.source).unwrap();
        let t_root = cluster_of.get(&e.target).unwrap();
        if s_root != t_root {
            continue;
        }
        edges_by_cluster.entry(s_root.clone()).or_default().push(e.clone());
    }

    struct LaidOutCluster {
        root: String,
        start: usize,
        end: usize,
        w: f64,
        h: f64,
        r: f64,
    }

    let mut laid_out: Vec<LaidOutCluster> = Vec::new();
    let mut idx = 0usize;
    for root in &root_iris {
        let start = idx;
        while idx < nodes.len() && cluster_of.get(&nodes[idx].iri).unwrap() == root {
            idx += 1;
        }
        let end = idx;
        if start == end {
            continue; // every root has at least itself; defensive only
        }
        let no_edges = Vec::new();
        let edges_for = edges_by_cluster.get(root).unwrap_or(&no_edges);
        let (w, h) = layout(&mut nodes[start..end], edges_for, options);
        // Minimal enclosing circle of the cluster's own [0,w]x[0,h]
        // bounding box (exact for a rectangle), padded so the container
        // doesn't hug the content too tightly.
        let r = w.hypot(h) / 2.0 + CLUSTER_PAD;
        laid_out.push(LaidOutCluster { root: root.clone(), start, end, w, h, r });
    }

    // Pack at the SAFE radius (guarantees rectangle content never collides —
    // pack_siblings never overlaps the circles it's given), but DRAW each
    // container inflated beyond that — letting the circles themselves
    // overlap in their empty corner space (a rectangle only fills ~64% of
    // its own minimal enclosing circle) for a tighter-looking, more organic
    // arrangement, without ever moving a node: the inflation is cosmetic
    // only, positions are unchanged from the safe packing.
    let radii: Vec<f64> = laid_out.iter().map(|c| c.r).collect();
    let mut circles = pack_siblings(&radii);

    // Cross-cluster links: pairs of clusters still connected by a visible
    // edge after redirecting collapsed endpoints — pulled towards each
    // other next (relax_cluster_links) so a hierarchy that actually
    // references another one lands near it, instead of wherever
    // pack_siblings' pure space-filling happened to put it. Every expand or
    // collapse rebuilds this from scratch (build_uml_diagram has no memory
    // of the previous layout), so the whole arrangement is always reviewed
    // fresh against the current set of visible clusters, not patched in place.
    let cluster_index: HashMap<String, usize> = laid_out.iter().enumerate().map(|(i, c)| (c.root.clone(), i)).collect();
    let mut seen_links: HashSet<String> = HashSet::new();
    let mut links: Vec<(usize, usize)> = Vec::new();
    for e in &visible_edges {
        let i = cluster_index[cluster_of.get(&e.source).unwrap()];
        let j = cluster_index[cluster_of.get(&e.target).unwrap()];
        if i == j {
            continue;
        }
        let (lo, hi) = if i < j { (i, j) } else { (j, i) };
        let key = format!("{lo}|{hi}");
        if seen_links.contains(&key) {
            continue;
        }
        seen_links.insert(key);
        links.push((lo, hi));
    }
    if !links.is_empty() {
        relax_cluster_links(&mut circles, &links);
    }

    let mut cluster_boxes: Vec<UmlCluster> = Vec::new();
    for (i, c) in laid_out.iter().enumerate() {
        let packed = circles[i];
        // The packed circle's centre must land on this cluster's own local
        // centre (w/2, h/2) once its nodes are shifted there.
        let offset_x = packed.x - c.w / 2.0;
        let offset_y = packed.y - c.h / 2.0;
        for n in &mut nodes[c.start..c.end] {
            n.x += offset_x;
            n.y += offset_y;
        }
        // A single unexpanded root has no "group" to show — its own box is
        // enough; only draw the cluster container once there's more than
        // one node in it.
        if c.end - c.start > 1 {
            cluster_boxes.push(UmlCluster { root: c.root.clone(), cx: packed.x, cy: packed.y, r: c.r * CLUSTER_DISPLAY_INFLATION });
        }
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for n in &nodes {
        min_x = min_x.min(n.x);
        min_y = min_y.min(n.y);
        max_x = max_x.max(n.x + n.w);
        max_y = max_y.max(n.y + n.h);
    }
    // Inflated circles can extend past the node-rectangle bounds above;
    // widen the canvas so they don't clip at the edge (cosmetic only —
    // never moves a node, so rectangle safety is untouched).
    for c in &cluster_boxes {
        min_x = min_x.min(c.cx - c.r);
        min_y = min_y.min(c.cy - c.r);
        max_x = max_x.max(c.cx + c.r);
        max_y = max_y.max(c.cy + c.r);
    }
    let shift_x = MARGIN - min_x;
    let shift_y = MARGIN - min_y;
    for n in &mut nodes {
        n.x += shift_x;
        n.y += shift_y;
    }
    for c in &mut cluster_boxes {
        c.cx += shift_x;
        c.cy += shift_y;
    }

    route_edges(&nodes, &mut visible_edges);
    UmlDiagram {
        nodes,
        edges: visible_edges,
        width: (max_x - min_x + 2.0 * MARGIN).ceil(),
        height: (max_y - min_y + 2.0 * MARGIN).ceil(),
        clusters: cluster_boxes,
    }
}

/// Recursively reveal `iri` into `clusters`/`cluster_root` (tagged with
/// `root_iri`), then its direct children too — always, when `iri` isn't a
/// collapse-worthy hub (its `child_count` field is already 0 in that case,
/// whatever its real child count; see `build_uml_diagram`'s
/// `collapse_threshold`, which decides that before calling this), or only
/// when `iri` is also in `expanded` otherwise. A free function (not a
/// closure) so it can recurse while holding a `&mut [UmlClassNode]` —
/// mirrors `buildUmlDiagram`'s inner `reveal` closure in uml.ts (which
/// predates the flat-mode auto-collapse case and only ever had the
/// collapse-worthy branch below).
fn reveal(
    iri: &str,
    root_iri: &str,
    clusters: &mut HashMap<String, String>,
    children_of: &HashMap<String, Vec<String>>,
    expanded: &HashSet<String>,
    index_of: &HashMap<String, usize>,
    nodes: &mut [UmlClassNode],
) {
    if clusters.contains_key(iri) {
        return;
    }
    clusters.insert(iri.to_string(), root_iri.to_string());
    let idx = index_of[iri];
    nodes[idx].cluster_root = Some(root_iri.to_string());
    let Some(kids) = children_of.get(iri).cloned() else { return };
    let collapse_worthy = nodes[idx].child_count > 0;
    if collapse_worthy {
        nodes[idx].children_expanded = expanded.contains(iri);
    }
    if !collapse_worthy || expanded.contains(iri) {
        for k in &kids {
            reveal(k, root_iri, clusters, children_of, expanded, index_of, nodes);
        }
    }
}

pub fn attr_text(a: &UmlAttribute) -> String {
    match &a.attr_type {
        Some(t) => format!("{}: {}", a.name, t),
        None => a.name.clone(),
    }
}

/// Attributes the box renders (the rest are summarised as a "+N more" line).
pub fn box_attributes(n: &UmlClassNode) -> &[UmlAttribute] {
    if n.hidden_count > 0 {
        &n.attributes[..MAX_ATTRS.min(n.attributes.len())]
    } else {
        &n.attributes
    }
}

/// Members the box renders (the rest are summarised as a "+N more" line).
pub fn box_members(n: &UmlClassNode) -> &[UmlRef] {
    if n.member_hidden_count > 0 {
        &n.members[..MAX_MEMBERS.min(n.members.len())]
    } else {
        &n.members
    }
}

/// Each class's primary (first-declared) superclass, from its
/// generalization edges — the "one parent" a class collapses behind in
/// `group_hierarchy` mode and walks up in `ancestor_chain`. Shared by
/// anything that needs that same one-parent view of the hierarchy, rather
/// than each recomputing it from `edges` independently.
fn primary_parent_map(edges: &[UmlEdge]) -> HashMap<String, String> {
    let mut parent = HashMap::new();
    for e in edges {
        if e.kind == UmlEdgeKind::Generalization {
            parent.entry(e.source.clone()).or_insert_with(|| e.target.clone());
        }
    }
    parent
}

/// Every class IRI that has at least one direct subclass — i.e. every class
/// that would carry an expand/collapse badge in `group_hierarchy` mode.
/// Backs an "expand all" / "collapse all" control: pass the full set as
/// `expanded` to reveal everything, or an empty set to collapse back to
/// just the roots. Building a full (ungrouped) diagram to derive this is a
/// little redundant work, but it's a one-off action, not a hot path.
pub fn expandable_iris(model: &OntologyModel) -> HashSet<String> {
    let diagram = build_uml_diagram(model, &UmlLayoutOptions::default());
    primary_parent_map(&diagram.edges).into_values().collect()
}

/// The chain of ancestor IRIs (immediate superclass up to, but excluding,
/// the ultimate root) that must be added to `expanded` for `iri` to be
/// visible in `group_hierarchy` mode — e.g. jumping to a subclass linked
/// from elsewhere in the UI. Empty when `iri` is itself a root (already
/// always visible) or isn't a diagrammed class.
pub fn ancestor_chain(model: &OntologyModel, iri: &str) -> Vec<String> {
    let diagram = build_uml_diagram(model, &UmlLayoutOptions::default());
    let primary_parent = primary_parent_map(&diagram.edges);
    let mut chain: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut cur = primary_parent.get(iri).cloned();
    while let Some(c) = cur {
        if seen.contains(&c) {
            break;
        }
        seen.insert(c.clone());
        chain.push(c.clone());
        cur = primary_parent.get(&c).cloned();
    }
    chain
}

/// One row of the "Class tree" sidebar forest — see [`class_tree`].
#[derive(Clone, Debug, PartialEq)]
pub struct UmlTreeNode {
    pub iri: String,
    pub label: String,
    pub children: Vec<UmlTreeNode>,
}

fn build_tree_node(
    iri: &str,
    by_iri: &HashMap<String, String>,
    children_of: &HashMap<String, Vec<String>>,
) -> UmlTreeNode {
    let mut children: Vec<UmlTreeNode> = children_of
        .get(iri)
        .map(|kids| kids.iter().map(|c| build_tree_node(c, by_iri, children_of)).collect())
        .unwrap_or_default();
    children.sort_by(|a, b| cmp_label(&a.label, &b.label));
    UmlTreeNode { iri: iri.to_string(), label: by_iri[iri].clone(), children }
}

/// The full class hierarchy as a forest (multiple roots, same as the
/// diagram's own clusters) nested by primary superclass — unlike the
/// diagram, this always includes every diagrammed class regardless of
/// `group_hierarchy`/`expanded` state; it's a plain outline, not itself
/// collapsible by data, only by the tree view's own UI state.
pub fn class_tree(model: &OntologyModel) -> Vec<UmlTreeNode> {
    let diagram = build_uml_diagram(model, &UmlLayoutOptions::default());
    let by_iri: HashMap<String, String> = diagram.nodes.iter().map(|n| (n.iri.clone(), n.label.clone())).collect();
    let primary_parent = primary_parent_map(&diagram.edges);
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    for n in &diagram.nodes {
        if let Some(p) = primary_parent.get(&n.iri) {
            if by_iri.contains_key(p) {
                children_of.entry(p.clone()).or_default().push(n.iri.clone());
            }
        }
    }
    let mut roots: Vec<UmlTreeNode> = diagram
        .nodes
        .iter()
        .filter(|n| !primary_parent.get(&n.iri).is_some_and(|p| by_iri.contains_key(p)))
        .map(|n| build_tree_node(&n.iri, &by_iri, &children_of))
        .collect();
    roots.sort_by(|a, b| cmp_label(&a.label, &b.label));
    roots
}

fn size_nodes(nodes: &mut [UmlClassNode]) {
    for n in nodes.iter_mut() {
        let shown: Vec<UmlAttribute> = box_attributes(n).to_vec();
        let shown_members: Vec<UmlRef> = box_members(n).to_vec();
        let mut w = text_width(&n.label, 13.0, true);
        for a in &shown {
            w = w.max(text_width(&attr_text(a), ATTR_FS, false));
        }
        for m in &shown_members {
            w = w.max(text_width(&m.name, ATTR_FS, false));
        }
        n.w = clamp(w.ceil() + 2.0 * PAD_X, MIN_W, MAX_W);
        n.label_display = clip(&n.label, n.w);
        for a in n.attributes.iter_mut() {
            let text = attr_text(a);
            a.display = clip(&text, n.w);
        }
        let attr_rows = shown.len() + if n.hidden_count > 0 { 1 } else { 0 };
        let member_rows = shown_members.len() + if n.member_hidden_count > 0 { 1 } else { 0 };
        let rows = attr_rows + member_rows;
        n.h = HEADER_H + if rows > 0 { rows as f64 * ATTR_H + 8.0 } else { 8.0 };
    }
}

fn row_width(nodes: &[UmlClassNode], row: &[usize]) -> f64 {
    row.iter().map(|&i| nodes[i].w).sum::<f64>() + row.len().saturating_sub(1) as f64 * H_GAP
}

fn pack_row(nodes: &mut [UmlClassNode], row: &[usize], y: f64, axis: f64) {
    let mut x = axis - row_width(nodes, row) / 2.0;
    for &i in row {
        nodes[i].x = x;
        nodes[i].y = y;
        x += nodes[i].w + H_GAP;
    }
}

#[allow(clippy::too_many_arguments)]
fn layer_of(
    iri: &str,
    index_of: &HashMap<String, usize>,
    supers_of: &HashMap<String, Vec<String>>,
    layer_cache: &mut HashMap<String, i32>,
    in_progress: &mut HashSet<String>,
) -> i32 {
    if let Some(&cached) = layer_cache.get(iri) {
        return cached;
    }
    if in_progress.contains(iri) {
        return 0;
    }
    in_progress.insert(iri.to_string());
    let mut level = 0;
    if let Some(sups) = supers_of.get(iri) {
        for s in sups {
            if index_of.contains_key(s) {
                level = level.max(layer_of(s, index_of, supers_of, layer_cache, in_progress) + 1);
            }
        }
    }
    in_progress.remove(iri);
    layer_cache.insert(iri.to_string(), level);
    level
}

/// Layered layout with wrapping and x-relaxation.
///
///  - Generalization depth sets the vertical layer (superclasses above subclasses).
///  - Wide layers wrap into several rows, keeping the drawing near a 4:3 box
///    instead of one very wide strip.
///  - Each node's x is then relaxed towards the mean x of the classes it links
///    to, with per-row overlap resolution, which pulls connected boxes
///    together and shortens the edges. By default that considers every edge
///    (generalizations and associations alike); with `group_hierarchy` it
///    only considers generalization edges (see `UmlLayoutOptions`).
///
/// This restriction is deliberately the ONLY thing `group_hierarchy` changes
/// about this algorithm — several stronger alternatives (row reassignment by
/// superclass, group-aware row packing, a proper recursive subtree-width
/// tree layout, grouped initial sort order) were tried and measured, against
/// a real, large ontology, to make the diagram measurably worse (longer
/// links, far more crossings, and/or a much wider canvas); see uml.ts's own
/// comment on `layout()` for the numbers. Collapsing sidesteps that need
/// entirely instead of trying to out-guess it.
fn layout(nodes: &mut [UmlClassNode], edges: &[UmlEdge], options: &UmlLayoutOptions) -> (f64, f64) {
    if nodes.is_empty() {
        return (0.0, 0.0);
    }
    let group_hierarchy = options.group_hierarchy;

    let index_of: HashMap<String, usize> = nodes.iter().enumerate().map(|(i, n)| (n.iri.clone(), i)).collect();

    // Generalization layers (longest path from a root), cycle-guarded.
    let mut supers_of: HashMap<String, Vec<String>> = HashMap::new();
    for e in edges {
        if e.kind != UmlEdgeKind::Generalization {
            continue;
        }
        supers_of.entry(e.source.clone()).or_default().push(e.target.clone());
    }
    let mut layer_cache: HashMap<String, i32> = HashMap::new();
    let mut in_progress: HashSet<String> = HashSet::new();

    let per_row = 3usize.max((nodes.len() as f64).sqrt().round() as usize);

    // Undirected adjacency for ordering + relaxation: every edge by
    // default, generalization edges only when grouping by hierarchy — so a
    // class's x is pulled only towards its superclass/siblings, not also
    // towards whatever it happens to be associated with elsewhere in the
    // diagram.
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for e in edges {
        if e.source == e.target {
            continue;
        }
        if group_hierarchy && e.kind != UmlEdgeKind::Generalization {
            continue;
        }
        adj.entry(e.source.clone()).or_default().push(e.target.clone());
        adj.entry(e.target.clone()).or_default().push(e.source.clone());
    }

    // Group by layer, then wrap each layer into rows of at most `per_row` boxes.
    let mut layer_map: HashMap<i32, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        let l = layer_of(&n.iri, &index_of, &supers_of, &mut layer_cache, &mut in_progress);
        layer_map.entry(l).or_default().push(i);
    }
    let mut layer_keys: Vec<i32> = layer_map.keys().copied().collect();
    layer_keys.sort();

    let mut rows: Vec<Vec<usize>> = Vec::new();
    for l in layer_keys {
        let mut group = layer_map.remove(&l).unwrap();
        group.sort_by(|&a, &b| cmp_label(&nodes[a].label, &nodes[b].label));
        let num_rows = ((group.len() as f64 / per_row as f64).ceil() as usize).max(1);
        let size = (group.len() as f64 / num_rows as f64).ceil() as usize;
        let mut i = 0;
        while i < group.len() {
            let end = (i + size).min(group.len());
            rows.push(group[i..end].to_vec());
            i += size;
        }
    }

    // Vertical placement: one band per row.
    let mut row_y: Vec<f64> = Vec::new();
    let mut y = MARGIN;
    for row in &rows {
        row_y.push(y);
        let max_h = row.iter().map(|&i| nodes[i].h).fold(0.0_f64, f64::max);
        y += max_h + V_GAP;
    }

    // Row width depends only on membership, so the widest row fixes the
    // canvas width and a single vertical centre line that every row is
    // packed around.
    let max_row_width = rows.iter().map(|r| row_width(nodes, r)).fold(0.0_f64, f64::max);
    let axis = MARGIN + max_row_width / 2.0;
    for (r, row) in rows.iter().enumerate() {
        pack_row(nodes, row, row_y[r], axis);
    }

    // Barycentre ordering: repeatedly order each row by the mean x of the
    // classes it links to, then re-pack it centred. Bounded width (stays
    // compact) and stable, while pulling connected classes towards a shared
    // column — which is what shortens the links.
    for _ in 0..ORDER_SWEEPS {
        for (r, row) in rows.iter_mut().enumerate() {
            if row.len() > 1 {
                let mut key: HashMap<usize, f64> = HashMap::new();
                for &i in row.iter() {
                    let mut sum = 0.0;
                    let mut c = 0usize;
                    if let Some(neighbors) = adj.get(&nodes[i].iri) {
                        for m in neighbors {
                            if let Some(&mi) = index_of.get(m) {
                                sum += nodes[mi].x + nodes[mi].w / 2.0;
                                c += 1;
                            }
                        }
                    }
                    let k = if c > 0 { sum / c as f64 } else { nodes[i].x + nodes[i].w / 2.0 };
                    key.insert(i, k);
                }
                row.sort_by(|&a, &b| key[&a].partial_cmp(&key[&b]).unwrap());
            }
            pack_row(nodes, row, row_y[r], axis);
        }
    }

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for n in nodes.iter() {
        min_x = min_x.min(n.x);
        max_x = max_x.max(n.x + n.w);
    }
    for n in nodes.iter_mut() {
        n.x += MARGIN - min_x;
    }

    let last_row = rows.last().unwrap();
    let last_row_max_h = last_row.iter().map(|&i| nodes[i].h).fold(0.0_f64, f64::max);
    let width = (max_x - min_x + 2.0 * MARGIN).ceil();
    let height = (row_y[rows.len() - 1] + last_row_max_h + MARGIN).ceil();
    (width, height)
}

fn cx(n: &UmlClassNode) -> f64 {
    n.x + n.w / 2.0
}
fn cy(n: &UmlClassNode) -> f64 {
    n.y + n.h / 2.0
}

/// The source's `Math.abs(x) || eps` idiom: `x` unless it's exactly zero.
fn or_epsilon(x: f64, eps: f64) -> f64 {
    if x == 0.0 { eps } else { x }
}

/// Intersection of the segment (centre of n -> external point) with n's border.
fn border_point(n: &UmlClassNode, tx: f64, ty: f64) -> (f64, f64) {
    let dx = tx - cx(n);
    let dy = ty - cy(n);
    if dx == 0.0 && dy == 0.0 {
        return (cx(n), cy(n));
    }
    let scale = (n.w / 2.0 / or_epsilon(dx.abs(), 1e-9)).min(n.h / 2.0 / or_epsilon(dy.abs(), 1e-9));
    (cx(n) + dx * scale, cy(n) + dy * scale)
}

/// A packed circle: centre + radius. Shared by `pack_siblings`/`pack_enclose`
/// (see the bottom of this file) and `relax_cluster_links` below.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Circle {
    x: f64,
    y: f64,
    r: f64,
}

/// Nudge cluster circles that are linked by a cross-cluster edge closer
/// together, without ever letting any two clusters overlap — the same
/// "attract along real relationships, resolve overlaps every step" shape
/// `layout()`'s own per-row relaxation already uses for classes, one level
/// up (clusters instead of classes). `circles` (`pack_siblings`' own
/// output, one entry per cluster, in `laid_out`/link-index order) is
/// mutated in place — only x/y move, radii never change, so the no-overlap
/// guarantee `pack_siblings` gave them going in is never at risk of being
/// undone, only repositioned.
fn relax_cluster_links(circles: &mut [Circle], links: &[(usize, usize)]) {
    for iter in 0..CLUSTER_LINK_ITERATIONS {
        // Cooling step, like layout()'s own sweeps: big moves early,
        // settling to near-zero by the last iteration instead of
        // oscillating forever.
        let step = 0.15 * (1.0 - iter as f64 / CLUSTER_LINK_ITERATIONS as f64);
        // Attraction: a taut string, not a spring — pulls two linked
        // clusters together only while they're farther apart than just
        // touching, so it can't also drag already-close clusters into
        // overlapping.
        for &(a, b) in links {
            let dx = circles[b].x - circles[a].x;
            let dy = circles[b].y - circles[a].y;
            let dist = or_epsilon(dx.hypot(dy), 1.0);
            let min_dist = circles[a].r + circles[b].r + CLUSTER_LINK_GAP;
            if dist <= min_dist {
                continue;
            }
            let pull = (dist - min_dist) * step;
            let ux = dx / dist;
            let uy = dy / dist;
            circles[a].x += ux * pull;
            circles[a].y += uy * pull;
            circles[b].x -= ux * pull;
            circles[b].y -= uy * pull;
        }
        // Overlap resolution: attraction above can otherwise pull an
        // unrelated third cluster into the way — push any pair now closer
        // than their combined safe radius back apart.
        let n = circles.len();
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = circles[j].x - circles[i].x;
                let dy = circles[j].y - circles[i].y;
                let dist = or_epsilon(dx.hypot(dy), 0.01);
                let min_dist = circles[i].r + circles[j].r + CLUSTER_LINK_GAP;
                if dist >= min_dist {
                    continue;
                }
                let push = (min_dist - dist) / 2.0;
                let ux = dx / dist;
                let uy = dy / dist;
                circles[i].x -= ux * push;
                circles[i].y -= uy * push;
                circles[j].x += ux * push;
                circles[j].y += uy * push;
            }
        }
    }
}

fn route_edges(nodes: &[UmlClassNode], edges: &mut [UmlEdge]) {
    let by_iri: HashMap<&str, &UmlClassNode> = nodes.iter().map(|n| (n.iri.as_str(), n)).collect();
    for e in edges.iter_mut() {
        let (Some(&s), Some(&t)) = (by_iri.get(e.source.as_str()), by_iri.get(e.target.as_str())) else {
            e.d = String::new();
            continue;
        };
        if e.source == e.target {
            // Self-association: a small loop off the top-right corner.
            let x = s.x + s.w;
            let y = s.y + 12.0;
            e.d = format!(
                "M {x} {y} C {} {}, {} {}, {} {}",
                x + 34.0,
                y - 26.0,
                x + 34.0,
                s.y - 14.0,
                s.x + s.w * 0.72,
                s.y
            );
            e.label_x = x + 30.0;
            e.label_y = y - 20.0;
            continue;
        }
        let (ax, ay) = border_point(s, cx(t), cy(t));
        let (bx, by) = border_point(t, cx(s), cy(s));
        e.d = format!("M {ax:.1} {ay:.1} L {bx:.1} {by:.1}");
        e.label_x = (ax + bx) / 2.0;
        e.label_y = (ay + by) / 2.0;
    }
}

// ---- circle packing (private port of d3-hierarchy's pack/siblings.js and
// pack/enclose.js) -----------------------------------------------------------
//
// `group_hierarchy` mode is the one place this file leans on non-trivial
// third-party geometry rather than its own from-scratch layout (see the
// module doc comment): it packs each cluster's minimal enclosing circle
// against the others via the same two primitives d3-hierarchy's
// `packSiblings`/`packEnclose` use — the front-chain circle-placement
// algorithm from "Visualization of Large Hierarchical Data by Circle
// Packing" (Wang, Wang, Wang, Peng), and a Matousek/Sharir/Welzl-style
// incremental minimum-enclosing-circle construction. Ported by hand from:
//   https://raw.githubusercontent.com/d3/d3-hierarchy/main/src/pack/siblings.js
//   https://raw.githubusercontent.com/d3/d3-hierarchy/main/src/pack/enclose.js
//
// Deliberate simplification: d3 shuffles the input with a seeded linear
// congruential generator before `packEnclose`'s incremental construction —
// this only affects *expected* running time (the algorithm is correct for
// any input order; Welzl-style construction doesn't need randomness for
// correctness, only for its O(n) *expected*-time guarantee), and this repo
// has no `Math.random()`/`Date.now()` equivalent readily available anyway.
// This port omits the shuffle entirely and just processes circles in their
// given order — worst case is a slower (still-terminating, still-correct)
// pack for pathological inputs, never a wrong one.

/// Place circle `out` tangent to `u` and `v`. Positional, not named, to
/// mirror the source: `siblings.js`'s own `place(b, a, c)` is called with
/// swapped argument order at its two call sites (`place(b, a, c)` for the
/// third circle, `place(a._, b._, c)` inside the main loop) — what matters
/// is which *position* a circle is passed in, not what it's locally named
/// by the caller, so this keeps neutral parameter names instead.
fn place(u: &Circle, v: &Circle, out: &mut Circle) {
    let dx = u.x - v.x;
    let dy = u.y - v.y;
    let d2 = dx * dx + dy * dy;
    if d2 != 0.0 {
        let mut a2 = v.r + out.r;
        a2 *= a2;
        let mut b2 = u.r + out.r;
        b2 *= b2;
        if a2 > b2 {
            let x = (d2 + b2 - a2) / (2.0 * d2);
            let y = (0.0_f64.max(b2 / d2 - x * x)).sqrt();
            out.x = u.x - x * dx - y * dy;
            out.y = u.y - x * dy + y * dx;
        } else {
            let x = (d2 + a2 - b2) / (2.0 * d2);
            let y = (0.0_f64.max(a2 / d2 - x * x)).sqrt();
            out.x = v.x + x * dx - y * dy;
            out.y = v.y + x * dy + y * dx;
        }
    } else {
        out.x = v.x + out.r;
        out.y = v.y;
    }
}

fn intersects(a: &Circle, b: &Circle) -> bool {
    let dr = a.r + b.r - 1e-6;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dr > 0.0 && dr * dr > dx * dx + dy * dy
}

fn score(circles: &[Circle], next: &[usize], node: usize) -> f64 {
    let a = circles[node];
    let b = circles[next[node]];
    let ab = a.r + b.r;
    let dx = (a.x * b.r + b.x * a.r) / ab;
    let dy = (a.y * b.r + b.y * a.r) / ab;
    dx * dx + dy * dy
}

/// Port of d3-hierarchy's `packSiblings`: places every circle (by radius
/// only — matching the call site in `build_uml_diagram`, which only ever
/// needs radii in) tangent to its neighbours with no two overlapping, and
/// returns them all with `x`/`y` filled in (`r` unchanged from the input).
///
/// Front-chain "nodes" are identified by their circle's index into `radii`
/// throughout (each circle enters the chain exactly once, so its index
/// doubles as a stable node id) — `next`/`prev` hold each chain node's
/// neighbour, standing in for the source's `Node.next`/`Node.previous`
/// object pointers.
fn pack_siblings(radii: &[f64]) -> Vec<Circle> {
    let n = radii.len();
    let mut circles: Vec<Circle> = radii.iter().map(|&r| Circle { x: 0.0, y: 0.0, r }).collect();
    if n == 0 {
        return circles;
    }
    circles[0].x = 0.0;
    circles[0].y = 0.0;
    if n == 1 {
        return circles;
    }

    let r0 = circles[0].r;
    circles[0].x = -circles[1].r;
    circles[1].x = r0;
    circles[1].y = 0.0;
    if n == 2 {
        return circles;
    }

    // Place the third circle tangent to the first two.
    {
        let u = circles[1];
        let v = circles[0];
        let mut out = circles[2];
        place(&u, &v, &mut out);
        circles[2] = out;
    }

    // Initialize the front-chain using the first three circles.
    let mut next = vec![0usize; n];
    let mut prev = vec![0usize; n];
    next[0] = 1;
    next[1] = 2;
    next[2] = 0;
    prev[0] = 2;
    prev[1] = 0;
    prev[2] = 1;
    let mut a_idx = 0usize;
    let mut b_idx = 1usize;

    // Attempt to place each remaining circle…
    let mut i = 3usize;
    'pack: while i < n {
        {
            let u = circles[a_idx];
            let v = circles[b_idx];
            let mut out = circles[i];
            place(&u, &v, &mut out);
            circles[i] = out;
        }
        let c_idx = i;

        // Find the closest intersecting circle on the front-chain, if any.
        // "Closeness" is determined by linear distance along the
        // front-chain. "Ahead" or "behind" is likewise determined by
        // linear distance.
        let mut j = next[b_idx];
        let mut k = prev[a_idx];
        let mut sj = circles[b_idx].r;
        let mut sk = circles[a_idx].r;
        loop {
            if sj <= sk {
                if intersects(&circles[j], &circles[c_idx]) {
                    b_idx = j;
                    next[a_idx] = b_idx;
                    prev[b_idx] = a_idx;
                    continue 'pack; // retry this circle with the widened (a, b) window
                }
                sj += circles[j].r;
                j = next[j];
            } else {
                if intersects(&circles[k], &circles[c_idx]) {
                    a_idx = k;
                    next[a_idx] = b_idx;
                    prev[b_idx] = a_idx;
                    continue 'pack;
                }
                sk += circles[k].r;
                k = prev[k];
            }
            if j == next[k] {
                break;
            }
        }

        // Success! Insert the new circle c between a and b.
        let old_b = b_idx;
        prev[c_idx] = a_idx;
        next[c_idx] = old_b;
        next[a_idx] = c_idx;
        prev[old_b] = c_idx;
        b_idx = c_idx;

        // Compute the new closest circle pair to the centroid.
        let mut best_idx = a_idx;
        let mut best_score = score(&circles, &next, a_idx);
        let mut cursor = next[c_idx];
        while cursor != b_idx {
            let s = score(&circles, &next, cursor);
            if s < best_score {
                best_idx = cursor;
                best_score = s;
            }
            cursor = next[cursor];
        }
        a_idx = best_idx;
        b_idx = next[a_idx];

        i += 1;
    }

    // Compute the enclosing circle of the front chain.
    let mut ring: Vec<Circle> = vec![circles[b_idx]];
    let mut c_idx = b_idx;
    loop {
        c_idx = next[c_idx];
        if c_idx == b_idx {
            break;
        }
        ring.push(circles[c_idx]);
    }
    let enclosing = pack_enclose(&ring);

    // Translate the circles to put the enclosing circle around the origin.
    for c in circles.iter_mut() {
        c.x -= enclosing.x;
        c.y -= enclosing.y;
    }

    circles
}

fn encloses_not(a: &Circle, b: &Circle) -> bool {
    let dr = a.r - b.r;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dr < 0.0 || dr * dr < dx * dx + dy * dy
}

fn encloses_weak(a: &Circle, b: &Circle) -> bool {
    let dr = a.r - b.r + a.r.max(b.r).max(1.0) * 1e-9;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dr > 0.0 && dr * dr > dx * dx + dy * dy
}

fn encloses_weak_all(a: &Circle, basis: &[Circle]) -> bool {
    basis.iter().all(|b| encloses_weak(a, b))
}

fn enclose_basis1(a: &Circle) -> Circle {
    Circle { x: a.x, y: a.y, r: a.r }
}

fn enclose_basis2(a: &Circle, b: &Circle) -> Circle {
    let (x1, y1, r1) = (a.x, a.y, a.r);
    let (x2, y2, r2) = (b.x, b.y, b.r);
    let x21 = x2 - x1;
    let y21 = y2 - y1;
    let r21 = r2 - r1;
    let l = (x21 * x21 + y21 * y21).sqrt();
    Circle {
        x: (x1 + x2 + x21 / l * r21) / 2.0,
        y: (y1 + y2 + y21 / l * r21) / 2.0,
        r: (l + r1 + r2) / 2.0,
    }
}

fn enclose_basis3(a: &Circle, b: &Circle, c: &Circle) -> Circle {
    let (x1, y1, r1) = (a.x, a.y, a.r);
    let (x2, y2, r2) = (b.x, b.y, b.r);
    let (x3, y3, r3) = (c.x, c.y, c.r);
    let a2 = x1 - x2;
    let a3 = x1 - x3;
    let b2 = y1 - y2;
    let b3 = y1 - y3;
    let c2 = r2 - r1;
    let c3 = r3 - r1;
    let d1 = x1 * x1 + y1 * y1 - r1 * r1;
    let d2 = d1 - x2 * x2 - y2 * y2 + r2 * r2;
    let d3 = d1 - x3 * x3 - y3 * y3 + r3 * r3;
    let ab = a3 * b2 - a2 * b3;
    let xa = (b2 * d3 - b3 * d2) / (ab * 2.0) - x1;
    let xb = (b3 * c2 - b2 * c3) / ab;
    let ya = (a3 * d2 - a2 * d3) / (ab * 2.0) - y1;
    let yb = (a2 * c3 - a3 * c2) / ab;
    let qa = xb * xb + yb * yb - 1.0;
    let qb = 2.0 * (r1 + xa * xb + ya * yb);
    let qc = xa * xa + ya * ya - r1 * r1;
    let r = -(if qa.abs() > 1e-6 { (qb + (qb * qb - 4.0 * qa * qc).sqrt()) / (2.0 * qa) } else { qc / qb });
    Circle { x: x1 + xa + xb * r, y: y1 + ya + yb * r, r }
}

fn enclose_basis(basis: &[Circle]) -> Circle {
    match basis.len() {
        1 => enclose_basis1(&basis[0]),
        2 => enclose_basis2(&basis[0], &basis[1]),
        3 => enclose_basis3(&basis[0], &basis[1], &basis[2]),
        _ => unreachable!("enclose_basis: basis must have 1-3 circles"),
    }
}

fn extend_basis(basis: &[Circle], p: &Circle) -> Vec<Circle> {
    if encloses_weak_all(p, basis) {
        return vec![*p];
    }

    // If we get here then basis must have at least one element.
    for i in 0..basis.len() {
        if encloses_not(p, &basis[i]) && encloses_weak_all(&enclose_basis2(&basis[i], p), basis) {
            return vec![basis[i], *p];
        }
    }

    // If we get here then basis must have at least two elements.
    for i in 0..basis.len().saturating_sub(1) {
        for j in (i + 1)..basis.len() {
            if encloses_not(&enclose_basis2(&basis[i], &basis[j]), p)
                && encloses_not(&enclose_basis2(&basis[i], p), &basis[j])
                && encloses_not(&enclose_basis2(&basis[j], p), &basis[i])
                && encloses_weak_all(&enclose_basis3(&basis[i], &basis[j], p), basis)
            {
                return vec![basis[i], basis[j], *p];
            }
        }
    }

    // If we get here then something is very wrong (mirrors the source's own
    // `throw new Error` — unreachable for a well-formed, finite-radius
    // circle set).
    unreachable!("extend_basis: no basis extension found")
}

/// Port of d3-hierarchy's `packEncloseRandom`, minus the random shuffle (see
/// this section's header comment) — the minimum enclosing circle of `circles`.
fn pack_enclose(circles: &[Circle]) -> Circle {
    let n = circles.len();
    let mut i = 0usize;
    let mut basis: Vec<Circle> = Vec::new();
    let mut e: Option<Circle> = None;
    while i < n {
        let p = circles[i];
        if e.is_some_and(|e| encloses_weak(&e, &p)) {
            i += 1;
        } else {
            basis = extend_basis(&basis, &p);
            e = Some(enclose_basis(&basis));
            i = 0;
        }
    }
    e.unwrap_or(Circle { x: 0.0, y: 0.0, r: 0.0 })
}

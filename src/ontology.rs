//! Parse **expanded** JSON-LD (a flat array of node objects, each keyed by
//! full-IRI predicates) into the view model the ontology-browser components
//! render.
//!
//! Pure and dependency-free (beyond `serde_json`'s `Value` type), so it
//! behaves identically regardless of where the `.jsonld` document was
//! fetched from — mirrors `containers/prez-ui/theme/app/utils/ontology.ts`.
//!
//! Relies on `serde_json`'s `preserve_order` feature (see Cargo.toml) so a
//! node's predicates iterate in document order, same as `Object.entries()`
//! over a JS object parsed from the same JSON text — this determines the
//! display order of a term's annotations/notes/links blocks.
//!
//! Not yet wired into the UI (later stage) — allow dead_code until then so
//! the build stays warning-clean.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{Map, Value};

// ---- entity kinds ----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TermKind {
    Ontology,
    Class,
    ObjectProperty,
    DatatypeProperty,
    AnnotationProperty,
    Property,
    NamedIndividual,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LiteralValue {
    pub value: String,
    pub language: Option<String>,
    pub datatype: Option<String>,
}

/// A reference to another resource (internal term or external IRI).
#[derive(Clone, Debug, PartialEq)]
pub struct TermRef {
    pub iri: String,
    pub label: String,
    pub curie: String,
    pub kind: Option<TermKind>,
    /// true when the IRI resolves to a term rendered on this page
    pub internal: bool,
    /// in-page anchor id, present when `internal`
    pub anchor: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedValues {
    pub predicate: String,
    pub label: String,
    pub values: Vec<LiteralValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedRefs {
    pub predicate: String,
    pub label: String,
    pub refs: Vec<TermRef>,
}

/// A property applying to a class, paired with a brief description for listing.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyRef {
    // TS field name is `ref`, which is a reserved word in Rust; renamed here.
    pub reference: TermRef,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub iri: String,
    pub anchor: String,
    pub curie: String,
    pub kind: TermKind,
    pub types: Vec<String>,
    pub label: String,
    pub deprecated: bool,
    /// searchable haystack (lowercased label + curie + descriptions)
    pub haystack: String,
    pub descriptions: Vec<LiteralValue>,
    pub notes: Vec<NamedValues>,
    pub annotations: Vec<NamedValues>,
    pub relations: Vec<NamedRefs>,
    /// properties that declare this term as their `rdfs:domain` (classes only)
    pub properties: Vec<PropertyRef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OntologyHeader {
    pub iri: String,
    pub curie: String,
    pub title: String,
    pub descriptions: Vec<LiteralValue>,
    pub metadata: Vec<NamedValues>,
    pub links: Vec<NamedRefs>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OntologySection {
    pub kind: TermKind,
    pub title: String,
    pub terms: Vec<Term>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OntologyModel {
    pub header: Option<OntologyHeader>,
    pub sections: Vec<OntologySection>,
    pub term_count: usize,
}

/// One entry of the static `/docs/ontologies.json` manifest (emitted by the
/// publish-widoco-docs pipeline) that drives the ontology selector.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OntologyEntry {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub jsonld: String,
    /// the ontology's own base IRI (the subject of its `?s a owl:Ontology` triple)
    pub namespace: Option<String>,
    /// short prefix label — from `vann:preferredNamespacePrefix` when declared, else the slug
    pub prefix: Option<String>,
    pub downloads: Option<OntologyDownloads>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct OntologyDownloads {
    pub ttl: Option<String>,
    pub jsonld: Option<String>,
    pub owl: Option<String>,
    pub nt: Option<String>,
}

// ---- namespaces -------------------------------------------------------------
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
const DCT: &str = "http://purl.org/dc/terms/";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const ODRL: &str = "http://www.w3.org/ns/odrl/2/";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// Exported so callers (e.g. an OntUML diagram's prefix legend) can look up
/// the canonical namespace IRI for a well-known external prefix to link out
/// to. Kept as an ordered `Vec` (not a `HashMap`) because `to_curie` returns
/// the *first* declaration-order match rather than the longest — matching
/// the source's plain `Object.entries()` iteration, which is insertion-order
/// in JS but would be unordered in a Rust `HashMap`.
pub static PREFIXES: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    vec![
        ("rdf", RDF),
        ("rdfs", RDFS),
        ("owl", OWL),
        ("skos", SKOS),
        ("dcterms", DCT),
        ("dc", DC),
        ("odrl", ODRL),
        ("xsd", XSD),
        ("foaf", "http://xmlns.com/foaf/0.1/"),
        ("vann", "http://purl.org/vocab/vann/"),
        ("prov", "http://www.w3.org/ns/prov#"),
        ("schema", "http://schema.org/"),
    ]
});

static PRED_LABELS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    let mut put = |k: String, v: &str| {
        m.insert(k, v.to_string());
    };
    put(format!("{RDFS}subClassOf"), "Subclass of");
    put(format!("{RDFS}subPropertyOf"), "Subproperty of");
    put(format!("{RDFS}domain"), "Domain");
    put(format!("{RDFS}range"), "Range");
    put(format!("{RDFS}seeAlso"), "See also");
    put(format!("{RDF}type"), "Type");
    put(format!("{OWL}equivalentClass"), "Equivalent class");
    put(format!("{OWL}equivalentProperty"), "Equivalent property");
    put(format!("{OWL}inverseOf"), "Inverse of");
    put(format!("{OWL}disjointWith"), "Disjoint with");
    put(format!("{OWL}unionOf"), "Union of");
    put(format!("{OWL}sameAs"), "Same as");
    put(format!("{OWL}versionInfo"), "Version");
    put(format!("{SKOS}broader"), "Broader");
    put(format!("{SKOS}narrower"), "Narrower");
    put(format!("{SKOS}related"), "Related");
    put(format!("{SKOS}member"), "Members");
    put(format!("{SKOS}exactMatch"), "Exact match");
    put(format!("{SKOS}closeMatch"), "Close match");
    put(format!("{SKOS}definition"), "Definition");
    put(format!("{SKOS}scopeNote"), "Scope note");
    put(format!("{SKOS}note"), "Note");
    put(format!("{SKOS}example"), "Example");
    put(format!("{SKOS}editorialNote"), "Editorial note");
    put(format!("{SKOS}historyNote"), "History note");
    put(format!("{SKOS}changeNote"), "Change note");
    put(format!("{SKOS}altLabel"), "Alternative label");
    put(format!("{ODRL}includedIn"), "Included in");
    put(format!("{DCT}source"), "Source");
    put(format!("{DCT}conformsTo"), "Conforms to");
    put(format!("{DCT}title"), "Title");
    put(format!("{DCT}description"), "Description");
    put(format!("{DCT}creator"), "Creator");
    put(format!("{DCT}contributor"), "Contributor");
    put(format!("{DCT}publisher"), "Publisher");
    put(format!("{DCT}license"), "License");
    put(format!("{DCT}rights"), "Rights");
    put(format!("{DCT}created"), "Created");
    put(format!("{DCT}modified"), "Modified");
    put(format!("{DCT}issued"), "Issued");
    put(format!("{RDFS}comment"), "Comment");
    put(format!("{RDFS}label"), "Label");
    put(format!("{SKOS}prefLabel"), "Preferred label");
    m
});

// Predicates used to pick a display label (in priority order).
static LABEL_PREDS: LazyLock<Vec<String>> =
    LazyLock::new(|| vec![format!("{RDFS}label"), format!("{SKOS}prefLabel")]);

// Predicates rendered as the term's main descriptive body.
static DESC_PREDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        format!("{SKOS}definition"),
        format!("{RDFS}comment"),
        format!("{DCT}description"),
    ]
});

// Predicates rendered as secondary "notes".
static NOTE_PREDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        format!("{SKOS}scopeNote"),
        format!("{SKOS}note"),
        format!("{SKOS}example"),
        format!("{SKOS}editorialNote"),
        format!("{SKOS}historyNote"),
        format!("{SKOS}changeNote"),
        format!("{SKOS}altLabel"),
    ]
});

// Resource-valued relations, in the order they should be displayed.
static RELATION_ORDER: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        format!("{RDFS}subClassOf"),
        format!("{RDFS}subPropertyOf"),
        format!("{RDFS}domain"),
        format!("{RDFS}range"),
        format!("{OWL}equivalentClass"),
        format!("{OWL}equivalentProperty"),
        format!("{OWL}inverseOf"),
        format!("{OWL}unionOf"),
        format!("{OWL}disjointWith"),
        format!("{RDF}type"),
        format!("{SKOS}broader"),
        format!("{SKOS}narrower"),
        format!("{SKOS}related"),
        format!("{SKOS}member"),
        format!("{OWL}sameAs"),
        format!("{SKOS}exactMatch"),
        format!("{SKOS}closeMatch"),
        format!("{ODRL}includedIn"),
        format!("{RDFS}seeAlso"),
    ]
});

// Single predicates reused across helpers below; hoisted so each is built once.
static RDFS_DOMAIN: LazyLock<String> = LazyLock::new(|| format!("{RDFS}domain"));
static RDFS_IS_DEFINED_BY: LazyLock<String> = LazyLock::new(|| format!("{RDFS}isDefinedBy"));
static OWL_DEPRECATED: LazyLock<String> = LazyLock::new(|| format!("{OWL}deprecated"));
static OWL_UNION_OF: LazyLock<String> = LazyLock::new(|| format!("{OWL}unionOf"));
static DCT_TITLE: LazyLock<String> = LazyLock::new(|| format!("{DCT}title"));

// Predicates never shown on a term (redundant / structural).
static SKIP_PREDS: LazyLock<HashSet<String>> =
    LazyLock::new(|| HashSet::from([RDFS_IS_DEFINED_BY.clone(), OWL_DEPRECATED.clone()]));

fn kind_title(kind: TermKind) -> &'static str {
    match kind {
        TermKind::Ontology => "Ontology",
        TermKind::Class => "Classes",
        TermKind::ObjectProperty => "Object Properties",
        TermKind::DatatypeProperty => "Datatype Properties",
        TermKind::AnnotationProperty => "Annotation Properties",
        TermKind::Property => "Properties",
        TermKind::NamedIndividual => "Individuals",
        TermKind::Other => "Other Terms",
    }
}

const SECTION_ORDER: [TermKind; 7] = [
    TermKind::Class,
    TermKind::ObjectProperty,
    TermKind::DatatypeProperty,
    TermKind::Property,
    TermKind::AnnotationProperty,
    TermKind::NamedIndividual,
    TermKind::Other,
];

// ---- small helpers ------------------------------------------------------------

fn split_iri(iri: &str) -> (String, String) {
    let hash = iri.rfind('#');
    let slash = iri.rfind('/');
    let i = hash.or(slash);
    match i {
        None => (String::new(), iri.to_string()),
        Some(i) => (iri[..=i].to_string(), iri[i + 1..].to_string()),
    }
}

/// Compact an IRI to a `prefix:local` CURIE. When `extra` is given (namespace
/// IRI -> prefix, typically the ontologies published in this browser) it is
/// checked first, longest namespace first so a longer, more specific
/// namespace wins over a shorter accidental prefix match; falls back to the
/// static PREFIXES table, then to the IRI's bare local name.
fn to_curie(iri: &str, extra: Option<&HashMap<String, String>>) -> String {
    if let Some(extra) = extra {
        let mut entries: Vec<(&String, &String)> = extra.iter().collect();
        entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (ns, p) in entries {
            if iri.starts_with(ns.as_str()) && iri.len() > ns.len() {
                return format!("{p}:{}", &iri[ns.len()..]);
            }
        }
    }
    for (p, ns) in PREFIXES.iter() {
        if iri.starts_with(ns) && iri.len() > ns.len() {
            return format!("{p}:{}", &iri[ns.len()..]);
        }
    }
    let (_, local) = split_iri(iri);
    if local.is_empty() { iri.to_string() } else { local }
}

fn pred_label(pred: &str, extra: Option<&HashMap<String, String>>) -> String {
    PRED_LABELS
        .get(pred)
        .cloned()
        .unwrap_or_else(|| to_curie(pred, extra))
}

/// Normalise a JSON-LD predicate value — always an array in expanded form,
/// but be defensive — to a list of value references.
fn arr(v: Option<&Value>) -> Vec<&Value> {
    match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().collect(),
        Some(other) => vec![other],
    }
}

fn parse_literal(v: &Value) -> Option<LiteralValue> {
    let obj = v.as_object()?;
    let raw = obj.get("@value")?;
    let value = match raw {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let language = obj.get("@language").and_then(Value::as_str).map(String::from);
    let datatype = obj.get("@type").and_then(Value::as_str).map(String::from);
    Some(LiteralValue { value, language, datatype })
}

fn is_ref(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.contains_key("@id"))
}

/// Prefer English literals when any are present; otherwise keep them all.
fn prefer_en(vals: Vec<LiteralValue>) -> Vec<LiteralValue> {
    let en: Vec<LiteralValue> = vals
        .iter()
        .filter(|v| v.language.as_deref().is_some_and(|l| l.to_lowercase().starts_with("en")))
        .cloned()
        .collect();
    let chosen = if en.is_empty() { vals } else { en };
    let mut seen = HashSet::new();
    chosen.into_iter().filter(|v| seen.insert(v.value.clone())).collect()
}

fn slugify(s: &str) -> String {
    let mut base = String::new();
    let mut pending_dash = false;
    for c in s.chars() {
        for lc in c.to_lowercase() {
            if lc.is_ascii_lowercase() || lc.is_ascii_digit() {
                base.push(lc);
                pending_dash = false;
            } else if !pending_dash {
                base.push('-');
                pending_dash = true;
            }
        }
    }
    let trimmed = base.trim_matches('-');
    if trimmed.is_empty() { String::new() } else { format!("t-{trimmed}") }
}

/// Resolve resource-valued predicate values to a flat list of IRIs,
/// transparently unwrapping RDF lists (`@list`) and blank-node
/// `owl:unionOf` structures one or more levels deep so
/// `domain`/`range`/`subClassOf` unions render as their members.
fn collect_iris<'a>(
    values: &[&'a Value],
    node_map: &HashMap<String, &'a Map<String, Value>>,
    depth: u32,
) -> Vec<String> {
    if depth > 6 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for v in values {
        let obj = match v.as_object() {
            Some(o) => o,
            None => continue,
        };
        if let Some(list) = obj.get("@list").and_then(Value::as_array) {
            let list_refs: Vec<&Value> = list.iter().collect();
            out.extend(collect_iris(&list_refs, node_map, depth + 1));
            continue;
        }
        let id = match obj.get("@id").and_then(Value::as_str) {
            Some(s) => s,
            None => continue,
        };
        if id.starts_with("_:") {
            if let Some(bn) = node_map.get(id) {
                let union = arr(bn.get(OWL_UNION_OF.as_str()));
                if !union.is_empty() {
                    out.extend(collect_iris(&union, node_map, depth + 1));
                } else if let Some(list) = bn.get("@list").and_then(Value::as_array) {
                    let list_refs: Vec<&Value> = list.iter().collect();
                    out.extend(collect_iris(&list_refs, node_map, depth + 1));
                }
            }
            continue; // drop otherwise-anonymous nodes
        }
        out.push(id.to_string());
    }
    out
}

fn get_types(node: &Map<String, Value>) -> Vec<String> {
    node.get("@type")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn primary_kind(types: &[String]) -> TermKind {
    let has = |ns: &str, suffix: &str| types.iter().any(|t| t == &format!("{ns}{suffix}"));
    if has(OWL, "Ontology") {
        return TermKind::Ontology;
    }
    if has(OWL, "Class") || has(RDFS, "Class") {
        return TermKind::Class;
    }
    if has(OWL, "ObjectProperty") {
        return TermKind::ObjectProperty;
    }
    if has(OWL, "DatatypeProperty") {
        return TermKind::DatatypeProperty;
    }
    if has(OWL, "AnnotationProperty") {
        return TermKind::AnnotationProperty;
    }
    if types.iter().any(|t| t.starts_with(OWL) && t.ends_with("Property")) {
        return TermKind::ObjectProperty;
    }
    if has(RDF, "Property") {
        return TermKind::Property;
    }
    if has(OWL, "NamedIndividual") {
        return TermKind::NamedIndividual;
    }
    TermKind::Other
}

fn best_label(node: &Map<String, Value>) -> Option<String> {
    for pred in LABEL_PREDS.iter() {
        let lits: Vec<LiteralValue> = arr(node.get(pred.as_str())).into_iter().filter_map(parse_literal).collect();
        if lits.is_empty() {
            continue;
        }
        let en = lits
            .iter()
            .find(|l| l.language.as_deref().is_some_and(|lang| lang.to_lowercase().starts_with("en")));
        return Some(en.cloned().unwrap_or_else(|| lits[0].clone()).value);
    }
    None
}

/// The term's primary human description (first of skos:definition /
/// rdfs:comment / dct:description, preferring English), if any.
fn best_description(node: &Map<String, Value>) -> Option<String> {
    let lits: Vec<LiteralValue> = DESC_PREDS
        .iter()
        .flat_map(|p| arr(node.get(p.as_str())).into_iter().filter_map(parse_literal))
        .collect();
    prefer_en(lits).into_iter().next().map(|l| l.value)
}

/// Flatten whitespace and clip to a short, single-line blurb for compact
/// lists. Operates on `char`s rather than UTF-16 code units (the source's
/// unit of `.length`/`.slice`) — an acceptable simplification since the
/// clip point only ever affects word-wrapping, not the ASCII delimiters
/// being matched.
fn brief_text(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let truncated: String = flat.chars().take(max.saturating_sub(1)).collect();
    let trimmed = match truncated.rfind(char::is_whitespace) {
        Some(idx) => &truncated[..idx],
        None => truncated.as_str(),
    };
    format!("{}…", trimmed.trim_end())
}

/// Case-insensitive comparator standing in for the source's
/// `Intl.Collator("en", { sensitivity: "base" })` locale-aware sort — a
/// pragmatic simplification, not full Unicode collation.
fn cmp_ci(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn rel_order(pred: &str) -> usize {
    RELATION_ORDER.iter().position(|p| p == pred).unwrap_or(RELATION_ORDER.len())
}

fn extract_nodes(doc: &Value) -> Vec<&Map<String, Value>> {
    let items: &[Value] = if let Some(a) = doc.as_array() {
        a
    } else if let Some(a) = doc.get("@graph").and_then(Value::as_array) {
        a
    } else {
        &[]
    };
    items.iter().filter_map(Value::as_object).collect()
}

// ---- main entry -----------------------------------------------------------

/// # Arguments
/// * `namespace_prefixes` — namespace IRI -> prefix, built from every
///   published ontology's `namespace`/`prefix` manifest fields; lets CURIEs
///   for terms from other ontologies in this browser resolve to their
///   declared prefix instead of falling back to the static PREFIXES table or
///   bare local names.
pub fn parse_ontology(doc: &Value, namespace_prefixes: Option<&HashMap<String, String>>) -> OntologyModel {
    let nodes = extract_nodes(doc);

    let mut node_map: HashMap<String, &Map<String, Value>> = HashMap::new();
    let mut kind_map: HashMap<String, TermKind> = HashMap::new();
    let mut label_map: HashMap<String, String> = HashMap::new();
    let mut anchor_map: HashMap<String, String> = HashMap::new();
    let mut used_anchors: HashSet<String> = HashSet::new();

    for n in &nodes {
        let id = match n.get("@id").and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => continue,
        };
        node_map.insert(id.clone(), *n);
        let kind = primary_kind(&get_types(n));
        kind_map.insert(id.clone(), kind);
        label_map.insert(
            id.clone(),
            best_label(n).unwrap_or_else(|| to_curie(&id, namespace_prefixes)),
        );
        if id.starts_with("_:") || kind == TermKind::Ontology {
            continue;
        }
        let base = {
            let s = slugify(&to_curie(&id, namespace_prefixes));
            if s.is_empty() { "term".to_string() } else { s }
        };
        let mut anchor = base.clone();
        let mut i = 2;
        while used_anchors.contains(&anchor) {
            anchor = format!("{base}-{i}");
            i += 1;
        }
        used_anchors.insert(anchor.clone());
        anchor_map.insert(id, anchor);
    }

    // Reverse index: class IRI -> IRIs of properties declaring it as their
    // `rdfs:domain`, so a class card can list the attributes that apply to it.
    let mut domain_of: HashMap<String, Vec<String>> = HashMap::new();
    for n in &nodes {
        let id = match n.get("@id").and_then(Value::as_str) {
            Some(s) => s,
            None => continue,
        };
        if id.starts_with("_:") {
            continue;
        }
        let domain_vals = arr(n.get(RDFS_DOMAIN.as_str()));
        for cls in collect_iris(&domain_vals, &node_map, 0) {
            domain_of.entry(cls).or_default().push(id.to_string());
        }
    }

    let to_ref = |iri: &str| -> TermRef {
        let kind = kind_map.get(iri).copied();
        let internal = !iri.starts_with("_:") && anchor_map.contains_key(iri);
        TermRef {
            iri: iri.to_string(),
            label: label_map
                .get(iri)
                .cloned()
                .unwrap_or_else(|| to_curie(iri, namespace_prefixes)),
            curie: to_curie(iri, namespace_prefixes),
            kind,
            internal,
            anchor: if internal { anchor_map.get(iri).cloned() } else { None },
        }
    };

    // Resolve the properties applying to a class (its `rdfs:domain`
    // back-links), each with a brief description, de-duplicated and ordered
    // like the term lists.
    let domain_props = |class_iri: &str| -> Vec<PropertyRef> {
        let mut seen = HashSet::new();
        let mut result: Vec<PropertyRef> = domain_of
            .get(class_iri)
            .into_iter()
            .flatten()
            .filter(|p| seen.insert(p.to_string()))
            .map(|p| {
                let desc = node_map.get(p.as_str()).and_then(|n| best_description(n));
                PropertyRef {
                    reference: to_ref(p),
                    description: desc.map(|d| brief_text(&d, 160)),
                }
            })
            .collect();
        result.sort_by(|a, b| {
            cmp_ci(&a.reference.label, &b.reference.label).then_with(|| cmp_ci(&a.reference.curie, &b.reference.curie))
        });
        result
    };

    let mut header: Option<OntologyHeader> = None;
    let mut by_kind: HashMap<TermKind, Vec<Term>> = HashMap::new();

    for n in &nodes {
        let id = match n.get("@id").and_then(Value::as_str) {
            Some(s) => s,
            None => continue,
        };
        if id.starts_with("_:") {
            continue;
        }
        let kind = kind_map[id];

        if kind == TermKind::Ontology {
            header = Some(build_header(n, id, &to_ref, namespace_prefixes));
            continue;
        }

        let properties = if kind == TermKind::Class { domain_props(id) } else { Vec::new() };
        let anchor = &anchor_map[id];
        let term = build_term(n, id, kind, anchor, &node_map, &to_ref, properties, namespace_prefixes);
        by_kind.entry(kind).or_default().push(term);
    }

    let mut sections: Vec<OntologySection> = Vec::new();
    let mut term_count = 0usize;
    for kind in SECTION_ORDER {
        let mut terms = match by_kind.remove(&kind) {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };
        terms.sort_by(|a, b| cmp_ci(&a.label, &b.label).then_with(|| cmp_ci(&a.curie, &b.curie)));
        term_count += terms.len();
        sections.push(OntologySection { kind, title: kind_title(kind).to_string(), terms });
    }

    OntologyModel { header, sections, term_count }
}

fn build_header<F: Fn(&str) -> TermRef>(
    node: &Map<String, Value>,
    iri: &str,
    to_ref: &F,
    namespace_prefixes: Option<&HashMap<String, String>>,
) -> OntologyHeader {
    let title_lit = arr(node.get(DCT_TITLE.as_str())).into_iter().find_map(parse_literal);
    let title = title_lit
        .map(|l| l.value)
        .or_else(|| best_label(node))
        .unwrap_or_else(|| to_curie(iri, namespace_prefixes));

    let desc_lits: Vec<LiteralValue> = DESC_PREDS
        .iter()
        .flat_map(|p| arr(node.get(p.as_str())).into_iter().filter_map(parse_literal))
        .collect();
    let descriptions = prefer_en(desc_lits);

    let mut metadata: Vec<NamedValues> = Vec::new();
    let mut links: Vec<NamedRefs> = Vec::new();
    let mut shown: HashSet<String> = LABEL_PREDS.iter().cloned().collect();
    shown.extend(DESC_PREDS.iter().cloned());
    shown.insert(RDFS_IS_DEFINED_BY.clone());

    for (pred, raw_val) in node.iter() {
        if pred.starts_with('@') || shown.contains(pred) {
            continue;
        }
        let vals = arr(Some(raw_val));
        let lits: Vec<LiteralValue> = vals.iter().filter_map(|v| parse_literal(v)).collect();
        let refs: Vec<TermRef> = vals
            .iter()
            .filter(|v| is_ref(v))
            .filter_map(|v| v.get("@id").and_then(Value::as_str))
            .map(to_ref)
            .collect();
        if !refs.is_empty() {
            links.push(NamedRefs { predicate: pred.clone(), label: pred_label(pred, namespace_prefixes), refs });
        } else if !lits.is_empty() {
            metadata.push(NamedValues {
                predicate: pred.clone(),
                label: pred_label(pred, namespace_prefixes),
                values: prefer_en(lits),
            });
        }
    }

    OntologyHeader { iri: iri.to_string(), curie: to_curie(iri, namespace_prefixes), title, descriptions, metadata, links }
}

#[allow(clippy::too_many_arguments)]
fn build_term<F: Fn(&str) -> TermRef>(
    node: &Map<String, Value>,
    iri: &str,
    kind: TermKind,
    anchor: &str,
    node_map: &HashMap<String, &Map<String, Value>>,
    to_ref: &F,
    properties: Vec<PropertyRef>,
    namespace_prefixes: Option<&HashMap<String, String>>,
) -> Term {
    let label = best_label(node).unwrap_or_else(|| to_curie(iri, namespace_prefixes));
    let mut descriptions: Vec<LiteralValue> = Vec::new();
    let mut notes: Vec<NamedValues> = Vec::new();
    let mut annotations: Vec<NamedValues> = Vec::new();
    let mut relations: Vec<NamedRefs> = Vec::new();
    let mut deprecated = false;

    for (pred, raw_val) in node.iter() {
        if pred.starts_with('@') || LABEL_PREDS.iter().any(|p| p == pred) {
            continue;
        }
        if pred.as_str() == OWL_DEPRECATED.as_str() {
            // TS takes `arr(rawVal).map(parseLiteral)[0]` — the first raw
            // element specifically (parsed or not), not the first element
            // that successfully parses as a literal.
            let lit = arr(Some(raw_val)).first().and_then(|v| parse_literal(v));
            deprecated = lit.map(|l| l.value.to_lowercase()).as_deref() == Some("true");
            continue;
        }
        if SKIP_PREDS.contains(pred) {
            continue;
        }

        let vals = arr(Some(raw_val));
        let lits: Vec<LiteralValue> = vals.iter().filter_map(|v| parse_literal(v)).collect();

        if DESC_PREDS.iter().any(|p| p == pred) {
            descriptions.extend(lits);
            continue;
        }
        if NOTE_PREDS.iter().any(|p| p == pred) {
            if !lits.is_empty() {
                notes.push(NamedValues {
                    predicate: pred.clone(),
                    label: pred_label(pred, namespace_prefixes),
                    values: prefer_en(lits),
                });
            }
            continue;
        }
        let iris = collect_iris(&vals, node_map, 0);
        if !iris.is_empty() {
            let mut seen = HashSet::new();
            let refs: Vec<TermRef> = iris.into_iter().filter(|x| seen.insert(x.clone())).map(|x| to_ref(&x)).collect();
            relations.push(NamedRefs { predicate: pred.clone(), label: pred_label(pred, namespace_prefixes), refs });
        } else if !lits.is_empty() {
            annotations.push(NamedValues {
                predicate: pred.clone(),
                label: pred_label(pred, namespace_prefixes),
                values: prefer_en(lits),
            });
        }
    }

    relations.sort_by(|a, b| rel_order(&a.predicate).cmp(&rel_order(&b.predicate)));
    let clean_desc = prefer_en(descriptions);
    let mut haystack_parts: Vec<String> = vec![label.clone(), to_curie(iri, namespace_prefixes), iri.to_string()];
    haystack_parts.extend(clean_desc.iter().map(|d| d.value.clone()));
    let haystack = haystack_parts.join(" ").to_lowercase();

    Term {
        iri: iri.to_string(),
        anchor: anchor.to_string(),
        curie: to_curie(iri, namespace_prefixes),
        kind,
        types: get_types(node),
        label,
        deprecated,
        haystack,
        descriptions: clean_desc,
        notes,
        annotations,
        relations,
        properties,
    }
}

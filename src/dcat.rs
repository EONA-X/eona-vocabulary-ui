//! Parse **expanded** JSON-LD for the static `catalog.jsonld` (see
//! `pipelines/publish-widoco-docs/generate.py`'s `build_catalog_turtle`) into
//! the view model `pages::catalog` renders: a real `dcat:Catalog` of
//! `dcat:Dataset`s, one per published ontology, each with its
//! `dcat:Distribution`s.
//!
//! Deliberately its own small parser rather than reusing `crate::ontology`'s
//! `parse_ontology`: that one builds a class/property/individual term model
//! for a single OWL ontology, a much richer (and unrelated) shape than a flat
//! DCAT catalog listing.

use std::collections::HashMap;

use serde_json::{Map, Value};

const DCAT: &str = "http://www.w3.org/ns/dcat#";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const FOAF: &str = "http://xmlns.com/foaf/0.1/";

#[derive(Clone, Debug, PartialEq)]
pub struct Distribution {
    pub title: String,
    pub download_url: String,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Creator {
    pub name: String,
}

/// A dataset's asset type, as in the published IRIs
/// (`https://<host>/<asset-type>/<slug>/<version>`): the one covering its
/// `dcat:type` from the EU asset-classification table (the same table as
/// eona-vocabularies-reference's `ASSET_TYPES`). The former generator's
/// `dcterms:type` literals ("OWL Ontology", "SHACL Shapes Graph",
/// "Crosswalk") are still read when there is no `dcat:type`. `Other` carries
/// anything unrecognised through; `pages::catalog` shows no badge for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatasetKind {
    Ontology,
    Shape,
    Crosswalk,
    Vocabulary,
    Codelist,
    Other(String),
}

const ASSET_CLASSIFICATION: &str = "http://publications.europa.eu/resource/authority/asset-classification/";

/// EU asset-classification concepts covered by each asset type.
const ASSET_TYPES: [(&[&str], DatasetKind); 5] = [
    (&["c_89b4bdb7"], DatasetKind::Ontology),
    (&["c_b37963b3", "c_3948c2ed"], DatasetKind::Shape),
    (&["c_bba2bb35"], DatasetKind::Crosswalk),
    (
        &["c_64714767", "c_a7773248", "c_5796a20b", "c_ebfb658e", "c_25e514f4", "c_d3bf7907", "c_ecacbeba", "c_b0bfba6e"],
        DatasetKind::Vocabulary,
    ),
    (&["c_cdd11291", "c_5b130cc6"], DatasetKind::Codelist),
];

impl DatasetKind {
    /// From a `dcat:type` IRI.
    pub fn from_asset_class(iri: &str) -> Self {
        let code = iri.strip_prefix(ASSET_CLASSIFICATION).unwrap_or(iri);
        ASSET_TYPES
            .iter()
            .find(|(codes, _)| codes.contains(&code))
            .map_or_else(|| DatasetKind::Other(iri.to_string()), |(_, kind)| kind.clone())
    }
}

impl From<&str> for DatasetKind {
    /// From a former `dcterms:type` literal.
    fn from(raw: &str) -> Self {
        match raw {
            "OWL Ontology" => DatasetKind::Ontology,
            "SHACL Shapes Graph" => DatasetKind::Shape,
            "Crosswalk" => DatasetKind::Crosswalk,
            other => DatasetKind::Other(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogDataset {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub landing_page: Option<String>,
    pub conforms_to: Option<String>,
    /// The ontology's own owl:versionInfo, verbatim (see generate.py's
    /// find_version_info) — not necessarily valid semver (e.g. odrl22
    /// declares "2.2"). `pages::catalog` is responsible for degrading
    /// gracefully when it isn't parseable.
    pub version: Option<String>,
    /// The card's hero image: the upstream's own logo when `creator` is
    /// set, else the default owl illustration (see generate.py's
    /// DEFAULT_THUMBNAIL / UPSTREAM_CREATORS).
    pub thumbnail: Option<String>,
    /// The upstream standards body this ontology is derived from, when it
    /// is one (see generate.py's UPSTREAM_CREATORS) — absent for Eona-X's
    /// own originals, which have no external upstream to credit.
    pub creator: Option<Creator>,
    /// This ontology's `dcterms:type` (see `DatasetKind`) — `None` when the
    /// catalog was generated before that predicate existed, so callers must
    /// degrade gracefully (no badge) rather than assume it's always set.
    pub kind: Option<DatasetKind>,
    pub distributions: Vec<Distribution>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CatalogModel {
    pub title: Option<String>,
    pub description: Option<String>,
    pub datasets: Vec<CatalogDataset>,
}

fn arr(v: Option<&Value>) -> Vec<&Value> {
    match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().collect(),
        Some(other) => vec![other],
    }
}

fn first_literal(node: &Map<String, Value>, pred: &str) -> Option<String> {
    arr(node.get(pred)).into_iter().find_map(|v| v.get("@value")).and_then(Value::as_str).map(String::from)
}

fn first_ref(node: &Map<String, Value>, pred: &str) -> Option<String> {
    arr(node.get(pred)).into_iter().find_map(|v| v.get("@id")).and_then(Value::as_str).map(String::from)
}

fn has_type(node: &Map<String, Value>, iri: &str) -> bool {
    node.get("@type").and_then(Value::as_array).is_some_and(|types| types.iter().any(|t| t == iri))
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

/// Site-relative path (as served under `DOCS_BASE`) for an absolute catalog
/// IRI minted under `CATALOG_DOCS_BASE_IRI` in generate.py (e.g.
/// `https://vocab.eona-x.eu/docs/odrl22/ontology.ttl` ->
/// `/odrl22/ontology.ttl`). Falls back to the bare IRI, unchanged, for
/// anything not under that prefix (defensive — shouldn't happen with the
/// current generator).
pub fn site_relative(iri: &str) -> String {
    // vocabulary-hub.eona-x.eu's catalog, then the former generator's.
    const PREFIXES: [&str; 2] = ["https://vocabulary.eona-x.eu/docs", "https://vocab.eona-x.eu/docs"];
    PREFIXES.iter().find_map(|p| iri.strip_prefix(p)).map(String::from).unwrap_or_else(|| iri.to_string())
}

pub fn parse_catalog(doc: &Value) -> CatalogModel {
    let nodes = extract_nodes(doc);
    let node_map: HashMap<&str, &Map<String, Value>> = nodes
        .iter()
        .filter_map(|n| n.get("@id").and_then(Value::as_str).map(|id| (id, *n)))
        .collect();

    let Some(catalog_node) = nodes.iter().find(|n| has_type(n, &format!("{DCAT}Catalog"))) else {
        return CatalogModel::default();
    };

    let title = first_literal(catalog_node, &format!("{DCTERMS}title"));
    let description = first_literal(catalog_node, &format!("{DCTERMS}description"));

    let dataset_refs: Vec<String> =
        arr(catalog_node.get(format!("{DCAT}dataset").as_str())).into_iter().filter_map(|v| v.get("@id")).filter_map(Value::as_str).map(String::from).collect();

    let mut datasets: Vec<CatalogDataset> = dataset_refs
        .iter()
        .filter_map(|iri| node_map.get(iri.as_str()).copied())
        .filter_map(|node| {
            let slug = first_literal(node, &format!("{DCTERMS}identifier"))?;
            let title = first_literal(node, &format!("{DCTERMS}title")).unwrap_or_else(|| slug.clone());
            let description = first_literal(node, &format!("{DCTERMS}description"));
            let landing_page = first_ref(node, &format!("{DCAT}landingPage"));
            let conforms_to = first_ref(node, &format!("{DCTERMS}conformsTo"));
            let version = first_literal(node, &format!("{DCAT}version"));
            let kind = first_ref(node, &format!("{DCAT}type"))
                .map(|iri| DatasetKind::from_asset_class(&iri))
                .or_else(|| first_literal(node, &format!("{DCTERMS}type")).map(|raw| DatasetKind::from(raw.as_str())));
            let thumbnail = first_ref(node, &format!("{FOAF}thumbnail"));
            let creator = first_ref(node, &format!("{DCTERMS}creator"))
                .and_then(|iri| node_map.get(iri.as_str()).copied())
                .and_then(|agent| Some(Creator { name: first_literal(agent, &format!("{FOAF}name"))? }));

            let distributions: Vec<Distribution> = arr(node.get(format!("{DCAT}distribution").as_str()))
                .into_iter()
                .filter_map(|v| v.get("@id").and_then(Value::as_str))
                .filter_map(|iri| node_map.get(iri).copied())
                .filter_map(|dist| {
                    Some(Distribution {
                        title: first_literal(dist, &format!("{DCTERMS}title"))?,
                        download_url: first_ref(dist, &format!("{DCAT}downloadURL"))?,
                        media_type: first_literal(dist, &format!("{DCAT}mediaType")).unwrap_or_default(),
                    })
                })
                .collect();

            Some(CatalogDataset { slug, title, description, landing_page, conforms_to, version, kind, thumbnail, creator, distributions })
        })
        .collect();

    datasets.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

    CatalogModel { title, description, datasets }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ASSETTYPE: &str = "http://publications.europa.eu/resource/authority/asset-classification/";

    fn catalog_with(dataset: Value) -> Value {
        json!([
            {"@id": "https://vocabulary.eona-x.eu/catalog", "@type": [format!("{DCAT}Catalog")],
             format!("{DCAT}dataset"): [{"@id": "https://vocabulary.eona-x.eu/catalog/x"}]},
            dataset
        ])
    }

    fn kind_of(dataset: Value) -> Option<DatasetKind> {
        parse_catalog(&catalog_with(dataset)).datasets.pop().unwrap().kind
    }

    #[test]
    fn the_hub_catalog_iris_under_vocabulary_eona_x_eu_resolve_to_site_paths() {
        assert_eq!(site_relative("https://vocabulary.eona-x.eu/docs/assets/cen-hero.png"), "/assets/cen-hero.png");
        assert_eq!(site_relative("https://vocabulary.eona-x.eu/docs/ontologies?ontology=netex"), "/ontologies?ontology=netex");
        // The former generator's host still resolves.
        assert_eq!(site_relative("https://vocab.eona-x.eu/docs/did/ontology.ttl"), "/did/ontology.ttl");
    }

    #[test]
    fn the_kind_is_the_asset_type_covering_the_eu_dcat_type() {
        for (code, kind) in [
            ("c_89b4bdb7", DatasetKind::Ontology),
            ("c_3948c2ed", DatasetKind::Shape),
            ("c_bba2bb35", DatasetKind::Crosswalk),
            ("c_a7773248", DatasetKind::Vocabulary),
            ("c_cdd11291", DatasetKind::Codelist),
        ] {
            let dataset = json!({"@id": "https://vocabulary.eona-x.eu/catalog/x",
                format!("{DCTERMS}identifier"): [{"@value": "x"}],
                format!("{DCAT}type"): [{"@id": format!("{ASSETTYPE}{code}")}]});
            assert_eq!(kind_of(dataset), Some(kind), "{code}");
        }
    }

    #[test]
    fn the_former_dcterms_type_labels_still_give_a_kind() {
        for (label, kind) in [
            ("OWL Ontology", DatasetKind::Ontology),
            ("SHACL Shapes Graph", DatasetKind::Shape),
            ("Crosswalk", DatasetKind::Crosswalk),
        ] {
            let dataset = json!({"@id": "https://vocabulary.eona-x.eu/catalog/x",
                format!("{DCTERMS}identifier"): [{"@value": "x"}],
                format!("{DCTERMS}type"): [{"@value": label}]});
            assert_eq!(kind_of(dataset), Some(kind), "{label}");
        }
    }
}

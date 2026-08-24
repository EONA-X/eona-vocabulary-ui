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

#[derive(Clone, Debug, PartialEq)]
pub struct Distribution {
    pub title: String,
    pub download_url: String,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogDataset {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub landing_page: Option<String>,
    pub conforms_to: Option<String>,
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
    const PREFIX: &str = "https://vocab.eona-x.eu/docs";
    iri.strip_prefix(PREFIX).map(String::from).unwrap_or_else(|| iri.to_string())
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

            Some(CatalogDataset { slug, title, description, landing_page, conforms_to, distributions })
        })
        .collect();

    datasets.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

    CatalogModel { title, description, datasets }
}

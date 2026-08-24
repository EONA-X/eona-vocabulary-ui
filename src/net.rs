//! Small fetch / URL-query-sync helpers shared by every page — both
//! `pages::ontologies` and `pages::crosswalk` fetch static `/docs/*.json`
//! manifests and documents the same way, and both deep-link their current
//! selection into the URL the same way.
#![allow(dead_code)]

use serde::Deserialize;
use wasm_bindgen::JsValue;
use web_sys::{window, UrlSearchParams};

/// Fetch a static file as text and `serde_json`-parse it — robust regardless
/// of the content-type nginx serves it with (`.jsonld` has no default MIME
/// mapping), mirroring the Vue source's `fetchJson` helper.
pub async fn fetch_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let resp = gloo_net::http::Request::get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Read a query parameter from `window.location.search`.
pub fn query_param(name: &str) -> Option<String> {
    let search = window()?.location().search().ok()?;
    UrlSearchParams::new_with_str(&search).ok()?.get(name)
}

/// Reflect the selected slug into `?<param>=<slug>` via
/// `history.replaceState` (no navigation/reload) so the view stays
/// deep-linkable/shareable — mirrors the Vue source's
/// `router.replace({ query: { ...route.query, [param]: slug } })`, only
/// touching the URL when the query param doesn't already name this slug.
pub fn sync_url_slug(param: &str, slug: &str) {
    if query_param(param).as_deref() == Some(slug) {
        return;
    }
    let Some(win) = window() else { return };
    let loc = win.location();
    let Ok(search) = loc.search() else { return };
    let Ok(params) = UrlSearchParams::new_with_str(&search) else { return };
    params.set(param, slug);
    let Ok(pathname) = loc.pathname() else { return };
    let hash = loc.hash().unwrap_or_default();
    let query = String::from(params.to_string());
    let new_url = format!("{pathname}?{query}{hash}");
    if let Ok(history) = win.history() {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&new_url));
    }
}

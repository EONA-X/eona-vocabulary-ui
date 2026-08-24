//! ATOM · ThemeToggle
//!
//! Flips the app between EONA's dark (default) and light token sets — see
//! `styles/main.css`'s `:root` / `:root[data-theme="light"]` blocks — and,
//! in lockstep, PatternFly's own `pf-v6-theme-dark` class so
//! `pages::catalog`'s patternfly-yew components (DatasetCard/Gallery) follow
//! the same switch instead of staying stuck in one theme (see index.html's
//! inline bootstrap script, which applies the persisted choice before Yew
//! mounts to avoid a flash). Persisted to `localStorage` under
//! [`STORAGE_KEY`] — this SPA does full page reloads between routes (see
//! `main.rs`), so anything a route needs to remember has to live outside Yew
//! state.

use wasm_bindgen::JsCast;
use web_sys::window;
use yew::prelude::*;

pub const STORAGE_KEY: &str = "eovoc-theme";

fn is_light() -> bool {
    window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .as_deref()
        == Some("light")
}

/// Applies `light` to the DOM (both our own tokens and PatternFly's).
/// Exported so index.html's inline bootstrap script and this component agree
/// on exactly one place that knows the two attributes/classes involved.
pub fn apply_theme(light: bool) {
    let Some(root) = window().and_then(|w| w.document()).and_then(|d| d.document_element()) else {
        return;
    };
    if light {
        let _ = root.set_attribute("data-theme", "light");
    } else {
        let _ = root.remove_attribute("data-theme");
    }
    if let Some(html) = root.dyn_ref::<web_sys::HtmlElement>() {
        html.set_class_name(if light { "" } else { "pf-v6-theme-dark" });
    }
}

#[function_component(ThemeToggle)]
pub fn theme_toggle() -> Html {
    let light = use_state(is_light);

    let onclick = {
        let light = light.clone();
        Callback::from(move |_| {
            let next = !*light;
            apply_theme(next);
            if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                let _ = storage.set_item(STORAGE_KEY, if next { "light" } else { "dark" });
            }
            light.set(next);
        })
    };

    html! {
        <button
            type="button"
            class="eovoc-theme-toggle"
            onclick={onclick}
            aria-label="Toggle light/dark theme"
        >
            if *light { { "\u{2600} Light" } } else { { "\u{1f319} Dark" } }
        </button>
    }
}

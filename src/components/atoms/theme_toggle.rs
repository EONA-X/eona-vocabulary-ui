//! ATOM · ThemeToggle
//!
//! Flips between the charter's light and dark token sets and persists the
//! choice.
//!
//! Dark is the opt-in state, matching `tokens.css`: light lives on `:root` and
//! dark under `:root[data-theme="dark"]`. An app that wants to open dark says
//! so on its own `<html>` element rather than expecting this component to
//! invert the design system.
//!
//! It also keeps PatternFly's `pf-v6-theme-dark` class in lockstep, so an app
//! that mixes `patternfly-yew` components with these follows one switch instead
//! of two. Harmless where PatternFly is absent — the class then matches
//! nothing.
//!
//! [`apply_theme`] is public so a host's pre-mount bootstrap script and this
//! component agree on exactly one place that knows which attribute and class
//! are involved; see this repo's `index.html` for why that script exists (it
//! applies the persisted choice before the app mounts, so a returning visitor
//! never sees the wrong theme flash first).

use wasm_bindgen::JsCast;
use web_sys::window;
use yew::prelude::*;

/// Default `localStorage` key. Override with [`ThemeToggleProps::storage_key`]
/// when an app already persists under a name of its own.
pub const STORAGE_KEY: &str = "eona-theme";

/// The visitor's stored choice, or `default_dark` when they have none.
///
/// The fallback is not cosmetic: a dark-first app opens with
/// `data-theme="dark"` already on `<html>`, and a toggle that assumed light
/// would render the wrong label until the first click.
fn stored_is_dark(key: &str, default_dark: bool) -> bool {
    match window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(key).ok().flatten())
        .as_deref()
    {
        Some("dark") => true,
        Some("light") => false,
        _ => default_dark,
    }
}

/// Applies the theme to the document element — the charter's `data-theme` and
/// PatternFly's own class together.
pub fn apply_theme(dark: bool) {
    let Some(root) = window().and_then(|w| w.document()).and_then(|d| d.document_element()) else {
        return;
    };
    if dark {
        let _ = root.set_attribute("data-theme", "dark");
    } else {
        let _ = root.remove_attribute("data-theme");
    }
    if let Some(html) = root.dyn_ref::<web_sys::HtmlElement>() {
        html.set_class_name(if dark { "pf-v6-theme-dark" } else { "" });
    }
}

#[derive(Properties, PartialEq)]
pub struct ThemeToggleProps {
    /// `localStorage` key holding `"dark"` or `"light"`. Defaults to
    /// [`STORAGE_KEY`]; pass an app's existing key to keep visitors' saved
    /// preference working.
    #[prop_or(AttrValue::Static(STORAGE_KEY))]
    pub storage_key: AttrValue,
    /// Which theme the host opens in when the visitor has no stored choice.
    /// Set it to whatever the host's `<html>` already carries — `true` for an
    /// app that ships `data-theme="dark"`.
    #[prop_or_default]
    pub default_dark: bool,
}

#[function_component(ThemeToggle)]
pub fn theme_toggle(props: &ThemeToggleProps) -> Html {
    let key = props.storage_key.clone();
    let dark = use_state({
        let key = key.clone();
        let default_dark = props.default_dark;
        move || stored_is_dark(&key, default_dark)
    });

    let onclick = {
        let dark = dark.clone();
        let key = key.clone();
        Callback::from(move |_| {
            let next = !*dark;
            apply_theme(next);
            if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                let _ = storage.set_item(&key, if next { "dark" } else { "light" });
            }
            dark.set(next);
        })
    };

    html! {
        <button
            type="button"
            class="eovoc-theme-toggle"
            onclick={onclick}
            aria-label="Toggle light/dark theme"
        >
            if *dark { { "\u{1f319} Dark" } } else { { "\u{2600} Light" } }
        </button>
    }
}

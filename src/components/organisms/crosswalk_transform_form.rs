//! ORGANISM · CrosswalkTransformForm
//!
//! New functionality (no Vue/TS source to port — the Yew migration's
//! upstream `prez-ui` predates the `eona-crosswalk-transform` crate this
//! composes). Lets a person actually *use* the crosswalk this page
//! visualises: paste/type an instance JSON-LD document, rewrite it through
//! the alignment's mapping, see the transformed document plus a
//! human-readable interpretation report.
//!
//! `eona-crosswalk-transform`'s pure-Rust core (`build_mapping`/`transform`)
//! is called directly as an ordinary Rust dependency — eovoc-ui already
//! compiles the whole app into one wasm binary, so there's no JS/wasm-bindgen
//! boundary to cross here (that crate's own `wasm-bindgen` surface is for
//! *external* JS consumers of the standalone package, and is dead-code-
//! eliminated out of this binary since nothing here calls it).
//!
//! The crate's `Mapping` is bidirectional by construction — the same built
//! mapping rewrites either side's terms to the other, there's no "direction"
//! argument in `transform`. The directional switch below is purely a UX
//! framing device: which side's vocabulary the example/labels describe the
//! input as being in. Flipping it swaps which side's IRIs the pre-filled
//! example is built from; it never changes the `transform` call itself.

use eona_crosswalk_transform::{build_mapping, transform, Mapping, MatchType, RewriteRecord, SkippedRecord};
use serde_json::{json, Map, Value};
use web_sys::HtmlTextAreaElement;
use yew::prelude::*;

use crate::crosswalk::{XwalkGraph, XwalkTier};

#[derive(Properties, PartialEq, Clone)]
pub struct CrosswalkTransformFormProps {
    pub graph: XwalkGraph,
    /// The alignment document's raw JSON-LD text — `build_mapping` wants the
    /// reified-SKOS document itself, not the `OntologyModel` the rest of
    /// this page already parsed it into for the 3D visualisation.
    pub alignment_text: AttrValue,
}

const CONFIDENCE_OPTIONS: [(MatchType, &str); 3] =
    [(MatchType::Exact, "Exact only"), (MatchType::Close, "Exact + Close"), (MatchType::Related, "Exact + Close + Related")];

fn match_type_label(m: MatchType) -> &'static str {
    match m {
        MatchType::Exact => "exact",
        MatchType::Close => "close",
        MatchType::Related => "related",
    }
}

/// A small, illustrative instance document built from real IRIs in the
/// currently-active side of this alignment — an entity-tier term (as
/// `@type`) and, when one exists, an attribute-tier term (as a property
/// key), both chosen only from terms the mapping actually knows about, so
/// the pre-filled example always demonstrates a real rewrite rather than a
/// guessed-at IRI that happens to be unmapped.
fn generate_example(graph: &XwalkGraph, mapping: &Mapping, side_slug: &str) -> String {
    let entity = graph.nodes.iter().find(|n| n.side == side_slug && n.tier == XwalkTier::Entity && mapping.get(&n.iri).is_some());
    let attribute = graph.nodes.iter().find(|n| n.side == side_slug && n.tier == XwalkTier::Attribute && mapping.get(&n.iri).is_some());

    let mut node = Map::new();
    node.insert("@id".to_string(), json!("https://instance.example/thing-1"));
    if let Some(e) = entity {
        node.insert("@type".to_string(), json!([e.iri]));
    }
    if let Some(a) = attribute {
        node.insert(a.iri.clone(), json!([{ "@value": "example-value" }]));
    }
    if entity.is_none() && attribute.is_none() {
        // No side-tagged term happens to be in the mapping (shouldn't occur
        // for a real, non-empty alignment, but keep the textarea non-empty
        // and valid JSON-LD regardless).
        node.insert("@type".to_string(), json!(["https://instance.example/UnmappedExample"]));
    }
    serde_json::to_string_pretty(&Value::Array(vec![Value::Object(node)])).unwrap_or_default()
}

struct TransformOutcome {
    document_pretty: String,
    rewritten: Vec<RewriteRecord>,
    skipped: Vec<SkippedRecord>,
    unmapped: Vec<String>,
}

#[function_component(CrosswalkTransformForm)]
pub fn crosswalk_transform_form(props: &CrosswalkTransformFormProps) -> Html {
    let mapping = use_memo(props.alignment_text.clone(), |text| build_mapping(text));

    let active_side = use_state(|| 0usize);
    // Loosest by default: an interactive exploration tool should show what
    // the crosswalk actually knows about the pasted document immediately —
    // exact/close/related are all visible in the report either way, this
    // just controls what actually gets rewritten vs. reported as skipped.
    let min_confidence = use_state(|| MatchType::Related);
    let input_text = use_state(String::new);
    let result = use_state(|| None::<Result<TransformOutcome, String>>);

    // (Re)seed the textarea with a fresh example whenever the active side
    // flips, the alignment changes, or the mapping first becomes available.
    {
        let input_text = input_text.clone();
        let result = result.clone();
        let graph = props.graph.clone();
        let mapping = mapping.clone();
        use_effect_with((*active_side, props.alignment_text.clone()), move |(side_idx, _)| {
            if let Ok(m) = mapping.as_ref() {
                if let Some(side) = graph.sides.get(*side_idx) {
                    input_text.set(generate_example(&graph, m, &side.slug));
                    result.set(None);
                }
            }
            || ()
        });
    }

    let on_input = {
        let input_text = input_text.clone();
        Callback::from(move |e: InputEvent| {
            let ta: HtmlTextAreaElement = e.target_unchecked_into();
            input_text.set(ta.value());
        })
    };

    let on_confidence_change = {
        let min_confidence = min_confidence.clone();
        Callback::from(move |e: Event| {
            let select: web_sys::HtmlSelectElement = e.target_unchecked_into();
            if let Ok(idx) = select.value().parse::<usize>() {
                if let Some((m, _)) = CONFIDENCE_OPTIONS.get(idx) {
                    min_confidence.set(*m);
                }
            }
        })
    };

    let on_transform = {
        let mapping = mapping.clone();
        let input_text = input_text.clone();
        let min_confidence = *min_confidence;
        let result = result.clone();
        Callback::from(move |_: MouseEvent| {
            let Ok(m) = mapping.as_ref() else {
                result.set(Some(Err("The alignment document itself could not be parsed as a crosswalk mapping.".to_string())));
                return;
            };
            match transform(m, &input_text, min_confidence) {
                Ok(r) => {
                    let document_pretty = serde_json::to_string_pretty(&r.document).unwrap_or_else(|e| format!("(could not pretty-print: {e})"));
                    result.set(Some(Ok(TransformOutcome {
                        document_pretty,
                        rewritten: r.report.rewritten,
                        skipped: r.report.skipped_below_threshold,
                        unmapped: r.report.unmapped,
                    })));
                }
                Err(e) => result.set(Some(Err(e.to_string()))),
            }
        })
    };

    let sides = &props.graph.sides;
    let other_side_idx = if *active_side == 0 { 1 } else { 0 };
    let from_title = sides.get(*active_side).map(|s| s.title.as_str()).unwrap_or("Side A");
    let to_title = sides.get(other_side_idx).map(|s| s.title.as_str()).unwrap_or("Side B");

    html! {
        <div class="crosswalk-form">
            <h2 class="crosswalk-form__title">{ "Try the crosswalk" }</h2>
            <p class="crosswalk-form__lede">
                { "Paste or edit an instance JSON-LD document below and rewrite it through this crosswalk's mapping \u{2014} vocabulary terms this alignment knows about are rewritten to their counterpart; everything else passes through unchanged." }
            </p>

            if let Err(e) = mapping.as_ref() {
                <div class="eovoc-state eovoc-state--error">
                    <p class="eovoc-state__title">{ "Could not build a mapping from this alignment" }</p>
                    <p class="eovoc-state__message">{ e.to_string() }</p>
                </div>
            } else {
                <>
                    <div class="crosswalk-form__controls">
                        <div class="crosswalk-form__switch" role="group" aria-label="Direction">
                            { for sides.iter().enumerate().map(|(i, s)| {
                                let other = sides.get(1 - i.min(1)).map(|o| o.title.as_str()).unwrap_or("\u{2026}");
                                let active_side = active_side.clone();
                                let is_active = i == *active_side;
                                html! {
                                    <button
                                        key={s.slug.clone()}
                                        type="button"
                                        class={classes!("crosswalk-form__switch-option", is_active.then_some("crosswalk-form__switch-option--active"))}
                                        aria-pressed={is_active.to_string()}
                                        onclick={Callback::from(move |_| active_side.set(i))}
                                    >
                                        { format!("{} \u{2192} {}", s.title, other) }
                                    </button>
                                }
                            }) }
                        </div>

                        <label class="crosswalk-form__confidence">
                            <span>{ "Minimum confidence" }</span>
                            <select onchange={on_confidence_change}>
                                { for CONFIDENCE_OPTIONS.iter().enumerate().map(|(i, (m, label))| html! {
                                    <option key={*label} value={i.to_string()} selected={*m == *min_confidence}>{ *label }</option>
                                }) }
                            </select>
                        </label>
                    </div>

                    <label class="crosswalk-form__field">
                        <span>{ format!("Instance JSON-LD (using {from_title} terms)") }</span>
                        <textarea
                            class="crosswalk-form__textarea"
                            spellcheck="false"
                            value={(*input_text).clone()}
                            oninput={on_input}
                        />
                    </label>

                    <button type="button" class="crosswalk-form__submit" onclick={on_transform}>{ "Transform" }</button>

                    {
                        match result.as_ref() {
                            None => html! {},
                            Some(Err(msg)) => html! {
                                <div class="eovoc-state eovoc-state--error crosswalk-form__result">
                                    <p class="eovoc-state__title">{ "Transform failed" }</p>
                                    <p class="eovoc-state__message">{ msg.clone() }</p>
                                </div>
                            },
                            Some(Ok(outcome)) => html! {
                                <div class="crosswalk-form__result">
                                    <div class="crosswalk-form__output-grid">
                                        <div>
                                            <h3 class="crosswalk-form__subheading">{ format!("Result ({to_title} terms where rewritten)") }</h3>
                                            <pre class="crosswalk-form__pre"><code>{ outcome.document_pretty.clone() }</code></pre>
                                        </div>
                                        <div class="crosswalk-form__report">
                                            <h3 class="crosswalk-form__subheading">{ format!("Rewritten ({})", outcome.rewritten.len()) }</h3>
                                            if outcome.rewritten.is_empty() {
                                                <p class="crosswalk-form__empty">{ "Nothing in the input matched a mapped term at this confidence." }</p>
                                            } else {
                                                <ul class="crosswalk-form__report-list">
                                                    { for outcome.rewritten.iter().map(|r| html! {
                                                        <li key={format!("{}-{}", r.from, r.to)}>
                                                            <code>{ r.from.clone() }</code>
                                                            { " \u{2192} " }
                                                            <code>{ r.to.clone() }</code>
                                                            <span class="crosswalk-form__match-tag">{ match_type_label(r.match_type) }</span>
                                                        </li>
                                                    }) }
                                                </ul>
                                            }

                                            <h3 class="crosswalk-form__subheading">{ format!("Skipped \u{2014} below threshold ({})", outcome.skipped.len()) }</h3>
                                            if outcome.skipped.is_empty() {
                                                <p class="crosswalk-form__empty">{ "Nothing was mappable-but-excluded at this confidence." }</p>
                                            } else {
                                                <ul class="crosswalk-form__report-list">
                                                    { for outcome.skipped.iter().map(|s| html! {
                                                        <li key={s.iri.clone()}>
                                                            <code>{ s.iri.clone() }</code>
                                                            { " \u{2014} would map to: " }
                                                            { for s.would_map_to.iter().enumerate().map(|(i, e)| html! {
                                                                <>
                                                                    if i > 0 { { ", " } }
                                                                    <code>{ e.counterpart_iri.clone() }</code>
                                                                    <span class="crosswalk-form__match-tag">{ match_type_label(e.match_type) }</span>
                                                                </>
                                                            }) }
                                                        </li>
                                                    }) }
                                                </ul>
                                            }

                                            <h3 class="crosswalk-form__subheading">{ format!("Unmapped ({})", outcome.unmapped.len()) }</h3>
                                            if outcome.unmapped.is_empty() {
                                                <p class="crosswalk-form__empty">{ "Every @type/property IRI in the input is known to this crosswalk." }</p>
                                            } else {
                                                <ul class="crosswalk-form__report-list crosswalk-form__report-list--plain">
                                                    { for outcome.unmapped.iter().map(|iri| html! {
                                                        <li key={iri.clone()}><code>{ iri.clone() }</code></li>
                                                    }) }
                                                </ul>
                                            }
                                        </div>
                                    </div>
                                </div>
                            },
                        }
                    }
                </>
            }
        </div>
    }
}

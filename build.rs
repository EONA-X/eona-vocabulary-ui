//! Writes the design system's stylesheets out of `eona-ui-toolkit` into
//! `styles/vendor/`, where `index.html` links them.
//!
//! They are crate constants, not files this repo owns — that is the whole point
//! of `TOKENS_CSS`/`COMPONENTS_CSS`/`ONTOLOGY_CSS`/`PATTERNFLY_SKIN_CSS`, so no
//! consumer has to know where the crate sits on disk. Trunk, though, links
//! stylesheets by path from `index.html`. Materialising them here bridges the
//! two: the constants stay the single source of truth, and the paths keep
//! working whether the toolkit is a sibling checkout or a git dependency
//! unpacked under `~/.cargo`.
//!
//! `styles/vendor/` is generated but **committed**, which is deliberate: Trunk
//! resolves `index.html`'s `rel="css"` links before it invokes cargo, so a file
//! this script has not written yet does not exist when Trunk looks for it, and
//! a fresh checkout fails to build. Committing the output makes `trunk serve`
//! work on a clean clone; this script then keeps it in step on every build, so
//! a stale copy self-heals rather than persisting.
//!
//! Edit the toolkit, not these files — an edit here is overwritten by the next
//! `cargo build`, and `cargo test` fails if the two have diverged.

use std::fs;
use std::path::Path;

fn main() {
    let out = Path::new("styles/vendor");
    fs::create_dir_all(out).expect("create styles/vendor");

    for (name, css) in [
        ("tokens.css", eona_ui_toolkit::TOKENS_CSS),
        ("components.css", eona_ui_toolkit::COMPONENTS_CSS),
        ("ontology.css", eona_ui_toolkit::ONTOLOGY_CSS),
    ] {
        let path = out.join(name);
        // Only rewrite on change: Trunk watches styles/, and an unconditional
        // write on every build would have it rebuild itself in a loop.
        if fs::read_to_string(&path).ok().as_deref() != Some(css) {
            fs::write(&path, css).unwrap_or_else(|e| panic!("write {name}: {e}"));
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
}

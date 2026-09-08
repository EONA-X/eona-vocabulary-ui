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
//! `styles/vendor/` is generated and gitignored. Edit the toolkit, not these.

use std::fs;
use std::path::Path;

fn main() {
    let out = Path::new("styles/vendor");
    fs::create_dir_all(out).expect("create styles/vendor");

    for (name, css) in [
        ("tokens.css", eona_ui_toolkit::TOKENS_CSS),
        ("components.css", eona_ui_toolkit::COMPONENTS_CSS),
        ("ontology.css", eona_ui_toolkit::ONTOLOGY_CSS),
        ("patternfly-skin.css", eona_ui_toolkit::PATTERNFLY_SKIN_CSS),
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

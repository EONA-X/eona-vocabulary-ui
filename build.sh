#!/usr/bin/env bash
# Build a fully standalone dist/: `trunk build` (the Yew/WASM bundle) plus the
# `/docs` data tree it fetches client-side at runtime (see src/net.rs) — every
# page reads `/docs/*.json` manifests and `/docs/<slug>/*.jsonld` documents
# relative to wherever the bundle itself is served from, so baking a copy of
# that tree into dist/docs is what turns the bundle into a self-contained
# static site (no separate API, no runtime volume mount) deployable as-is to
# GitHub Pages or any static host.
#
# Usage: ./build.sh [DATA_DIR] [PUBLIC_URL]
#   DATA_DIR   - a directory shaped like a /docs tree (catalog.jsonld,
#                ontologies.json, alignments.json, <slug>/ontology.jsonld,
#                ...), e.g. the output of eona-vocabulary-services'
#                pipelines/publish-vocabulary-catalog/build.sh. Defaults to
#                sample-data/docs, a small bundled fixture set for demos/CI.
#   PUBLIC_URL - the path this build will be served under, e.g. "/" for a
#                root-mounted deploy (the default — matches this repo's own
#                Dockerfile) or "/eona-vocabulary-ui/" for a GitHub Pages
#                project site. Every internal link and fetch is written
#                relative rather than root-absolute for exactly this reason —
#                see index.html's <base data-trunk-public-url>.
set -euo pipefail

DATA_DIR="${1:-sample-data/docs}"
PUBLIC_URL="${2:-/}"

trunk build --release --public-url "$PUBLIC_URL"

rm -rf dist/docs
cp -r "$DATA_DIR" dist/docs

# GitHub Pages (unlike the nginx deploy — see default.conf's try_files) has no
# server-side rewrite: a direct/refreshed load of e.g. /ontologies has no file
# there and 404s. Pages does serve a repo-provided 404.html for any such miss,
# so shipping a copy of the app shell under that name lets main.rs's own
# pathname-based routing take over client-side once it loads.
cp dist/index.html dist/404.html

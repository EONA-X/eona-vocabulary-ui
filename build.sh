#!/usr/bin/env bash
# Build a fully standalone dist/: `trunk build` (the Yew/WASM bundle) plus the
# `/docs` data tree it fetches client-side at runtime (see src/net.rs) — every
# page reads `/docs/*.json` manifests and `/docs/<slug>/*.jsonld` documents
# relative to wherever the bundle itself is served from, so baking a copy of
# that tree into dist/docs is what turns the bundle into a self-contained
# static site (no separate API, no runtime volume mount) deployable as-is to
# GitHub Pages or any static host.
#
# Usage: ./build.sh [DATA_DIR]
#   DATA_DIR - a directory shaped like a /docs tree (catalog.jsonld,
#              ontologies.json, alignments.json, <slug>/ontology.jsonld, ...),
#              e.g. the output of eona-vocabulary-services'
#              pipelines/publish-vocabulary-catalog/build.sh. Defaults to
#              sample-data/docs, a small bundled fixture set for demos/CI.
set -euo pipefail

DATA_DIR="${1:-sample-data/docs}"

trunk build --release

rm -rf dist/docs
cp -r "$DATA_DIR" dist/docs

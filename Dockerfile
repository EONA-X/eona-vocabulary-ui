# EOVoc UI — Yew (Rust/WASM) SPA, standalone dist SSG.
#
# Two-stage build: compile the WASM bundle with Trunk against the pinned Rust
# toolchain (via build.sh, which also bakes in a /docs data tree — see that
# script), then hand the static output to nginx.
#
# eona-crosswalk-transform (an ordinary Cargo path dependency, see Cargo.toml)
# lives in this repo as the crates/eona-crosswalk-transform submodule, so the
# build context is just this repo — checkout it with submodules before
# building (`git clone --recurse-submodules`, or `git submodule update --init`).
ARG RUST_VERSION=1.97.1
# Pinned: `trunk`'s `[[node_packages]]` resolver (Trunk.toml, pulls in
# patternfly-yew's CSS for pages::catalog — see index.html) needs 0.22+; the
# unpinned `cargo install trunk` previously here resolved 0.21.14, which
# doesn't support it and fails the build. Same version dataspace-rs/edc-web-ui
# pins, for the same reason.
ARG TRUNK_VERSION=0.22.0-beta.2
# Must match the `wasm-bindgen` crate version pinned in Cargo.lock exactly
# (trunk's own wasm-bindgen invocation errors on any mismatch). Installed via
# cargo (this whole layer is cached, and cargo install retries a stalled
# crates.io download on its own) rather than left to trunk's own first-use
# auto-download of the prebuilt release archive straight from GitHub, which
# repeatedly stalled ~30s into the transfer and failed the build outright —
# with no persistent cache, every retry re-downloaded from scratch.
ARG WASM_BINDGEN_VERSION=0.2.127

FROM rust:${RUST_VERSION}-slim AS build
RUN rustup target add wasm32-unknown-unknown
ARG TRUNK_VERSION
RUN cargo install trunk@${TRUNK_VERSION} --locked
ARG WASM_BINDGEN_VERSION
RUN cargo install wasm-bindgen-cli@${WASM_BINDGEN_VERSION} --locked

WORKDIR /app
COPY . .

RUN ./build.sh

FROM nginx:stable-alpine
COPY --from=build /app/dist /usr/share/nginx/html
COPY default.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]

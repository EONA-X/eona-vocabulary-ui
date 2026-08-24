# EOVoc UI — Yew (Rust/WASM) SPA, replacing the Nuxt-based prez-ui.
#
# Two-stage build: compile the WASM bundle with Trunk against the pinned Rust
# toolchain, then hand the static output to the same nginx base prez-ui uses
# so the two UIs are interchangeable at the compose/nginx layer.
#
# Build context is the REPO ROOT (see docker-compose.yml's eovoc-ui service),
# not this directory: eovoc-ui depends on ../../crates/eona-crosswalk-transform
# as an ordinary Cargo path dependency, which a Docker build context rooted
# at containers/eovoc-ui alone could never reach. Every COPY below is
# therefore repo-root-relative.
ARG RUST_VERSION=1.97.1
# Pinned: `trunk`'s `[[node_packages]]` resolver (Trunk.toml, pulls in
# patternfly-yew's CSS for pages::catalog — see index.html) needs 0.22+; the
# unpinned `cargo install trunk` previously here resolved 0.21.14, which
# doesn't support it and fails the build. Same version dataspace-rs/edc-web-ui
# pins, for the same reason.
ARG TRUNK_VERSION=0.22.0-beta.2

FROM rust:${RUST_VERSION}-slim AS build
RUN rustup target add wasm32-unknown-unknown
ARG TRUNK_VERSION
RUN cargo install trunk@${TRUNK_VERSION} --locked

WORKDIR /app
COPY crates/eona-crosswalk-transform/ ./crates/eona-crosswalk-transform/
WORKDIR /app/containers/eovoc-ui
COPY containers/eovoc-ui/Cargo.toml containers/eovoc-ui/Cargo.lock ./
COPY containers/eovoc-ui/src/ ./src/
COPY containers/eovoc-ui/styles/ ./styles/
COPY containers/eovoc-ui/index.html containers/eovoc-ui/Trunk.toml ./

RUN trunk build --release

FROM nginx:stable-alpine
COPY --from=build /app/containers/eovoc-ui/dist /usr/share/nginx/html
COPY containers/eovoc-ui/default.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]

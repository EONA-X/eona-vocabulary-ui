# EOVoc UI — Yew (Rust/WASM) SPA, replacing the Nuxt-based prez-ui.
#
# Two-stage build: compile the WASM bundle with Trunk against the pinned Rust
# toolchain, then hand the static output to the same nginx base prez-ui uses
# so the two UIs are interchangeable at the compose/nginx layer.
ARG RUST_VERSION=1.97.1

FROM rust:${RUST_VERSION}-slim AS build
RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk --locked

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
COPY styles/ ./styles/
COPY index.html Trunk.toml ./

RUN trunk build --release

FROM nginx:stable-alpine
COPY --from=build /app/dist /usr/share/nginx/html
COPY default.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]

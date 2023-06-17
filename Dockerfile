# syntax=docker/dockerfile:1.5

# reference: https://gist.github.com/noelbundick/6922d26667616e2ba5c3aff59f0824cd

FROM rustlang/rust:nightly AS build

RUN cargo install --locked cargo-leptos
RUN rustup target add wasm32-unknown-unknown

RUN cargo new app
COPY Cargo.toml Cargo.lock rust-toolchain.toml /app/
RUN mkdir /app/public /app/style
RUN touch /app/src/lib.rs /app/style/main.scss

WORKDIR /app
RUN --mount=type=cache,target=/usr/local/cargo/registry cargo leptos build --release

COPY public /app/public
COPY style /app/style
COPY src /app/src

RUN --mount=type=cache,target=/usr/local/cargo/registry <<EOF
  set -e
  touch /app/src/lib.rs /app/src/main.rs
  cargo leptos build --release
EOF

ENTRYPOINT ["cargo", "leptos", "serve", "--release"]

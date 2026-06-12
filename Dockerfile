FROM rust:1.95.0-bookworm

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        diffutils \
        git \
        jq \
        python3 \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy rustfmt

WORKDIR /opt/cx-bootstrap

COPY rust/cxrs/Cargo.toml rust/cxrs/Cargo.lock rust/cxrs/rust-toolchain.toml ./rust/cxrs/
RUN mkdir -p rust/cxrs/src \
    && printf 'fn main() {}\n' > rust/cxrs/src/main.rs \
    && cargo fetch --manifest-path rust/cxrs/Cargo.toml --locked

WORKDIR /work


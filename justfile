default:
  @just --list

fmt:
  cargo fmt

clippy:
  cargo clippy

test:
  cargo test

run:
  cargo run --release

build:
  cargo build --release

gate: fmt clippy test

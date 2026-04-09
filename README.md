# simpledb
A Rust project for experimenting with simple database internals, including pages, B-trees, and supporting utilities.

## Requirements
- Rust toolchain installed via `rustup`
- Cargo, which is included with Rust

## Getting started
Clone the repository and build the project:

```bash
cargo build
```

Run the binary:

```bash
cargo run
```

Run tests:

```bash
cargo test
```

## Project structure
- `src/page/` contains page-related logic and tests
- `src/btree/` contains B-tree-related data structures
- `src/utils/` contains shared helper code
- `benches/` contains Criterion benchmarks

## Development workflow
Typical local workflow:

```bash
cargo build
cargo test
```

Commit and push changes:

```bash
git add .
git commit -m "describe what changed"
git push
```

## Continuous integration
GitHub Actions runs a basic Rust CI workflow on pushes and pull requests to:
- build the project with `cargo check`
- compile tests with `cargo test --no-run`

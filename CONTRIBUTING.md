# Contributing to Binary Config Schema

Thank you for your interest in contributing to BCS!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/trovante/bcs.git`
3. Create a feature branch: `git checkout -b feature/my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Commit your changes: `git commit -am 'Add new feature'`
7. Push to your fork: `git push origin feature/my-feature`
8. Create a Pull Request

## Development Guidelines

### Code Style

- **Rust**: Follow standard Rust conventions, use `rustfmt`

### Testing

- Write unit tests for all new functionality
- Ensure all tests pass before submitting PR
- Add integration tests for cross-component features
- Run read-performance regression gate for changes that may affect decode/path access:
  - `./scripts/bench-gate.sh`
- To refresh checked-in baselines and docs numbers from a real release measurement:
  - `./scripts/record-benchmarks.sh`
  - then update [`docs/benchmarks.md`](docs/benchmarks.md) from `benchmarks/measured-readme-fragment.md`

### Documentation

- Keep the root README concise (presentation); put detailed guides under `docs/`
- Add inline documentation for public APIs
- Update relevant documentation in `spec/` and `docs/`
- For FFI or language wrappers, update [`docs/bindings.md`](docs/bindings.md) and the matching README under `ffi/` or `bindings/<lang>/`
- Update [`docs/README.md`](docs/README.md) when adding a new top-level guide

### FFI / language bindings

1. Build natives: `cargo build -p bcs-ffi --release`
2. Package: `./scripts/package-ffi.sh`
3. Smoke-test: `./scripts/run-binding-selftests.sh`

See the [Language Bindings Guide](docs/bindings.md) for generate/use instructions per language.

### Documentation Accuracy

- Keep claims verifiable from code, tests, or reproducible commands.
- Avoid unconditional guarantees for file size or performance.
- When documenting metrics, include how they were measured.

## Pull Request Process

1. Ensure your code builds and all tests pass
2. Run the benchmark regression gate when touching encoding, decoding, indexing, or file layout logic
3. Keep performance-impacting changes documented (what changed and why)
4. Update documentation as needed
5. Add a clear description of your changes
6. Reference any related issues
7. Wait for review from maintainers

## Code of Conduct

Be respectful and constructive in all interactions.

## Questions?

Open an issue or start a discussion on GitHub.

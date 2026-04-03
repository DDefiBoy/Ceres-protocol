# Contributing to Ceres Protocol

Ceres is open infrastructure. Contributions are welcome and encouraged.

---

## Before You Start

- Check existing issues and PRs before opening a new one
- For large changes, open an issue first to discuss the approach
- All contributions must include tests

---

## Development Setup

```bash
git clone https://github.com/DDefiboy/ceres-protocol.git
cd ceres-protocol

# Contracts
cd contracts/pool
cargo build --target wasm32-unknown-unknown --release
cargo test

# SDK
cd ../../sdk
npm install
npm run build
npm test
```

---

## Pull Request Process

1. Branch from `develop` — not `main`
2. Name your branch clearly: `feature/oracle-twap`, `fix/pool-tick-overflow`
3. Write tests for every change
4. Run `cargo fmt` before committing Rust code
5. Update documentation if you change public interfaces
6. PRs require at least one review before merge

---

## Code Standards

**Rust / Soroban**
- Follow standard Rust formatting (`cargo fmt`)
- No `unwrap()` in production code — use `panic_with_error!`
- Every public function must have a doc comment
- Tests go in `#[cfg(test)]` modules at the bottom of each file

**TypeScript SDK**
- Strict TypeScript — no `any` types
- Every public method must have JSDoc comments
- Tests use Jest

---

## Security

Do not open public issues for security vulnerabilities. Email security@ceres-protocol.xyz directly.

---

## License

By contributing, you agree your contributions are licensed under the MIT License.

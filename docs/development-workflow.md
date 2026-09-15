# Development Workflow

Before opening a change:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Use `cargo build --release --all-features` for the same optimized build used by the start script. Keep generated runtime secrets and subscriptions out of commits.

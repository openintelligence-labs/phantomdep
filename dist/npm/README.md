# phantomdep (npm)

```bash
npm install -g phantomdep
phantomdep doctor
```

This package is a tiny postinstall wrapper that downloads the platform-specific [PhantomDep](https://github.com/openintelligence-labs/phantomdep) binary from the official GitHub release and forwards every command transparently.

For the full docs see the [main README](https://github.com/openintelligence-labs/phantomdep#readme).

## Why a wrapper?

PhantomDep is written in Rust. Distributing it through npm gets the JS-developer audience without forcing them to install Rust. The binary is ~5 MB and works completely offline once installed.

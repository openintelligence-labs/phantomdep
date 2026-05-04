# phantomdep (PyPI)

```bash
pip install phantomdep
phantomdep doctor
```

Thin Python package that bundles the precompiled [PhantomDep](https://github.com/openintelligence-labs/phantomdep) binary so `pip install phantomdep` works for the Python audience without requiring a Rust toolchain.

For the full docs see the [main README](https://github.com/openintelligence-labs/phantomdep#readme).

## How it works

The release pipeline produces one wheel per (os, arch) target. Each wheel ships the matching `phantomdep` binary at `phantomdep/bin/`. The `phantomdep` console script in this package `os.execv`'s into that binary with the user's args.

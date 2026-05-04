# Contributing to PhantomDep

Thanks for considering a contribution. PhantomDep is small, focused, and the kind of project that benefits enormously from outside eyes.

## What we welcome

| Contribution | Where it goes | Notes |
|---|---|---|
| **Phantom-DB additions** (confirmed slop-squats, malicious packages) | `phantom-db/<ecosystem>/<letter>/<name>.json` | See [Phantom-DB schema](phantom-db/SCHEMA.md). Open an issue with the `phantom-db-submission` label first if it's a contested case. |
| **New ecosystem support** (Maven, Ruby gems, NuGet, …) | `crates/phantomdep-core/src/<ecosystem>.rs` + parser | One PR per ecosystem. Mirror the shape of `pypi.rs`/`npm.rs`. |
| **Parser improvements** | `crates/phantomdep-core/src/{py,js,go,cargo}imports.rs` | Add a regression test for any non-trivial change. |
| **IDE / editor integrations** | New top-level dir under `extensions/` | Each integration is a thin wrapper around `phantomdep lsp` or `phantomdep mcp`. |
| **Bug fixes** | Anywhere | Always include a regression test. |
| **Docs improvements** | `README.md`, `docs/`, `phantom-db/pipelines/README.md` | Especially welcome on `BENCHMARK.md` if you spot an unfair comparison. |

## What we don't accept

- **Closed-source dependencies.** Everything PhantomDep ships needs to be redistributable under MIT-compatible terms.
- **Telemetry, in any form.** No phone-home, no analytics, no opt-out toggles. Per the architecture pact, the wire is the wire.
- **Premium-tier features.** Per architecture §3 principle 10, no security feature gets paywalled. Convenience infrastructure (managed mirrors, hosted dashboards) is fine to monetise *separately*; the binary stays free forever.
- **Embargoed Phantom-DB names** (high-frequency unregistered hallucinations). Per architecture §6.5, those are an attacker shopping list and are *deliberately* kept out of the public repo. If you want to help with the embargoed research tier, file an issue and we'll discuss it privately.

## Setup

```bash
git clone https://github.com/openintelligence-labs/phantomdep
cd phantomdep

# Rust (the core binary + LSP + MCP + hook + everything)
cargo build --release --workspace
cargo test --workspace
./target/release/phantomdep doctor

# Python (Phantom-DB pipelines)
cd phantom-db/pipelines
pip install -e '.[dev]'
pytest tests/

# VS Code extension (optional)
cd ../../extensions/vscode
npm install
npx tsc -p .
```

## Pull-request checklist

- [ ] `cargo test --workspace` passes (or `pytest` for pipelines, or `npx tsc` for the extension)
- [ ] New behaviour has at least one regression test
- [ ] No new warnings from `cargo clippy --workspace`
- [ ] CHANGELOG entry under `[Unreleased]`
- [ ] Doc updates for any user-visible flag, command, or output format

## Phantom-DB submissions

The bar for adding a `squatted` or `malicious` entry to the public DB:

1. **A real registry record** that demonstrates the squat (CVE, GHSA, blog post, news article, takedown notice).
2. **A clear `intended_target`** — what the user / LLM probably *meant* to install.
3. **At least one `did_you_mean`** suggestion that's a real, actively-maintained package.
4. **No speculation.** If you can't show evidence, file it as a research note (issue) instead.

For LLM-hallucinated names that **haven't yet been registered**: do not commit them to the public repo. Per architecture §6.5 those are embargoed.

## Security disclosures

See [SECURITY.md](SECURITY.md). TL;DR: don't open a public issue for vulnerabilities — email the address listed there.

## Code of conduct

Be kind, be patient, be precise. We're building infrastructure that other security tools will eventually depend on; the bar is "would I want this tool to be the one my CI relies on at 3am?"

If something feels off, file an issue or DM a maintainer. We'd rather hear about it.

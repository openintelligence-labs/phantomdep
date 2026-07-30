# PhantomDep

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built in Rust](https://img.shields.io/badge/built%20in-rust-orange.svg)](https://www.rust-lang.org/)

PhantomDep is a local-first dependency firewall. It validates every new dependency at the moment it is introduced — AI tool call, IDE edit, shell command, manifest change, PR, or CI — and produces an evidence-backed verdict before the package reaches your machine or repo.

In April 2024, Lasso Security registered an empty Python package called `huggingface-cli` on PyPI. 30,000 developers installed it in three months. Every one of them was told to install it by an AI assistant. PhantomDep is built so that doesn't keep happening.

![phantomdep wrap pip install … blocking a phantom package](./assets/demos/headline.gif)

## How it works

A single 5.5 MB Rust binary that wraps your package managers, hooks into your AI agent, runs as an LSP server in your editor, and gates your CI. Each invocation:

1. Extracts the package name from whichever boundary fired (install command, hook event, source-file diff, manifest edit).
2. Resolves it against the registry (PyPI / npm / crates.io / Go modules) with a local SQLite cache (24h TTL for found packages, 5 min for 404s so freshly registered slop-squats are caught fast).
3. Cross-references against [Phantom-DB](./phantom-db/) — a public, MIT-licensed dataset of confirmed slop-squats and known-malicious entries.
4. Produces a discrete `Verdict` (`PHANTOM`, `SQUATTED`, `KNOWN_MALICIOUS`, `LOOKALIKE`, `REAL`, …) plus a machine-readable evidence bundle. No black-box scoring.
5. Blocks, warns, or allows according to the verdict's default action — overridable per-project via `.phantomdep.toml`.

Warm verdicts run in tens of microseconds; cold verdicts pay one HTTPS round-trip per package per ~24 hours.

## Install

Pick whichever toolchain you already have — all four ship the same static binary:

```bash
brew install openintelligence-labs/tap/phantomdep   # Homebrew (macOS / Linux)
npm  install -g phantomdep                          # npm — downloads the binary for your platform
pip  install phantomdep                             # PyPI — binary ships inside the wheel
cargo install --locked phantomdep                   # crates.io — builds from source
```

Or download the static binary straight from [GitHub Releases](https://github.com/openintelligence-labs/phantomdep/releases):

```bash
arch=$(uname -m | sed 's/^arm64$/aarch64/'); os=$([ "$(uname -s)" = Darwin ] && echo apple-darwin || echo unknown-linux-gnu)
curl -sSfL "https://github.com/openintelligence-labs/phantomdep/releases/latest/download/phantomdep-${arch}-${os}.tar.gz" \
    | tar -xz && sudo install phantomdep /usr/local/bin/
```

No Python or Node runtime required, no signup, no telemetry. The binary works offline against a bundled snapshot.

## Wrap a package manager

```bash
phantomdep wrap pip install requests fastapi
phantomdep wrap npm install react @anthropic-ai/sdk
phantomdep wrap cargo add serde tokio
phantomdep wrap go get github.com/spf13/cobra
```

`pip`, `uv`, `poetry`, `npm`, `pnpm`, `yarn`, `cargo`, and `go` are all supported. If every package is real, PhantomDep transparently `exec`s the underlying command. If anything is `PHANTOM`, `SQUATTED`, `KNOWN_MALICIOUS`, or `LOOKALIKE`, it blocks with exit code 2 — or prompts in interactive shells.

## Scan a repo

![phantomdep scan walking a 4-ecosystem project](./assets/demos/scan-polyglot.gif)

```bash
phantomdep scan .                        # walks .py / .js / .ts / .rs / .go + Cargo.toml + go.mod
phantomdep scan . --format sarif         # GitHub Code Scanning input
phantomdep scan . --format markdown      # PR-comment shaped output
phantomdep scan . --format json
```

A 51-file Python project scans in ~650 ms cold (one HTTPS RTT per unique package, parallelised at concurrency 16) and ~13 ms warm.

## Hook into Claude Code

```bash
phantomdep hook install
```

Writes a `PreToolUse` entry into `~/.claude/settings.json` so every `Bash` install command and every `Write`/`Edit`/`MultiEdit` touching a dependency manifest goes through PhantomDep first. Idempotent and reversible (`phantomdep hook uninstall`).

The hook returns standard Claude Code decision JSON:

```json
{
  "decision": "block",
  "reason": "PhantomDep blocked 1 package(s): huggingface-cli (Phantom) → did you mean: huggingface-hub"
}
```

Exit code 2 + the reason on stderr is fed back to Claude as the block explanation, per the Claude Code hooks spec.

## MCP server

```bash
phantomdep mcp
```

Stdio JSON-RPC, MCP protocol version `2025-06-18`. Exposes four read-only tools: `validate_package`, `validate_imports`, `suggest_real_alternative`, `phantom_db_status`. No shell execution, no filesystem mutation, no registry mutation, deterministic outputs. Wire it into Cursor, Claude Desktop, Continue, Cline, or any MCP client.

## LSP server + IDE extension

```bash
phantomdep lsp
```

LSP 3.17 over stdio. Emits `publishDiagnostics` for hallucinated/squatted/lookalike imports and offers `textDocument/codeAction` quick-fixes that rewrite the import in place.

For a turnkey install, see the [VS Code extension](./extensions/vscode/) — works in VS Code, Cursor, Windsurf, and any other VS Code fork.

## CI

```yaml
# .github/workflows/phantomdep.yml
name: PhantomDep
on: [pull_request, push]
jobs:
  scan:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v4
      - uses: openintelligence-labs/phantomdep@v1
        with:
          fail-on: block
```

Uploads SARIF to GitHub Code Scanning and posts a sticky markdown comment to the PR.

## Verdicts

| Verdict | Meaning | Default action |
|---|---|---|
| `PHANTOM` | Package not found on the registry | `BLOCK` |
| `SQUATTED` | Hallucinated name registered to capture installs | `BLOCK` |
| `KNOWN_MALICIOUS` | Listed in OpenSSF malicious-packages | `BLOCK` |
| `INTERNAL_COLLISION` | Public package collides with a known internal name | `BLOCK` |
| `LOOKALIKE` | Damerau-Levenshtein ≤1 from a popular package | `WARN` |
| `API_MISMATCH` | Package exists but doesn't export the symbols imported | `WARN` |
| `REAL` | Package exists, no high-confidence threat signals | `ALLOW` (with risk score) |
| `UNKNOWN` | Network unreachable, no snapshot, no DB entry | `WARN` |

Verdicts are deterministic and ordered: `PHANTOM > KNOWN_MALICIOUS > SQUATTED > INTERNAL_COLLISION > API_MISMATCH > LOOKALIKE > REAL > UNKNOWN`. The first matching verdict wins; subsequent signals attach to the evidence bundle.

Every finding ships an evidence bundle:

```json
{
  "name": "ccxt-mexc-futures",
  "ecosystem": "pypi",
  "verdict": "KNOWN_MALICIOUS",
  "action": "BLOCK",
  "confidence": 0.98,
  "evidence": [
    { "signal": "phantom_db_hit", "status": "malicious",
      "intended_target": "ccxt", "first_observed": "2025-04-01" }
  ],
  "fixes": [{ "replacement": "ccxt", "confidence": 0.85 }]
}
```

Defaults are configurable per-project via `.phantomdep.toml`:

```toml
[verdicts]
phantom = "block"
squatted = "block"
lookalike = "warn"
unknown = "warn"

[ci]
fail_on_verdict = ["phantom", "squatted", "known_malicious"]

[allowlist]
packages = ["my-internal-fresh-pkg@*"]
```

## Performance

```
$ phantomdep benchmark --iterations 100
PhantomDep benchmark (100 iterations, warm caches)

  scenario                       p50 (μs)   p95 (μs)   max (μs)
  ------------------------------------------------------------
  real PyPI (cached)                   17         22         39
  real npm (cached)                     9         12         16
  phantom PyPI (cached)                30         35         50
  offline resolve only                 16         18         19
```

![phantomdep replay + benchmark](./assets/demos/replay.gif)

Reproducible: `phantomdep benchmark --iterations 100 --json`. Numbers above are from a release build on Apple Silicon, residential broadband. Cold network verdicts are dominated by the registry HTTPS round-trip (typically 10–30 ms for warm TLS, occasional outliers up to ~350 ms).

## Supported ecosystems

| Ecosystem | Registry | Manifest | Source-file imports |
|---|---|---|---|
| PyPI | `pypi.org/pypi/{name}/json` | `requirements.txt`, `pyproject.toml` | `.py`, `.pyi` |
| npm | `registry.npmjs.org/{name}` | (planned) | `.js`, `.ts`, `.jsx`, `.tsx`, `.mjs`, `.cjs` |
| crates.io | `crates.io/api/v1/crates/{name}` | `Cargo.toml` | (planned) |
| Go modules | `proxy.golang.org/{module}/@v/list` | `go.mod` | `.go` |
| Maven | (planned) | (planned) | (planned) |

Cross-ecosystem fallback via [deps.dev](https://docs.deps.dev/api/v3/) for unified metadata.

## How it relates to existing tools

| Tool | Catches what PhantomDep catches? |
|---|---|
| Snyk, OSV-Scanner, Dependabot | No — they look up CVEs in real packages. A hallucinated or squatted name has no CVE. |
| Socket | Partially — behavioral risk fires after the bad name is in `package.json`. PhantomDep fires before. |
| Phylum (Veracode) | Partially — strong on known-malicious, but the free dev tier didn't survive the acquisition. |
| Sonatype Guide | Closest competitor for the AI-assistant intercept use case. Closed-source, service-backed, no offline mode, no transparent verdict engine. |
| Chainguard + Cursor | Curated registry inside Cursor only. Replaces the registry rather than validating against it. |
| Datadog `scfw` | Install-time blocking for known-malicious npm/PyPI. PhantomDep adds agent-time, hallucination/squat detection, and Cargo + Go. |

PhantomDep does **not** replace CVE scanners. Pair it with [OSV-Scanner](https://github.com/google/osv-scanner) or Dependabot for known vulnerabilities; PhantomDep handles the introduction-time gate they all miss.

## Phantom-DB

Confirmed slop-squats and known-malicious entries live in [`phantom-db/`](./phantom-db/) as JSON files, MIT-licensed. Schema in [`phantom-db/SCHEMA.md`](./phantom-db/SCHEMA.md). The dataset is public so any tool can sync from it.

A representative entry:

```json
{
  "name": "huggingface-cli",
  "ecosystem": "pypi",
  "status": "squatted",
  "first_observed": "2024-02-13",
  "intended_target": "huggingface-hub[cli]",
  "did_you_mean": ["huggingface-hub"],
  "evidence_url": "https://www.lasso.security/blog/ai-package-hallucinations"
}
```

Unregistered high-frequency hallucinations are deliberately not committed here — that would amount to publishing an attacker's shopping list. They live in an embargoed research tier and are released only as aggregate statistics, or once an attacker has actually registered the name.

The dataset grows automatically: a verifier workflow runs hourly to promote `phantom` → `squatted` when an attacker registers a hallucinated name; a feeder workflow runs nightly to surface candidate phantoms for human review.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Phantom-DB additions, registry checkers, parsers, and IDE/editor integrations are all welcome. Confirmed-malicious package reports go through GitHub Issues with the `phantom-db-submission` label.

## Security

See [SECURITY.md](./SECURITY.md). Don't open public issues for security bugs.

## License

MIT. See [LICENSE](./LICENSE).

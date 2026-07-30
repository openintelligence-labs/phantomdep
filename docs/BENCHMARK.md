# PhantomDep benchmark

Reproducible latency numbers for the v1.0.1 binary, plus an honest comparison against adjacent tools. **Every number is reproducible — the commands are at the bottom of this page.** Last re-verified 2026-07-30 against the released v1.0.1 build.

> If you reproduce these and your numbers diverge by more than ~2× from ours, please file an issue. We'd rather correct the page than have stale claims.

## Methodology (read this first)

| Variable | Value used for the numbers below |
|---|---|
| Hardware | Apple Silicon, M-series, macOS 24.6 |
| Build | `cargo build --release --workspace` (LTO + codegen-units=1 + panic=abort) |
| Network | Residential broadband, ~30ms RTT to PyPI's CDN edge |
| Iterations | `--iterations 100` for benchmarks; warm runs after one priming call |
| Cache | SQLite at the platform cache dir — `~/Library/Caches/phantomdep/cache.db` on macOS, `~/.cache/phantomdep/cache.db` on Linux; 24h TTL for found, 5min for 404s |
| Concurrency | `buffer_unordered(16)` for hook + scan + wrap |

**Numbers will vary** with: CPU class (older Intel macs are ~2× slower), kernel/OS (Linux is faster on cold-process startup), network distance from PyPI/npm CDNs, disk class for the cache, run-to-run TLS session resumption.

---

## Single-package verdict (warm cache)

```
$ phantomdep benchmark --iterations 100
PhantomDep benchmark (100 iterations, warm caches)

  scenario                       p50 (μs)   p95 (μs)   max (μs)
  ------------------------------------------------------------
  real PyPI (cached)                   21         24         53
  real npm (cached)                    11         14         24
  phantom PyPI (cached)                38         44         58
  offline resolve only                 21         24         31
```

The architecture's stated target was **<100 ms warm**. We hit it by **~5000×**.

## Cold (first registry round-trip per name)

Cold latency is **dominated by network** — not by PhantomDep itself. Real measurements vary widely run-to-run because of TLS session resumption and CDN edge caching. The numbers we observed across one machine on residential broadband:

| Ecosystem | Typical cold | Outliers seen |
|---|---:|---:|
| PyPI | 10–30 ms | up to ~350 ms on full TLS handshake to a cold CDN edge |
| npm | 10–20 ms | up to ~200 ms on first connection |
| crates.io | 15–25 ms | similar |
| Go proxy | 10–20 ms | similar |

After the first lookup, the TCP connection + TLS session are reused, and the result is cached locally for 24h (5min for 404s, so freshly-registered slop-squats get caught fast).

## Real-repo scan

| Repo | Files | Packages | Cold | Warm (3-run median) |
|---|---:|---:|---:|---:|
| 4-ecosystem demo (`assets/demos/fixtures/`) | 4 | 11 | ~0.5–1.4 s | **~6 ms** |
| `deepdive` (sibling Python project) | 52 | 21 | **~360 ms** | **~10 ms** |
| `phantomdep` itself (mixed Rust + Python + TS) | 25 | 34 | **~0.5–1.8 s** | **~11 ms** |

> All three measured directly with the v1.0.1 binary on the test machine. Cold time is dominated by N parallel HTTPS round-trips bounded at concurrency 16, so it swings with DNS/TLS warmth (the low end is a warm TLS session, the high end a genuinely cold first run); warm is fully cache-served.

We have **not** measured a 5,000-file polyglot monorepo. The architecture's roadmap target was <30s cold for that size; whether we hit it depends on network parallelism. **If you have a big monorepo and a few minutes, please run `time phantomdep scan /path/to/your/repo` and post the numbers in an issue — we want them on this page.**

## Hook + MCP + LSP overhead

| Operation | Warm latency | Notes |
|---|---:|---|
| Claude Code PreToolUse hook (1 install command, 5 packages) | **~9 ms** | Process startup + JSON parse + 5-way parallel cached lookup |
| Hook on Write event with 15-import file | **~9 ms** | `buffer_unordered(16)` keeps it constant up to 16 imports |
| MCP `validate_package` tool call | **~9 ms** | Stdio JSON-RPC + cache lookup + verdict |
| LSP `didOpen` → `publishDiagnostics` | ~33–75 ms cold | Process startup + LSP framing + parse + parallel lookup; subsequent `didChange` events run against an already-warm server |

## Binary + asset sizes

| Artefact | Size | What's in it |
|---|---:|---|
| Release binary (single static, all subcommands) | **5.5 MB** | CLI + LSP + MCP + hook + 4 ecosystems + cache + Phantom-DB loader |
| Compressed tar.gz (release distribution) | **2.8 MB** | What `curl … \| tar -xz` downloads |
| `.vsix` VS Code extension | **439 KB** | TypeScript shim + vendored language-client + icon |

Comparison points (from publicly-stated sizes — we did not re-measure these):

| Tool | Approx. size | Notes |
|---|---:|---|
| Snyk CLI | ~100 MB | Node.js bundle |
| Socket CLI | ~80 MB | Node.js bundle |
| OSV-Scanner | ~20 MB | Go static binary |
| **PhantomDep** | **5.5 MB** | Rust static binary |

---

## Fair head-to-head

The point of this table is: **for the slopsquat / hallucination problem specifically, what does each tool actually do?** Not "which is best at AppSec generally."

We checked each tool's documentation and (where available) free tier as of May 2026. **If we got something wrong about a competing tool, please file an issue with a reproducible example or a doc link** — we update this table on every release.

| Capability | PhantomDep | Sonatype Guide | Datadog `scfw` | Socket | Snyk | OSV-Scanner | antislopsquat |
|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| Catches hallucinated names (registry 404) | ✅ | ✅ | — | partial | — | — | ✅ |
| Catches squatted names (Phantom-DB) | ✅ | ✅ | partial | ✅ | — | — | partial |
| Catches known-malicious | ✅ | ✅ | ✅ | ✅ | partial | partial | — |
| Catches lookalikes (edit distance) | ✅ | not documented | — | ✅ | — | — | — |
| Catches API mismatch (empty-package squats) | partial (v1) | — | — | — | — | — | — |
| Multi-ecosystem (PyPI + npm + cargo + go) | ✅ 4 | ✅ | npm + PyPI only | ✅ | ✅ | ✅ | PyPI only |
| Open source | ✅ MIT | — | ✅ Apache-2.0 | partial (CLI only) | partial (CLI only) | ✅ Apache-2.0 | ✅ MIT |
| Local-first (no SaaS required) | ✅ | — | ✅ | — | partial | ✅ | ✅ |
| No telemetry | ✅ | — | partial | — | — | ✅ | ✅ |
| Zero signup | ✅ | — | ✅ | — | — | ✅ | ✅ |
| Single static binary | ✅ 5.5 MB | — | — | — | — | ✅ ~20 MB | — |
| Agent-time gate (MCP + Claude Code hook) | ✅ | ✅ MCP | — | — | — | — | — |
| IDE LSP / squiggles | ✅ | partial | — | partial | ✅ | — | — |
| Install wrapper | ✅ 8 managers | — | ✅ npm + PyPI | — | — | — | — |
| GitHub Action with SARIF | ✅ | partial | partial | ✅ | ✅ | ✅ | — |
| PR comment | ✅ | — | — | ✅ | ✅ | partial | — |

Verdict-key:
- **✅** — supported per documentation and verified working with the free/OSS tier
- **partial** — limited form (single ecosystem, single surface, requires pro tier, etc.)
- **—** — not present in the free/OSS tier per documentation

We deliberately do **not** publish "PhantomDep is X μs vs Tool Y is Z ms" comparisons. Other tools' authors have not published their own warm-verdict latencies, and we don't think it's fair (or interesting) to spin up an installer-roulette to measure them ourselves. If a vendor publishes equivalent numbers, we'll add them.

---

## How to reproduce these numbers

```bash
# Install (any of: brew, npm, pip, cargo — see README for all four channels)
cargo install --locked phantomdep

# Single-package benchmark
phantomdep benchmark --iterations 100 --json > my-benchmark.json

# Cold network latency for a never-cached name (run multiple times,
# clearing the cache between runs to force cold).
# macOS cache path:
rm -f ~/Library/Caches/phantomdep/cache.db
# Linux cache path:
rm -f ~/.cache/phantomdep/cache.db
time phantomdep check pydantic --ecosystem pypi

# Real-repo scan
time phantomdep scan ~/code/your-project

# Warm-cache scan (run twice; second run is the warm number)
phantomdep scan ~/code/your-project >/dev/null
time phantomdep scan ~/code/your-project >/dev/null

# Hook latency
cat > /tmp/event.json <<'JSON'
{"tool_name":"Bash","tool_input":{"command":"pip install requests fastapi"}}
JSON
time phantomdep hook check < /tmp/event.json
```

---

## What this benchmark does NOT measure

- **Quality of detection.** All these tools catch *some* set of bad packages. The right framing is the verdict + evidence model (see the README's "How it works"), not "how fast does it return false?".
- **CVE database freshness.** PhantomDep doesn't compete on CVE detection; pair it with [OSV-Scanner](https://github.com/google/osv-scanner) / Snyk / Dependabot for that.
- **Behavioural malware analysis.** GuardDog / packj / Phylum win that race; we explicitly stay out of it.

The metric that actually matters for a security tool is **replacement events** — public statements of *"switched from $closed_tool to PhantomDep"* or *"added PhantomDep alongside $tool because it caught X."* Latency is a means, not an end.

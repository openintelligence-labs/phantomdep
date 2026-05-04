# Phantom-DB pipelines

Nightly tooling that grows the public Phantom-DB. Two scripts:

- **`phantomdep-feeder`** — runs Spracklen-style prompts against open-weight LLMs (default: Ollama, optional cloud), parses the suggested imports, and detects which ones are **404 on the registry today** (= candidate hallucinations).
- **`phantomdep-verifier`** — re-checks every existing Phantom-DB entry. Promotes `phantom` → `squatted` when an attacker has registered the name; flags entries where the registered package now exists with the suspect status preserved.

## Responsible disclosure (architecture §6.5)

The pipeline never commits unregistered high-frequency phantoms directly. Output goes to two tiers:

| Tier | Contents | Where it lives |
|---|---|---|
| **Public** | Confirmed `squatted` and `malicious` entries; hallucinations already discussed in research/news | `phantom-db/<ecosystem>/<letter>/<name>.json` (this repo) |
| **Embargoed** | High-frequency unregistered phantom names; per-model rates; raw prompt corpus | aggregate stats only — no leaderboard of names |

Aggregate report (frequencies by model/ecosystem/month): `reports/state-of-hallucination-YYYY-MM.json`.

## Install

```bash
cd phantom-db/pipelines
pip install -e '.[dev]'
```

## Use

```bash
# Run the feeder against local Ollama (default model: codellama:7b).
phantomdep-feeder --provider ollama --model codellama:7b --output candidates/

# Dry-run with canned LLM output (no Ollama needed).
phantomdep-feeder --provider mock --output /tmp/dry-run/

# Verifier: re-check every entry in ../  (the public Phantom-DB).
phantomdep-verifier --db ../

# Just lint changes (CI mode).
phantomdep-verifier --db ../ --check-only
```

## Pipeline flow

```
prompt corpus  →  LLM (Ollama / mock)  →  parser  →  registry check  →  candidate JSON
                                                          |
                                                          ↓
                                                   confirmed 404s only
                                                          |
                                                          ↓
                                                  diff vs existing DB
                                                          |
                                                          ↓
                                                  PR with new entries
```

## CI cadence

`.github/workflows/phantomdb-nightly.yml` runs:

- **Verifier**: every hour. Auto-commits status promotions.
- **Feeder**: nightly with `--provider mock` baseline (no LLM cost). Open-weight runs are optional and require self-hosted runners.

## License

MIT.

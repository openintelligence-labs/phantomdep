# Phantom-DB schema (v1)

Each entry is a single JSON file at `phantom-db/<ecosystem>/<first-letter>/<name>.json`.

`name` is the lowercased package name as used in the registry. `<first-letter>` is the lowercase first character of `name`.

## Fields

| Field | Type | Required | Notes |
|---|---|---|---|
| `name` | string | yes | Exact registry name |
| `ecosystem` | enum | yes | `pypi`, `npm`, `cargo`, `go`, `maven` |
| `status` | enum | yes | `phantom` (LLM-hallucinated, not yet registered), `squatted` (registered to capture LLM hallucinations), `malicious` (confirmed malicious payload) |
| `first_observed` | string (YYYY-MM-DD) | no | When this hallucination was first reported |
| `intended_target` | string | no | What the user/LLM probably *meant*, e.g. `huggingface_hub[cli]` |
| `did_you_mean` | array of string | no | Suggested safe replacements, ordered by confidence |
| `evidence_url` | string | no | Public link to research/news/CVE that documents this entry |
| `models_observed` | array of object | no | `{model, rate, runs}` from probe data |
| `status_log` | array of object | no | `{date, from, to, reason}` transitions appended by the verifier; never hand-edit |
| `notes` | string | no | Free-text context (e.g. defensive registration by a researcher) |

## Example

```json
{
  "name": "huggingface-cli",
  "ecosystem": "pypi",
  "status": "squatted",
  "first_observed": "2024-02-13",
  "intended_target": "huggingface_hub[cli]",
  "did_you_mean": ["huggingface_hub", "huggingface-hub"],
  "evidence_url": "https://www.lasso.security/blog/ai-package-hallucinations"
}
```

## Responsible disclosure

Per architecture §6.5: only `squatted`, `malicious`, and publicly-disclosed `phantom` entries belong here. **Do not commit unregistered high-frequency hallucinated names** — those are an attacker shopping list and live in the embargoed research tier.

"""Phantom-DB feeder.

Pipeline:

  prompts → LLM (provider) → extract candidate package names → existence
  check against registry → keep only 404s → diff against existing DB → emit
  candidate JSON files to an output directory (ready for PR review).

Per architecture §6.5, *unregistered* high-frequency phantoms do NOT get
auto-promoted to public Phantom-DB entries. The feeder writes them to a
candidates/ directory; a human (or, eventually, the verifier bot once the
name has been registered) decides what to publish.

The feeder also writes an aggregate report — frequency by ecosystem and
prompt — that *is* safe to publish because it contains no leaderboard of
unregistered names.
"""

from __future__ import annotations

import asyncio
import datetime as dt
import json
from collections import defaultdict
from pathlib import Path
from typing import Any

import click

from .corpus import ALL_PROMPTS
from .parsers import extract_packages, normalize_pypi_name
from .providers import get_provider
from .registries import check_existence


def run_feeder(
    *,
    provider_name: str,
    model: str | None,
    languages: list[str],
    output_dir: Path,
    db_dir: Path,
) -> dict[str, Any]:
    """Run the feeder end-to-end. Returns an aggregate report."""

    return asyncio.run(
        _run_async(
            provider_name=provider_name,
            model=model,
            languages=languages,
            output_dir=output_dir,
            db_dir=db_dir,
        )
    )


async def _run_async(
    *,
    provider_name: str,
    model: str | None,
    languages: list[str],
    output_dir: Path,
    db_dir: Path,
) -> dict[str, Any]:
    provider = get_provider(provider_name, model)
    prompts_to_run = [p for p in ALL_PROMPTS if p.language in languages]

    responses = await provider.generate([(p.id, p.text) for p in prompts_to_run])

    # Aggregate: per-ecosystem candidate names + their source prompts.
    candidates: dict[str, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))
    for resp in responses:
        prompt = next((p for p in prompts_to_run if p.id == resp.prompt_id), None)
        if prompt is None:
            continue
        ecosystem = "pypi" if prompt.language == "python" else "npm"
        names = extract_packages(resp.text, prompt.language)
        for n in names:
            candidates[ecosystem][n].append(resp.prompt_id)

    # Existence check: only 404s become candidate phantom entries.
    confirmed_phantoms: dict[str, dict[str, list[str]]] = {}
    for ecosystem, names_to_prompts in candidates.items():
        if not names_to_prompts:
            continue
        existence = await check_existence(list(names_to_prompts.keys()), ecosystem)
        confirmed_phantoms[ecosystem] = {
            n: prompts for n, prompts in names_to_prompts.items() if not existence.get(n, True)
        }

    # Diff against existing DB. Skip candidates already on disk.
    new_candidates = _diff_against_db(confirmed_phantoms, db_dir)

    # Write candidate JSON files to output_dir/<ecosystem>/<letter>/<name>.json.
    output_dir.mkdir(parents=True, exist_ok=True)
    new_files: list[Path] = []
    for ecosystem, names in new_candidates.items():
        for name, source_prompts in names.items():
            path = _candidate_path(output_dir, ecosystem, name)
            path.parent.mkdir(parents=True, exist_ok=True)
            entry = {
                "name": name,
                "ecosystem": ecosystem,
                "status": "phantom",
                "first_observed": dt.date.today().isoformat(),
                "intended_target": None,
                "did_you_mean": [],
                "evidence_url": None,
                "models_observed": [
                    {"model": provider.model, "rate": None, "runs": len(source_prompts)}
                ],
                "_source_prompts": source_prompts,
            }
            path.write_text(json.dumps(entry, indent=2, sort_keys=True) + "\n")
            new_files.append(path)

    # Aggregate report (safe to publish — no leaderboard).
    report = _aggregate_report(
        provider=provider.name,
        model=provider.model,
        prompts_run=len(prompts_to_run),
        responses_received=len(responses),
        candidates=new_candidates,
        new_files=new_files,
    )
    return report


def _diff_against_db(
    candidates: dict[str, dict[str, list[str]]], db_dir: Path
) -> dict[str, dict[str, list[str]]]:
    """Skip candidates whose canonical name is already in the public DB.

    For PyPI we apply PEP 503 normalisation on both sides so
    `Super_Fast_Parser` and `super-fast-parser` are recognised as the same
    project.
    """
    existing_names = _load_existing_names(db_dir)
    new: dict[str, dict[str, list[str]]] = {}
    for ecosystem, names in candidates.items():
        canonical_existing = existing_names.get(ecosystem, set())
        for name, prompts in names.items():
            canonical = _canonicalise(name, ecosystem)
            if canonical in canonical_existing:
                continue
            new.setdefault(ecosystem, {})[name] = prompts
    return new


def _load_existing_names(db_dir: Path) -> dict[str, set[str]]:
    """Walk the existing DB and return the set of canonical names per ecosystem."""
    out: dict[str, set[str]] = {}
    if not db_dir.exists():
        return out
    for ecosystem_dir in db_dir.iterdir():
        if not ecosystem_dir.is_dir():
            continue
        eco = ecosystem_dir.name
        names: set[str] = set()
        for letter_dir in ecosystem_dir.iterdir():
            if not letter_dir.is_dir():
                continue
            for path in letter_dir.glob("*.json"):
                names.add(_canonicalise(path.stem, eco))
        out[eco] = names
    return out


def _canonicalise(name: str, ecosystem: str) -> str:
    if ecosystem == "pypi":
        return normalize_pypi_name(name)
    return name.lower()


def _candidate_path(output_dir: Path, ecosystem: str, name: str) -> Path:
    safe = name.replace("/", "_")
    return output_dir / ecosystem / safe[:1].lower() / f"{safe}.json"


def _aggregate_report(
    *,
    provider: str,
    model: str,
    prompts_run: int,
    responses_received: int,
    candidates: dict[str, dict[str, list[str]]],
    new_files: list[Path],
) -> dict[str, Any]:
    by_eco_count = {eco: len(names) for eco, names in candidates.items()}
    return {
        "generated_at": dt.datetime.now(dt.UTC).isoformat(),
        "provider": provider,
        "model": model,
        "prompts_run": prompts_run,
        "responses_received": responses_received,
        "candidate_phantoms_by_ecosystem": by_eco_count,
        "new_candidate_files": [str(f) for f in new_files],
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


@click.command()
@click.option(
    "--provider",
    type=click.Choice(["ollama", "mock"]),
    default="mock",
    help="LLM provider to use. `mock` is deterministic and free; `ollama` requires a local server.",
)
@click.option(
    "--model",
    default=None,
    help="Model name (provider-specific). Defaults to a sensible value per provider.",
)
@click.option(
    "--language",
    "languages",
    multiple=True,
    type=click.Choice(["python", "javascript"]),
    default=("python", "javascript"),
    help="Languages to probe. Repeat to enable multiple.",
)
@click.option(
    "--output",
    "output_dir",
    type=click.Path(file_okay=False, path_type=Path),
    default=Path("candidates"),
    help="Directory to write candidate JSON files into.",
)
@click.option(
    "--db",
    "db_dir",
    type=click.Path(file_okay=False, exists=True, path_type=Path),
    required=True,
    help="Path to the existing public Phantom-DB (used to skip already-known entries).",
)
@click.option(
    "--report",
    "report_path",
    type=click.Path(dir_okay=False, path_type=Path),
    default=None,
    help="Where to write the aggregate JSON report.",
)
def main(
    provider: str,
    model: str | None,
    languages: tuple[str, ...],
    output_dir: Path,
    db_dir: Path,
    report_path: Path | None,
) -> None:
    """Run one feeder pass."""
    report = run_feeder(
        provider_name=provider,
        model=model,
        languages=list(languages),
        output_dir=output_dir,
        db_dir=db_dir,
    )
    click.echo(json.dumps(report, indent=2))
    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()

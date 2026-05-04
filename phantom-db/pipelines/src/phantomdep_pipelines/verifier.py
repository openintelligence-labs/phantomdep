"""Phantom-DB verifier.

Walks every JSON entry under `phantom-db/<ecosystem>/<letter>/<name>.json`,
re-checks each against the live registry, and emits status transitions:

  - `phantom`   → `squatted`   if the registry now returns 200 (= someone
                                registered the previously-unregistered name)
  - `squatted`  → `phantom`    if the registry returns 404 (= the squatter
                                got taken down). Reasonably rare but happens.
  - `malicious` stays `malicious` regardless of registry state — the entry is
                                a permanent record of the incident.

The verifier *never* changes a `did_you_mean` or `intended_target` field; it
only updates `status` (and appends to a transitions log). All edits happen
in-place under `--db`. With `--check-only` it exits non-zero on pending
transitions without mutating files (CI lint mode).
"""

from __future__ import annotations

import asyncio
import datetime as dt
import json
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import click

from .registries import check_existence


@dataclass(frozen=True)
class Entry:
    path: Path
    name: str
    ecosystem: str
    status: str
    raw: dict[str, Any]


@dataclass(frozen=True)
class Transition:
    name: str
    ecosystem: str
    from_status: str
    to_status: str
    reason: str


def load_entries(db_dir: Path) -> list[Entry]:
    out: list[Entry] = []
    if not db_dir.exists():
        return out
    for ecosystem_dir in sorted(db_dir.iterdir()):
        if not ecosystem_dir.is_dir():
            continue
        for letter_dir in sorted(ecosystem_dir.iterdir()):
            if not letter_dir.is_dir():
                continue
            for path in sorted(letter_dir.glob("*.json")):
                try:
                    raw = json.loads(path.read_text())
                except (json.JSONDecodeError, OSError):
                    continue
                if not isinstance(raw, dict):
                    continue
                out.append(
                    Entry(
                        path=path,
                        name=str(raw.get("name", "")),
                        ecosystem=str(raw.get("ecosystem", "")),
                        status=str(raw.get("status", "")),
                        raw=raw,
                    )
                )
    return out


async def _check_all(
    entries: Iterable[Entry], *, concurrency: int = 16
) -> dict[tuple[str, str], bool]:
    by_eco: dict[str, list[str]] = {}
    for e in entries:
        by_eco.setdefault(e.ecosystem, []).append(e.name)

    results: dict[tuple[str, str], bool] = {}
    for eco, names in by_eco.items():
        if eco not in {"pypi", "npm"}:
            # Verifier only knows how to check PyPI + npm right now.
            for n in names:
                results[(eco, n)] = True  # assume exists; don't promote unknown ecosystems
            continue
        existence = await check_existence(names, eco, concurrency=concurrency)
        for n, exists in existence.items():
            results[(eco, n)] = exists
    return results


def compute_transitions(
    entries: list[Entry], existence: dict[tuple[str, str], bool]
) -> list[Transition]:
    transitions: list[Transition] = []
    for e in entries:
        exists = existence.get((e.ecosystem, e.name))
        if exists is None:
            continue
        if e.status == "phantom" and exists:
            transitions.append(
                Transition(
                    name=e.name,
                    ecosystem=e.ecosystem,
                    from_status="phantom",
                    to_status="squatted",
                    reason="registry now returns 200 — name was registered after first observation",
                )
            )
        elif e.status == "squatted" and not exists:
            transitions.append(
                Transition(
                    name=e.name,
                    ecosystem=e.ecosystem,
                    from_status="squatted",
                    to_status="phantom",
                    reason="registry returns 404 — squatter takedown or removal",
                )
            )
        # malicious entries are sticky: we never demote them.
    return transitions


def apply_transitions(
    entries: list[Entry], transitions: list[Transition]
) -> dict[Path, dict[str, Any]]:
    by_key = {(e.ecosystem, e.name): e for e in entries}
    updated: dict[Path, dict[str, Any]] = {}
    today = dt.date.today().isoformat()
    for t in transitions:
        e = by_key[(t.ecosystem, t.name)]
        new = dict(e.raw)
        new["status"] = t.to_status
        log = new.get("status_log", [])
        if not isinstance(log, list):
            log = []
        log.append(
            {
                "date": today,
                "from": t.from_status,
                "to": t.to_status,
                "reason": t.reason,
            }
        )
        new["status_log"] = log
        updated[e.path] = new
    return updated


def write_transitions(updates: dict[Path, dict[str, Any]]) -> None:
    for path, payload in updates.items():
        path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


@click.command()
@click.option(
    "--db",
    "db_dir",
    type=click.Path(file_okay=False, exists=True, path_type=Path),
    required=True,
    help="Path to the public Phantom-DB.",
)
@click.option(
    "--check-only",
    is_flag=True,
    help="Don't write changes; exit 1 if any transitions would happen.",
)
@click.option(
    "--report",
    "report_path",
    type=click.Path(dir_okay=False, path_type=Path),
    default=None,
    help="Where to write the JSON transitions log.",
)
@click.option(
    "--concurrency",
    type=int,
    default=16,
    help="Maximum concurrent registry checks.",
)
def main(
    db_dir: Path,
    check_only: bool,
    report_path: Path | None,
    concurrency: int,
) -> None:
    """Re-check every Phantom-DB entry against the live registry."""

    entries = load_entries(db_dir)
    existence = asyncio.run(_check_all(entries, concurrency=concurrency))
    transitions = compute_transitions(entries, existence)

    summary = {
        "checked_at": dt.datetime.now(dt.UTC).isoformat(),
        "entries_checked": len(entries),
        "transitions": [t.__dict__ for t in transitions],
    }
    click.echo(json.dumps(summary, indent=2))
    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(summary, indent=2) + "\n")

    if not transitions:
        return

    if check_only:
        raise SystemExit(1)

    updates = apply_transitions(entries, transitions)
    write_transitions(updates)


if __name__ == "__main__":
    main()

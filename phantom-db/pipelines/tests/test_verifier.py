import json
from pathlib import Path

from phantomdep_pipelines.verifier import (
    Entry,
    apply_transitions,
    compute_transitions,
    load_entries,
)


def _write_entry(tmp_path: Path, eco: str, name: str, status: str) -> Path:
    p = tmp_path / eco / name[:1].lower() / f"{name}.json"
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(
        json.dumps({"name": name, "ecosystem": eco, "status": status}, indent=2)
    )
    return p


def test_load_entries_reads_nested_layout(tmp_path: Path) -> None:
    _write_entry(tmp_path, "pypi", "huggingface-cli", "phantom")
    _write_entry(tmp_path, "npm", "react-codeshift", "squatted")
    entries = load_entries(tmp_path)
    names = sorted(e.name for e in entries)
    assert names == ["huggingface-cli", "react-codeshift"]


def test_phantom_to_squatted_when_now_exists() -> None:
    entries = [
        Entry(Path("/tmp/x.json"), "huggingface-cli", "pypi", "phantom",
              {"name": "huggingface-cli", "ecosystem": "pypi", "status": "phantom"})
    ]
    existence = {("pypi", "huggingface-cli"): True}
    transitions = compute_transitions(entries, existence)
    assert len(transitions) == 1
    assert transitions[0].from_status == "phantom"
    assert transitions[0].to_status == "squatted"


def test_squatted_to_phantom_when_taken_down() -> None:
    entries = [
        Entry(Path("/tmp/x.json"), "huggingface-cli", "pypi", "squatted",
              {"name": "huggingface-cli", "ecosystem": "pypi", "status": "squatted"})
    ]
    existence = {("pypi", "huggingface-cli"): False}
    transitions = compute_transitions(entries, existence)
    assert len(transitions) == 1
    assert transitions[0].to_status == "phantom"


def test_malicious_is_sticky() -> None:
    entries = [
        Entry(Path("/tmp/x.json"), "ccxt-mexc-futures", "pypi", "malicious",
              {"name": "ccxt-mexc-futures", "ecosystem": "pypi", "status": "malicious"})
    ]
    # Even if the registry now reports it as gone, we keep the malicious record.
    transitions = compute_transitions(entries, {("pypi", "ccxt-mexc-futures"): False})
    assert transitions == []


def test_apply_transitions_appends_log(tmp_path: Path) -> None:
    p = _write_entry(tmp_path, "pypi", "huggingface-cli", "phantom")
    entries = load_entries(tmp_path)
    transitions = compute_transitions(
        entries, {("pypi", "huggingface-cli"): True}
    )
    updates = apply_transitions(entries, transitions)
    assert p in updates
    payload = updates[p]
    assert payload["status"] == "squatted"
    assert isinstance(payload["status_log"], list)
    assert payload["status_log"][0]["from"] == "phantom"
    assert payload["status_log"][0]["to"] == "squatted"

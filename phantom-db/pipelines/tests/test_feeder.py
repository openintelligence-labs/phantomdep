"""Dry-run feeder test using the mock provider — no network, no LLM."""
import json
from pathlib import Path
from unittest.mock import patch

import pytest

from phantomdep_pipelines.feeder import run_feeder


@pytest.fixture
def fake_db(tmp_path: Path) -> Path:
    """Empty Phantom-DB layout — every candidate looks new."""
    (tmp_path / "pypi").mkdir()
    (tmp_path / "npm").mkdir()
    return tmp_path


def test_mock_feeder_produces_candidates_for_known_phantoms(fake_db: Path, tmp_path: Path) -> None:
    """The mock provider deliberately mentions ccxt-mexc-futures (malicious),
    huggingface-cli (squatted), super-fast-json-parser (phantom),
    aioredis_typed (phantom), react-codeshift (npm squatted), and
    @hallucinated/vector-store-local (npm phantom). All of these should land
    in the candidates directory because the test patches PyPI/npm to report
    them as 404."""
    output_dir = tmp_path / "candidates"

    async def fake_check(names, ecosystem, *, concurrency=16):
        # Real packages are real; everything else is a phantom (404).
        real = {
            "pypi": {"pypdf", "redis", "pyyaml", "ujson"},
            "npm": {"@anthropic-ai/sdk"},
        }[ecosystem]
        return {n: (n in real) for n in names}

    with patch("phantomdep_pipelines.feeder.check_existence", new=fake_check):
        report = run_feeder(
            provider_name="mock",
            model="mock-7b",
            languages=["python", "javascript"],
            output_dir=output_dir,
            db_dir=fake_db,
        )

    pypi_count = report["candidate_phantoms_by_ecosystem"].get("pypi", 0)
    npm_count = report["candidate_phantoms_by_ecosystem"].get("npm", 0)
    assert pypi_count >= 3, f"expected at least 3 pypi candidates, got {pypi_count}"
    assert npm_count >= 2, f"expected at least 2 npm candidates, got {npm_count}"
    assert len(report["new_candidate_files"]) == pypi_count + npm_count


def test_existing_db_entry_is_skipped(fake_db: Path, tmp_path: Path) -> None:
    """If huggingface-cli is already in the DB, the feeder should not re-emit it
    even when the LLM emits the underscore variant `huggingface_cli`."""
    output_dir = tmp_path / "candidates"
    (fake_db / "pypi" / "h").mkdir(parents=True, exist_ok=True)
    (fake_db / "pypi" / "h" / "huggingface-cli.json").write_text(
        json.dumps({"name": "huggingface-cli", "ecosystem": "pypi", "status": "squatted"})
    )

    async def fake_check(names, ecosystem, *, concurrency=16):
        return dict.fromkeys(names, False)  # everything is a 404

    with patch("phantomdep_pipelines.feeder.check_existence", new=fake_check):
        report = run_feeder(
            provider_name="mock",
            model="mock-7b",
            languages=["python"],
            output_dir=output_dir,
            db_dir=fake_db,
        )

    candidate_files = [Path(p) for p in report["new_candidate_files"]]
    candidate_names = {p.stem for p in candidate_files}
    # Both hyphen and underscore forms should be skipped.
    assert "huggingface-cli" not in candidate_names
    assert "huggingface_cli" not in candidate_names

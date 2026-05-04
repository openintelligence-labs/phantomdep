"""Async registry existence checks for the feeder + verifier.

We deliberately don't share Rust crates here — the pipeline runs as plain
Python on CI and we want zero binary deps. The Rust scanner stays the
primary user-facing path; this is a research tool that emits JSON.
"""

from __future__ import annotations

import asyncio
from urllib.parse import quote

import httpx

USER_AGENT = "phantomdep-pipelines/0.1 (https://github.com/openintelligence-labs/phantomdep)"
TIMEOUT = httpx.Timeout(8.0, connect=5.0)


async def exists_pypi(client: httpx.AsyncClient, name: str) -> bool:
    """True if PyPI returns 200 for /pypi/{name}/json."""
    url = f"https://pypi.org/pypi/{quote(name, safe='')}/json"
    r = await client.get(url, headers={"Accept": "application/json"})
    return r.status_code == 200


async def exists_npm(client: httpx.AsyncClient, name: str) -> bool:
    """True if npm registry returns 200 for /{name} (scoped names URL-encoded)."""
    encoded = name.replace("/", "%2F")
    url = f"https://registry.npmjs.org/{encoded}"
    r = await client.get(url, headers={"Accept": "application/json"})
    return r.status_code == 200


async def check_existence(
    names: list[str],
    ecosystem: str,
    *,
    concurrency: int = 16,
) -> dict[str, bool]:
    """Return {name: exists} for every name in `names`.

    Bounded concurrency keeps the pipeline polite to PyPI/npm.
    """
    sem = asyncio.Semaphore(concurrency)

    async def one(client: httpx.AsyncClient, name: str) -> tuple[str, bool]:
        async with sem:
            try:
                if ecosystem == "pypi":
                    return name, await exists_pypi(client, name)
                if ecosystem == "npm":
                    return name, await exists_npm(client, name)
                raise ValueError(f"unsupported ecosystem: {ecosystem}")
            except (httpx.HTTPError, httpx.TimeoutException):
                # On network error, conservatively report "exists" so we don't
                # publish a false phantom. The verifier will pick it up later.
                return name, True

    async with httpx.AsyncClient(
        timeout=TIMEOUT,
        headers={"User-Agent": USER_AGENT},
    ) as client:
        results = await asyncio.gather(*[one(client, n) for n in names])

    return dict(results)

"""LLM provider abstraction for the feeder.

Two providers ship out of the box:

- `ollama`: local stdio HTTP at http://localhost:11434/api/generate
- `mock`: deterministic canned responses for CI / tests / dry-runs

Cloud providers (OpenAI, Anthropic, Gemini) intentionally NOT bundled — they
require API keys and credit, and the responsible-disclosure rule says we
publish only confirmed registrations. CI runs the mock provider so we never
spend money on speculative probing.
"""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass
from typing import Protocol

import httpx


@dataclass(frozen=True)
class LLMResponse:
    prompt_id: str
    model: str
    text: str


class Provider(Protocol):
    name: str
    model: str

    async def generate(self, prompts: Iterable[tuple[str, str]]) -> list[LLMResponse]:
        """Yield (prompt_id, model, text) for each (prompt_id, prompt_text) pair."""
        ...


# ---------------------------------------------------------------------------
# Ollama
# ---------------------------------------------------------------------------


@dataclass
class OllamaProvider:
    name: str = "ollama"
    model: str = "codellama:7b"
    base_url: str = "http://localhost:11434"
    timeout: float = 60.0

    async def generate(self, prompts: Iterable[tuple[str, str]]) -> list[LLMResponse]:
        responses: list[LLMResponse] = []
        async with httpx.AsyncClient(timeout=self.timeout) as client:
            for prompt_id, text in prompts:
                payload = {"model": self.model, "prompt": text, "stream": False}
                r = await client.post(f"{self.base_url}/api/generate", json=payload)
                r.raise_for_status()
                body = r.json()
                responses.append(
                    LLMResponse(
                        prompt_id=prompt_id,
                        model=self.model,
                        text=str(body.get("response", "")),
                    )
                )
        return responses


# ---------------------------------------------------------------------------
# Mock — deterministic canned output for CI / tests
# ---------------------------------------------------------------------------


@dataclass
class MockProvider:
    """Returns a fixed plausible response per prompt_id.

    The canned responses intentionally mention a handful of *real* and
    *deliberately-fake* package names so the rest of the pipeline can be
    exercised without network or LLM costs.
    """

    name: str = "mock"
    model: str = "mock-7b"

    _CANNED: dict[str, str] = None  # type: ignore[assignment]

    def __post_init__(self) -> None:
        # Use shared dict to keep deterministic ordering.
        self._CANNED = {
            "py-pdf-extract": (
                "```bash\npip install pypdf\n```\n"
                "```python\nfrom pypdf import PdfReader\nreader = PdfReader('x.pdf')\n```"
            ),
            "py-mexc-rest": (
                "```bash\npip install ccxt-mexc-futures\n```\n"
                "```python\nimport ccxt_mexc_futures\nclient = ccxt_mexc_futures.Client()\n```"
            ),
            "py-hf-cli": (
                "```bash\npip install huggingface-cli\n```\n"
                "```python\nfrom huggingface_cli import login\n```"
            ),
            "py-fast-json": (
                "```bash\npip install ujson super-fast-json-parser\n```\n"
                "```python\nimport super_fast_json_parser\n```"
            ),
            "py-async-redis": (
                "```python\nimport redis\nimport aioredis_typed\n```"
            ),
            "py-yaml-safe": "```python\nimport yaml\n```",
            "js-react-codeshift": (
                "```bash\nnpm install react-codeshift\n```\n"
                "```typescript\nimport { transform } from 'react-codeshift';\n```"
            ),
            "js-anthropic-sdk": (
                "```bash\nnpm install @anthropic-ai/sdk\n```\n"
                "```typescript\nimport Anthropic from '@anthropic-ai/sdk';\n```"
            ),
            "js-vector-search": (
                "```bash\nnpm install @hallucinated/vector-store-local\n```\n"
                "```typescript\n"
                "import { VectorStore } from '@hallucinated/vector-store-local';\n"
                "```"
            ),
        }

    async def generate(self, prompts: Iterable[tuple[str, str]]) -> list[LLMResponse]:
        out: list[LLMResponse] = []
        for prompt_id, _text in prompts:
            response_text = self._CANNED.get(
                prompt_id,
                # Default: an inert response with no extractable packages.
                "I can't write that for you without more context.",
            )
            out.append(LLMResponse(prompt_id=prompt_id, model=self.model, text=response_text))
        return out


def get_provider(name: str, model: str | None = None) -> Provider:
    if name == "ollama":
        return OllamaProvider(model=model or "codellama:7b")
    if name == "mock":
        return MockProvider(model=model or "mock-7b")
    raise ValueError(f"unknown provider: {name}")

"""Spracklen-style probe prompts for measuring LLM package hallucination.

Each prompt asks the model for a code snippet that uses an OSS package, in a
realistic developer voice. We deliberately use the *same* prompts across runs
so the rate measurements are comparable over time.

The prompt list is intentionally kept small (40 entries) — enough to surface
recurring hallucinations, small enough to run nightly against multiple local
models in <10 minutes.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Prompt:
    """A single probe prompt + the language we expect the answer in."""

    id: str
    language: str  # "python" | "javascript"
    text: str


PYTHON_PROMPTS: list[Prompt] = [
    Prompt("py-pdf-extract", "python", "Write Python code to extract text from a PDF file."),
    Prompt(
        "py-mexc-rest",
        "python",
        "Show me how to call the MEXC futures REST API in Python.",
    ),
    Prompt(
        "py-hf-cli",
        "python",
        "Write Python that uses the Hugging Face CLI to download a model called bert-base.",
    ),
    Prompt(
        "py-fast-json",
        "python",
        "Give me a fast Python JSON parser library and example usage.",
    ),
    Prompt(
        "py-async-redis",
        "python",
        "Write async Python code to set and get a Redis key using a typed client.",
    ),
    Prompt(
        "py-yaml-safe",
        "python",
        "Read a YAML file safely in Python, raising on malicious tags.",
    ),
    Prompt(
        "py-llm-stream",
        "python",
        "Stream tokens from a local Llama model over HTTP in Python.",
    ),
    Prompt(
        "py-fastapi-deps",
        "python",
        "Write FastAPI code with dependency injection for a database session.",
    ),
    Prompt(
        "py-grpc-async",
        "python",
        "Write an async gRPC client in Python that calls a Greeter service.",
    ),
    Prompt(
        "py-arrow-csv",
        "python",
        "Read a CSV with Apache Arrow in Python and project two columns.",
    ),
    Prompt(
        "py-vector-db",
        "python",
        "Index 100 sentences in a local vector database in Python and run a similarity query.",
    ),
    Prompt(
        "py-typed-pydantic",
        "python",
        "Write a Pydantic v2 model for a User and validate from a dict.",
    ),
    Prompt(
        "py-async-postgres",
        "python",
        "Write async Python code to connect to PostgreSQL and run a parameterised query.",
    ),
    Prompt(
        "py-image-resize",
        "python",
        "Resize a JPEG to 512x512 in Python without using PIL.",
    ),
    Prompt(
        "py-sentence-embed",
        "python",
        "Compute sentence embeddings locally in Python without sending data to OpenAI.",
    ),
    Prompt(
        "py-otel-trace",
        "python",
        "Emit an OpenTelemetry trace from a Python script using the local stdout exporter.",
    ),
    Prompt(
        "py-sqlite-vec",
        "python",
        "Use Python to write 100 vectors into sqlite-vec and query the nearest 5.",
    ),
    Prompt(
        "py-feature-flags",
        "python",
        "Implement local feature flags in Python without sending data to a SaaS.",
    ),
    Prompt(
        "py-rate-limit",
        "python",
        "Add token-bucket rate limiting to a FastAPI endpoint in Python.",
    ),
    Prompt(
        "py-mcp-client",
        "python",
        "Write a Python MCP client that calls a local stdio server's tools/list.",
    ),
]

JS_PROMPTS: list[Prompt] = [
    Prompt(
        "js-react-codeshift",
        "javascript",
        "Write a JavaScript codemod using React-Codeshift to upgrade class components to hooks.",
    ),
    Prompt(
        "js-anthropic-sdk",
        "javascript",
        "Call Claude Sonnet from TypeScript using the official Anthropic SDK with streaming.",
    ),
    Prompt(
        "js-pdf-parse",
        "javascript",
        "Parse a PDF and extract text in Node.js without sending it to a server.",
    ),
    Prompt(
        "js-zod-validate",
        "javascript",
        "Validate a JSON payload in TypeScript with a typed schema.",
    ),
    Prompt(
        "js-mcp-server",
        "javascript",
        "Write a TypeScript MCP server that exposes one read-only tool over stdio.",
    ),
    Prompt(
        "js-vector-search",
        "javascript",
        "Index sentences with embeddings locally in Node and run a similarity query.",
    ),
    Prompt(
        "js-cli-prompts",
        "javascript",
        "Build a Node CLI that prompts the user for choices and validates input.",
    ),
    Prompt(
        "js-sql-query",
        "javascript",
        "Run a parameterised SQL query against PostgreSQL from Node, returning typed rows.",
    ),
    Prompt(
        "js-image-magick",
        "javascript",
        "Resize an image in Node without spawning ImageMagick.",
    ),
    Prompt(
        "js-otel-trace",
        "javascript",
        "Emit an OpenTelemetry trace from a Node script using the stdout exporter.",
    ),
    Prompt(
        "js-feature-flags",
        "javascript",
        "Implement local feature flags in TypeScript without sending data to a SaaS.",
    ),
    Prompt(
        "js-rate-limit",
        "javascript",
        "Add token-bucket rate limiting to an Express endpoint in TypeScript.",
    ),
    Prompt(
        "js-grpc-async",
        "javascript",
        "Write an async gRPC client in TypeScript that calls a Greeter service.",
    ),
    Prompt(
        "js-arrow-csv",
        "javascript",
        "Read a CSV with Apache Arrow in Node and project two columns.",
    ),
    Prompt(
        "js-pinecone-local",
        "javascript",
        "Index 100 sentences in a local vector DB in Node and run a similarity query.",
    ),
    Prompt(
        "js-llm-stream",
        "javascript",
        "Stream tokens from a local Llama model over HTTP in TypeScript.",
    ),
    Prompt(
        "js-cli-spinner",
        "javascript",
        "Show a CLI spinner with status messages in a Node script.",
    ),
    Prompt(
        "js-json-schema",
        "javascript",
        "Validate JSON against a JSON Schema in TypeScript with strict typing.",
    ),
    Prompt(
        "js-html-sanitize",
        "javascript",
        "Sanitize untrusted HTML in Node before rendering it in a server-side template.",
    ),
    Prompt(
        "js-image-ocr",
        "javascript",
        "OCR an image to text locally in Node without calling a cloud API.",
    ),
]


ALL_PROMPTS: list[Prompt] = PYTHON_PROMPTS + JS_PROMPTS


def by_language(language: str) -> list[Prompt]:
    """Return the subset of prompts targeting a given language."""
    return [p for p in ALL_PROMPTS if p.language == language]

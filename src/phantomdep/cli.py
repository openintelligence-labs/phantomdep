from __future__ import annotations

import click
from rich.console import Console
from rich.table import Table

from phantomdep.risk import RiskReport

console = Console()


@click.group()
def main() -> None:
    """PhantomDep — scan for hallucinated dependencies."""


@main.command()
@click.argument("path", type=click.Path(exists=True), default=".")
def scan(path: str) -> None:
    """Scan PATH for suspicious dependencies."""
    report = RiskReport()  # TODO: actually scan
    table = Table(title=f"PhantomDep scan: {path}")
    table.add_column("Package")
    table.add_column("Registry")
    table.add_column("Level")
    table.add_column("Reason")
    for finding in report.findings:
        table.add_row(
            finding.package,
            finding.registry,
            finding.level.value,
            finding.reason,
        )
    console.print(table)
    console.print(f"[bold]Max risk:[/bold] {report.max_level.value}")

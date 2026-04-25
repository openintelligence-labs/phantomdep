from __future__ import annotations

from datetime import UTC, datetime, timedelta
from enum import StrEnum

from pydantic import BaseModel, Field


class RiskLevel(StrEnum):
    SAFE = "safe"
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class PackageInfo(BaseModel):
    name: str
    registry: str
    exists: bool = True
    created_at: datetime | None = None
    download_count: int | None = None


class ScanFinding(BaseModel):
    package: str
    registry: str
    level: RiskLevel
    reason: str
    file: str | None = None
    line: int | None = None


class RiskReport(BaseModel):
    findings: list[ScanFinding] = Field(default_factory=list)

    @property
    def max_level(self) -> RiskLevel:
        order = [
            RiskLevel.SAFE,
            RiskLevel.LOW,
            RiskLevel.MEDIUM,
            RiskLevel.HIGH,
            RiskLevel.CRITICAL,
        ]
        if not self.findings:
            return RiskLevel.SAFE
        return max(self.findings, key=lambda f: order.index(f.level)).level


def evaluate(pkg: PackageInfo) -> ScanFinding:
    if not pkg.exists:
        return ScanFinding(
            package=pkg.name,
            registry=pkg.registry,
            level=RiskLevel.CRITICAL,
            reason="package does not exist on registry",
        )
    if pkg.created_at is not None:
        age = datetime.now(UTC) - pkg.created_at
        if age < timedelta(days=7):
            return ScanFinding(
                package=pkg.name,
                registry=pkg.registry,
                level=RiskLevel.HIGH,
                reason=f"package is only {age.days} days old (slop-squat risk)",
            )
    if pkg.download_count is not None and pkg.download_count < 100:
        return ScanFinding(
            package=pkg.name,
            registry=pkg.registry,
            level=RiskLevel.MEDIUM,
            reason=f"only {pkg.download_count} downloads",
        )
    return ScanFinding(
        package=pkg.name,
        registry=pkg.registry,
        level=RiskLevel.SAFE,
        reason="no signals",
    )

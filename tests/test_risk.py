from __future__ import annotations

from datetime import UTC, datetime, timedelta

from phantomdep.risk import PackageInfo, RiskLevel, RiskReport, evaluate


def test_nonexistent_package_is_critical():
    pkg = PackageInfo(name="flask-auth-utils", registry="pypi", exists=False)
    finding = evaluate(pkg)
    assert finding.level is RiskLevel.CRITICAL
    assert "does not exist" in finding.reason


def test_young_package_is_high_risk():
    pkg = PackageInfo(
        name="new-thing",
        registry="pypi",
        created_at=datetime.now(UTC) - timedelta(days=2),
    )
    finding = evaluate(pkg)
    assert finding.level is RiskLevel.HIGH


def test_low_downloads_is_medium():
    pkg = PackageInfo(
        name="obscure",
        registry="pypi",
        created_at=datetime.now(UTC) - timedelta(days=365),
        download_count=10,
    )
    finding = evaluate(pkg)
    assert finding.level is RiskLevel.MEDIUM


def test_established_package_is_safe():
    pkg = PackageInfo(
        name="requests",
        registry="pypi",
        created_at=datetime.now(UTC) - timedelta(days=1000),
        download_count=1_000_000,
    )
    finding = evaluate(pkg)
    assert finding.level is RiskLevel.SAFE


def test_report_max_level_critical_beats_safe():
    report = RiskReport(
        findings=[
            evaluate(PackageInfo(name="ok", registry="pypi", download_count=10_000)),
            evaluate(PackageInfo(name="bad", registry="pypi", exists=False)),
        ]
    )
    assert report.max_level is RiskLevel.CRITICAL


def test_empty_report_is_safe():
    assert RiskReport().max_level is RiskLevel.SAFE

use time::OffsetDateTime;

use crate::checker::PackageRecord;
use crate::evidence::{Evidence, EvidenceBundle, Fix};
use crate::phantom_db::{PhantomDb, PhantomStatus};
use crate::popular::top_packages;
use crate::verdict::{Action, Ecosystem, Verdict};

const LOOKALIKE_DISTANCE_BLOCK: usize = 1;
const LOOKALIKE_DISTANCE_WARN: usize = 2;

pub struct Resolver<'a> {
    pub phantom_db: &'a PhantomDb,
    pub now: OffsetDateTime,
}

impl<'a> Resolver<'a> {
    pub fn new(phantom_db: &'a PhantomDb) -> Self {
        Self {
            phantom_db,
            now: OffsetDateTime::now_utc(),
        }
    }

    pub fn resolve(&self, name: &str, ecosystem: Ecosystem, record: PackageRecord) -> EvidenceBundle {
        let mut bundle = EvidenceBundle::new(name, ecosystem);
        bundle.phantom_db_snapshot = self.phantom_db.snapshot().map(String::from);

        // Verdicts are resolved in priority order. The first matching verdict wins;
        // remaining signals contribute evidence but do not override.

        // 1. PHANTOM — registry says it does not exist.
        if !record.exists {
            bundle.verdict = Verdict::Phantom;
            bundle.action = Action::Block;
            bundle.confidence = 0.99;
            bundle.add(Evidence::RegistryExistence {
                source: ecosystem.as_str().into(),
                exists: false,
                checked_at: self.now,
            });
            self.attach_phantom_db_evidence(&mut bundle, name, ecosystem, true);
            self.attach_lookalike_fixes(&mut bundle, name, ecosystem);
            return bundle;
        }

        bundle.add(Evidence::RegistryExistence {
            source: ecosystem.as_str().into(),
            exists: true,
            checked_at: self.now,
        });

        // 2. KNOWN_MALICIOUS / SQUATTED via Phantom-DB.
        if let Some(entry) = self.phantom_db.lookup(name, ecosystem) {
            let (verdict, action, confidence) = match entry.status {
                PhantomStatus::Malicious => (Verdict::KnownMalicious, Action::Block, 0.98),
                PhantomStatus::Squatted => (Verdict::Squatted, Action::Block, 0.95),
                PhantomStatus::Phantom => (Verdict::Squatted, Action::Block, 0.92),
            };
            bundle.verdict = verdict;
            bundle.action = action;
            bundle.confidence = confidence;
            bundle.add(Evidence::PhantomDbHit {
                source: "phantom-db".into(),
                status: format!("{:?}", entry.status).to_lowercase(),
                first_observed: entry.first_observed.clone(),
                intended_target: entry.intended_target.clone(),
                checked_at: self.now,
            });
            for replacement in &entry.did_you_mean {
                bundle.fixes.push(Fix {
                    replacement: replacement.clone(),
                    confidence: 0.85,
                });
            }
            return bundle;
        }

        // Age signal (recorded for downstream risk score regardless of verdict).
        if let Some(created) = record.created_at {
            let age_days = ((self.now - created).whole_days()).max(0) as u64;
            bundle.add(Evidence::RegistryAge {
                source: ecosystem.as_str().into(),
                value_days: age_days,
                checked_at: self.now,
            });
        }
        if let Some(downloads) = record.downloads_30d {
            bundle.add(Evidence::Downloads30d {
                source: ecosystem.as_str().into(),
                value: downloads,
                checked_at: self.now,
            });
        }

        // 3. LOOKALIKE — edit distance to a popular real package.
        if let Some((distance, target)) = closest_popular(name, ecosystem) {
            if distance > 0 && distance <= LOOKALIKE_DISTANCE_WARN {
                bundle.add(Evidence::Lookalike {
                    source: "popular-list".into(),
                    edit_distance: distance,
                    compared_to: target.clone(),
                });
                if distance <= LOOKALIKE_DISTANCE_BLOCK {
                    bundle.verdict = Verdict::Lookalike;
                    bundle.action = Action::Warn;
                    bundle.confidence = 0.75;
                    bundle.fixes.push(Fix {
                        replacement: target,
                        confidence: 0.7,
                    });
                    bundle.risk_score = compute_risk_score(&bundle, &record, self.now);
                    return bundle;
                }
            }
        }

        // 4. REAL — package exists, no verdict-level threat. Score the risk.
        bundle.verdict = Verdict::Real;
        bundle.action = Action::Allow;
        bundle.confidence = 0.9;
        bundle.risk_score = compute_risk_score(&bundle, &record, self.now);
        bundle
    }

    fn attach_phantom_db_evidence(
        &self,
        bundle: &mut EvidenceBundle,
        name: &str,
        ecosystem: Ecosystem,
        for_phantom: bool,
    ) {
        if let Some(entry) = self.phantom_db.lookup(name, ecosystem) {
            bundle.add(Evidence::PhantomDbHit {
                source: "phantom-db".into(),
                status: format!("{:?}", entry.status).to_lowercase(),
                first_observed: entry.first_observed.clone(),
                intended_target: entry.intended_target.clone(),
                checked_at: self.now,
            });
            if for_phantom {
                for replacement in &entry.did_you_mean {
                    bundle.fixes.push(Fix {
                        replacement: replacement.clone(),
                        confidence: 0.9,
                    });
                }
            }
        }
    }

    fn attach_lookalike_fixes(&self, bundle: &mut EvidenceBundle, name: &str, ecosystem: Ecosystem) {
        if let Some((distance, target)) = closest_popular(name, ecosystem) {
            if distance > 0 && distance <= LOOKALIKE_DISTANCE_WARN && bundle.fixes.is_empty() {
                bundle.fixes.push(Fix {
                    replacement: target,
                    confidence: 0.6,
                });
            }
        }
    }
}

fn closest_popular(name: &str, ecosystem: Ecosystem) -> Option<(usize, String)> {
    let needle = name.to_ascii_lowercase();
    top_packages(ecosystem)
        .iter()
        .map(|p| (strsim::damerau_levenshtein(&needle, p), p.to_string()))
        .min_by_key(|(d, _)| *d)
}

fn compute_risk_score(
    bundle: &EvidenceBundle,
    record: &PackageRecord,
    now: OffsetDateTime,
) -> u8 {
    let mut score: i32 = 0;

    if let Some(created) = record.created_at {
        let age_days = ((now - created).whole_days()).max(0) as u64;
        score += match age_days {
            0..=6 => 25,
            7..=29 => 15,
            30..=89 => 5,
            _ => 0,
        };
    }

    if let Some(downloads) = record.downloads_30d {
        score += match downloads {
            0 => 20,
            1..=99 => 15,
            100..=999 => 5,
            _ => 0,
        };
    }

    if record.starjacked == Some(true) {
        score += 15;
    } else if record.repository_url.is_none() {
        score += 5;
    }

    if record.provenance_verified == Some(true) {
        score -= 15;
    }

    let _ = bundle;
    score.clamp(0, 100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phantom_db::PhantomDb;

    fn fixed_resolver(db: &PhantomDb) -> Resolver<'_> {
        Resolver {
            phantom_db: db,
            now: OffsetDateTime::from_unix_timestamp(1_780_000_000).unwrap(),
        }
    }

    #[test]
    fn missing_package_is_phantom_block() {
        let db = PhantomDb::empty();
        let resolver = fixed_resolver(&db);
        let record = PackageRecord::missing("nonexistent-pkg-xyz", Ecosystem::Pypi);
        let bundle = resolver.resolve("nonexistent-pkg-xyz", Ecosystem::Pypi, record);
        assert_eq!(bundle.verdict, Verdict::Phantom);
        assert_eq!(bundle.action, Action::Block);
    }

    #[test]
    fn squatted_known_entry_blocks() {
        let db = PhantomDb::bootstrap();
        let resolver = fixed_resolver(&db);
        let mut record = PackageRecord::missing("huggingface-cli", Ecosystem::Pypi);
        record.exists = true;
        let bundle = resolver.resolve("huggingface-cli", Ecosystem::Pypi, record);
        assert_eq!(bundle.verdict, Verdict::Squatted);
        assert_eq!(bundle.action, Action::Block);
        assert!(!bundle.fixes.is_empty());
    }

    #[test]
    fn established_real_package_allows() {
        let db = PhantomDb::empty();
        let resolver = fixed_resolver(&db);
        let mut record = PackageRecord::missing("requests", Ecosystem::Pypi);
        record.exists = true;
        record.created_at = Some(OffsetDateTime::from_unix_timestamp(1_300_000_000).unwrap());
        record.downloads_30d = Some(100_000_000);
        record.repository_url = Some("https://github.com/psf/requests".into());
        let bundle = resolver.resolve("requests", Ecosystem::Pypi, record);
        assert_eq!(bundle.verdict, Verdict::Real);
        assert_eq!(bundle.action, Action::Allow);
    }

    #[test]
    fn lookalike_warns_with_fix() {
        let db = PhantomDb::empty();
        let resolver = fixed_resolver(&db);
        let mut record = PackageRecord::missing("reqests", Ecosystem::Pypi);
        record.exists = true;
        record.created_at = Some(OffsetDateTime::from_unix_timestamp(1_770_000_000).unwrap());
        let bundle = resolver.resolve("reqests", Ecosystem::Pypi, record);
        assert_eq!(bundle.verdict, Verdict::Lookalike);
        assert_eq!(bundle.action, Action::Warn);
        assert!(bundle.fixes.iter().any(|f| f.replacement == "requests"));
    }

    #[test]
    fn phantom_includes_phantom_db_evidence_when_available() {
        let db = PhantomDb::bootstrap();
        let resolver = fixed_resolver(&db);
        let record = PackageRecord::missing("huggingface-cli", Ecosystem::Pypi);
        let bundle = resolver.resolve("huggingface-cli", Ecosystem::Pypi, record);
        assert_eq!(bundle.verdict, Verdict::Phantom);
        let has_db_hit = bundle
            .evidence
            .iter()
            .any(|e| matches!(e, Evidence::PhantomDbHit { .. }));
        assert!(has_db_hit);
    }
}

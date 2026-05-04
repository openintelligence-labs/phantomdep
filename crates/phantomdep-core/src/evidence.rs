use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::verdict::{Action, Ecosystem, Verdict};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum Evidence {
    RegistryExistence {
        source: String,
        exists: bool,
        #[serde(with = "time::serde::rfc3339")]
        checked_at: OffsetDateTime,
    },
    RegistryAge {
        source: String,
        value_days: u64,
        #[serde(with = "time::serde::rfc3339")]
        checked_at: OffsetDateTime,
    },
    Downloads30d {
        source: String,
        value: u64,
        #[serde(with = "time::serde::rfc3339")]
        checked_at: OffsetDateTime,
    },
    PhantomDbHit {
        source: String,
        status: String,
        first_observed: Option<String>,
        intended_target: Option<String>,
        #[serde(with = "time::serde::rfc3339")]
        checked_at: OffsetDateTime,
    },
    Lookalike {
        source: String,
        edit_distance: usize,
        compared_to: String,
    },
    KnownMalicious {
        source: String,
        advisory_id: Option<String>,
    },
    GithubLink {
        source: String,
        url: Option<String>,
        starjacked: bool,
    },
    Provenance {
        source: String,
        verified: bool,
    },
    Note {
        source: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    pub replacement: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundle {
    pub name: String,
    pub ecosystem: Ecosystem,
    pub verdict: Verdict,
    pub action: Action,
    pub confidence: f32,
    pub phantom_db_snapshot: Option<String>,
    pub evidence: Vec<Evidence>,
    pub fixes: Vec<Fix>,
    pub explain_url: Option<String>,
    pub risk_score: u8,
}

impl EvidenceBundle {
    pub fn new(name: impl Into<String>, ecosystem: Ecosystem) -> Self {
        Self {
            name: name.into(),
            ecosystem,
            verdict: Verdict::Unknown,
            action: Action::Warn,
            confidence: 0.0,
            phantom_db_snapshot: None,
            evidence: Vec::new(),
            fixes: Vec::new(),
            explain_url: None,
            risk_score: 0,
        }
    }

    pub fn add(&mut self, evidence: Evidence) {
        self.evidence.push(evidence);
    }
}

/// Short, human-readable rendering of one Evidence variant.
/// Shared between CLI terminal output, markdown PR comments, and explain.
pub fn evidence_short_text(ev: &Evidence) -> String {
    use Evidence::*;
    match ev {
        RegistryExistence { source, exists, .. } => {
            format!("registry_existence: {source} reports exists={exists}")
        }
        RegistryAge {
            source, value_days, ..
        } => format!("registry_age: {source} package is {value_days} days old"),
        Downloads30d { source, value, .. } => {
            format!("downloads_30d: {source} reports {value}")
        }
        PhantomDbHit {
            status,
            first_observed,
            intended_target,
            ..
        } => {
            let mut s = format!("phantom_db_hit: status={status}");
            if let Some(when) = first_observed {
                s.push_str(&format!(", first_observed={when}"));
            }
            if let Some(target) = intended_target {
                s.push_str(&format!(", intended_target={target}"));
            }
            s
        }
        Lookalike {
            edit_distance,
            compared_to,
            ..
        } => format!("lookalike: edit_distance={edit_distance} from {compared_to}"),
        KnownMalicious {
            source,
            advisory_id,
        } => match advisory_id {
            Some(id) => format!("known_malicious: {source} advisory {id}"),
            None => format!("known_malicious: {source}"),
        },
        GithubLink {
            url, starjacked, ..
        } => format!(
            "github_link: url={} starjacked={}",
            url.as_deref().unwrap_or("none"),
            starjacked
        ),
        Provenance { source, verified } => {
            format!("provenance: {source} verified={verified}")
        }
        Note { source, message } => format!("note ({source}): {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_serialises_to_documented_shape() {
        let mut bundle = EvidenceBundle::new("huggingface-cli", Ecosystem::Pypi);
        bundle.verdict = Verdict::Squatted;
        bundle.action = Action::Block;
        bundle.confidence = 0.96;
        bundle.phantom_db_snapshot = Some("2026-05-02".into());
        bundle.add(Evidence::PhantomDbHit {
            source: "phantom-db".into(),
            status: "squatted".into(),
            first_observed: Some("2024-02-13".into()),
            intended_target: Some("huggingface_hub[cli]".into()),
            checked_at: OffsetDateTime::UNIX_EPOCH,
        });
        bundle.fixes.push(Fix {
            replacement: "huggingface_hub[cli]".into(),
            confidence: 0.91,
        });

        let json = serde_json::to_value(&bundle).unwrap();
        assert_eq!(json["verdict"], "SQUATTED");
        assert_eq!(json["action"], "BLOCK");
        assert_eq!(json["ecosystem"], "pypi");
        assert_eq!(json["evidence"][0]["signal"], "phantom_db_hit");
        assert_eq!(json["fixes"][0]["replacement"], "huggingface_hub[cli]");
    }
}

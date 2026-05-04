use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::verdict::Ecosystem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRecord {
    pub name: String,
    pub ecosystem: Ecosystem,
    pub exists: bool,
    pub created_at: Option<OffsetDateTime>,
    pub downloads_30d: Option<u64>,
    pub repository_url: Option<String>,
    pub starjacked: Option<bool>,
    pub provenance_verified: Option<bool>,
}

impl PackageRecord {
    pub fn missing(name: impl Into<String>, ecosystem: Ecosystem) -> Self {
        Self {
            name: name.into(),
            ecosystem,
            exists: false,
            created_at: None,
            downloads_30d: None,
            repository_url: None,
            starjacked: None,
            provenance_verified: None,
        }
    }
}

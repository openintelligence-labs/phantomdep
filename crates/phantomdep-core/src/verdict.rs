use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Phantom,
    KnownMalicious,
    Squatted,
    InternalCollision,
    ApiMismatch,
    Lookalike,
    Real,
    Unknown,
}

impl Verdict {
    pub fn priority(self) -> u8 {
        match self {
            Self::Phantom => 0,
            Self::KnownMalicious => 1,
            Self::Squatted => 2,
            Self::InternalCollision => 3,
            Self::ApiMismatch => 4,
            Self::Lookalike => 5,
            Self::Real => 6,
            Self::Unknown => 7,
        }
    }

    pub fn default_action(self) -> Action {
        match self {
            Self::Phantom => Action::Block,
            Self::KnownMalicious => Action::Block,
            Self::Squatted => Action::Block,
            Self::InternalCollision => Action::Block,
            Self::ApiMismatch => Action::Warn,
            Self::Lookalike => Action::Warn,
            Self::Real => Action::Allow,
            Self::Unknown => Action::Warn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Action {
    Allow,
    Warn,
    Block,
}

impl Action {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Allow => 0,
            Self::Warn => 1,
            Self::Block => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Pypi,
    Npm,
    Cargo,
    Go,
    Maven,
}

impl Ecosystem {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pypi => "pypi",
            Self::Npm => "npm",
            Self::Cargo => "cargo",
            Self::Go => "go",
            Self::Maven => "maven",
        }
    }
}

impl std::str::FromStr for Ecosystem {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "pypi" | "python" | "pip" => Ok(Self::Pypi),
            "npm" | "js" | "javascript" | "ts" | "typescript" => Ok(Self::Npm),
            "cargo" | "rust" | "crates" => Ok(Self::Cargo),
            "go" | "golang" => Ok(Self::Go),
            "maven" | "java" => Ok(Self::Maven),
            other => Err(format!("unknown ecosystem: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phantom_outranks_everything() {
        let mut verdicts = [
            Verdict::Real,
            Verdict::Lookalike,
            Verdict::ApiMismatch,
            Verdict::Phantom,
            Verdict::Squatted,
        ];
        verdicts.sort_by_key(|v| v.priority());
        assert_eq!(verdicts[0], Verdict::Phantom);
    }

    #[test]
    fn block_actions_align() {
        assert_eq!(Verdict::Phantom.default_action(), Action::Block);
        assert_eq!(Verdict::KnownMalicious.default_action(), Action::Block);
        assert_eq!(Verdict::Squatted.default_action(), Action::Block);
        assert_eq!(Verdict::Real.default_action(), Action::Allow);
        assert_eq!(Verdict::Unknown.default_action(), Action::Warn);
    }

    #[test]
    fn ecosystem_parses_aliases() {
        assert_eq!("pypi".parse::<Ecosystem>().unwrap(), Ecosystem::Pypi);
        assert_eq!("python".parse::<Ecosystem>().unwrap(), Ecosystem::Pypi);
        assert_eq!("NPM".parse::<Ecosystem>().unwrap(), Ecosystem::Npm);
        assert_eq!("rust".parse::<Ecosystem>().unwrap(), Ecosystem::Cargo);
        assert!("perl".parse::<Ecosystem>().is_err());
    }
}

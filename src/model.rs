use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A validated four-digit positive year.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Year(u16);

impl Year {
    /// Creates a year in the supported `0001..=9999` range.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is zero or greater than 9999.
    pub fn new(value: u16) -> Result<Self, ValidationError> {
        if (1..=9999).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ValidationError::Year(value))
        }
    }

    /// Returns the numeric year.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Year {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:04}", self.0)
    }
}

/// A validated, unpadded calendar month.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Month(u8);

impl Month {
    /// Creates a month in the `1..=12` range.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is outside the calendar month range.
    pub fn new(value: u8) -> Result<Self, ValidationError> {
        if (1..=12).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ValidationError::Month(value))
        }
    }

    /// Returns the numeric month.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for Month {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A year and month used for calendar-based version calculation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct YearMonth {
    year: Year,
    month: Month,
}

impl YearMonth {
    /// Creates a validated year and month.
    ///
    /// # Errors
    ///
    /// Returns an error when either component is outside its supported range.
    pub fn new(year: u16, month: u8) -> Result<Self, ValidationError> {
        Ok(Self {
            year: Year::new(year)?,
            month: Month::new(month)?,
        })
    }

    /// Returns the year.
    #[must_use]
    pub const fn year(self) -> Year {
        self.year
    }

    /// Returns the month.
    #[must_use]
    pub const fn month(self) -> Month {
        self.month
    }
}

impl fmt::Display for YearMonth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.year, self.month)
    }
}

/// Stability from least to most stable.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Stability {
    /// A development release.
    Dev,
    /// An early alpha release.
    Alpha,
    /// A feature-complete beta release.
    Beta,
    /// A release candidate.
    Rc,
    /// A final release.
    #[default]
    Final,
}

impl Stability {
    /// Returns the stable textual representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Rc => "rc",
            Self::Final => "final",
        }
    }
}

impl fmt::Display for Stability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A validated 7–16 character lowercase hexadecimal Git hash.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GitHash(String);

impl GitHash {
    /// Validates and stores a short Git hash.
    ///
    /// # Errors
    ///
    /// Returns an error unless the value is 7–16 lowercase hexadecimal characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        let length = value.len();
        if !(7..=16).contains(&length) {
            return Err(ValidationError::HashLength(length));
        }
        if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ValidationError::HashCharacters);
        }
        if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(ValidationError::HashUppercase);
        }
        Ok(Self(value))
    }

    /// Returns the hash text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GitHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The mutually exclusive release and Git-build representations.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum VersionKind {
    /// A numbered release or pre-release.
    Release {
        /// The release increment within the month.
        increment: u64,
        /// The release stability.
        tag: Stability,
    },
    /// A development build identified by a Git hash.
    Git {
        /// The abbreviated commit hash.
        hash: GitHash,
    },
}

/// A validated calendar-anchored version.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChronoStamp {
    year: Year,
    month: Month,
    kind: VersionKind,
}

impl ChronoStamp {
    /// Creates a numbered release.
    ///
    /// # Errors
    ///
    /// Returns an error when the year or month is outside its supported range.
    pub fn release(
        year: u16,
        month: u8,
        increment: u64,
        tag: Stability,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            year: Year::new(year)?,
            month: Month::new(month)?,
            kind: VersionKind::Release { increment, tag },
        })
    }

    /// Creates a Git development build.
    ///
    /// # Errors
    ///
    /// Returns an error when the date or hash is invalid.
    pub fn git(year: u16, month: u8, hash: impl Into<String>) -> Result<Self, ValidationError> {
        Ok(Self {
            year: Year::new(year)?,
            month: Month::new(month)?,
            kind: VersionKind::Git {
                hash: GitHash::new(hash)?,
            },
        })
    }

    pub(crate) const fn from_parts(year: Year, month: Month, kind: VersionKind) -> Self {
        Self { year, month, kind }
    }

    /// Returns the calendar year.
    #[must_use]
    pub const fn year(&self) -> Year {
        self.year
    }

    /// Returns the calendar month.
    #[must_use]
    pub const fn month(&self) -> Month {
        self.month
    }

    /// Returns the calendar anchor.
    #[must_use]
    pub const fn year_month(&self) -> YearMonth {
        YearMonth {
            year: self.year,
            month: self.month,
        }
    }

    /// Returns the version representation.
    #[must_use]
    pub const fn kind(&self) -> &VersionKind {
        &self.kind
    }

    /// Returns the release increment, or `None` for a Git build.
    #[must_use]
    pub const fn increment(&self) -> Option<u64> {
        match self.kind {
            VersionKind::Release { increment, .. } => Some(increment),
            VersionKind::Git { .. } => None,
        }
    }

    /// Returns the Git hash, or `None` for a numbered release.
    #[must_use]
    pub fn git_hash(&self) -> Option<&GitHash> {
        match &self.kind {
            VersionKind::Git { hash } => Some(hash),
            VersionKind::Release { .. } => None,
        }
    }

    /// Returns the explicit or implicit stability.
    #[must_use]
    pub const fn stability(&self) -> Stability {
        match self.kind {
            VersionKind::Release { tag, .. } => tag,
            VersionKind::Git { .. } => Stability::Dev,
        }
    }

    /// Returns whether this is a pre-release.
    #[must_use]
    pub const fn is_prerelease(&self) -> bool {
        !matches!(
            self.kind,
            VersionKind::Release {
                tag: Stability::Final,
                ..
            }
        )
    }

    /// Returns whether this is a Git or `dev` build.
    #[must_use]
    pub const fn is_dev_build(&self) -> bool {
        matches!(
            self.kind,
            VersionKind::Git { .. }
                | VersionKind::Release {
                    tag: Stability::Dev,
                    ..
                }
        )
    }

    /// Returns the complete canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        match &self.kind {
            VersionKind::Release { increment, tag } => {
                format!("{}.{}.{}-{tag}", self.year, self.month, increment)
            }
            VersionKind::Git { hash } => format!("{}.{}.0-{hash}", self.year, self.month),
        }
    }
}

impl fmt::Display for ChronoStamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            VersionKind::Release {
                increment,
                tag: Stability::Final,
            } => write!(formatter, "{}.{}.{}", self.year, self.month, increment),
            VersionKind::Release { increment, tag } => {
                write!(
                    formatter,
                    "{}.{}.{}-{tag}",
                    self.year, self.month, increment
                )
            }
            VersionKind::Git { hash } => write!(formatter, "{}.{}.0-{hash}", self.year, self.month),
        }
    }
}

impl Ord for ChronoStamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.year
            .cmp(&other.year)
            .then_with(|| self.month.cmp(&other.month))
            .then_with(|| match (&self.kind, &other.kind) {
                (VersionKind::Git { hash: left }, VersionKind::Git { hash: right }) => {
                    left.cmp(right)
                }
                (VersionKind::Git { .. }, VersionKind::Release { .. }) => Ordering::Less,
                (VersionKind::Release { .. }, VersionKind::Git { .. }) => Ordering::Greater,
                (
                    VersionKind::Release {
                        increment: left_increment,
                        tag: left_tag,
                    },
                    VersionKind::Release {
                        increment: right_increment,
                        tag: right_tag,
                    },
                ) => left_increment
                    .cmp(right_increment)
                    .then_with(|| left_tag.cmp(right_tag)),
            })
    }
}

impl PartialOrd for ChronoStamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for ChronoStamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ChronoStamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Component validation failures.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ValidationError {
    /// The year falls outside the supported range.
    #[error("year must be between 0001 and 9999, got {0}")]
    Year(u16),
    /// The month falls outside the calendar range.
    #[error("month must be between 1 and 12, got {0}")]
    Month(u8),
    /// The Git hash has an unsupported length.
    #[error("Git hash must contain 7 to 16 characters, got {0}")]
    HashLength(usize),
    /// The Git hash contains a non-hexadecimal character.
    #[error("Git hash must contain only hexadecimal characters")]
    HashCharacters,
    /// The Git hash contains uppercase characters.
    #[error("Git hash must use lowercase hexadecimal characters")]
    HashUppercase,
}

use std::str::FromStr;

use thiserror::Error;

use crate::model::{ChronoStamp, GitHash, Month, Stability, ValidationError, VersionKind, Year};

impl FromStr for Stability {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "dev" => Ok(Self::Dev),
            "alpha" => Ok(Self::Alpha),
            "beta" => Ok(Self::Beta),
            "rc" => Ok(Self::Rc),
            "final" => Ok(Self::Final),
            _ => Err(ParseError::UnknownTag(value.to_owned())),
        }
    }
}

impl FromStr for ChronoStamp {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(ParseError::Empty);
        }
        if !value.is_ascii() {
            return Err(ParseError::NonAscii);
        }

        let mut suffix_parts = value.split('-');
        let base = suffix_parts.next().expect("split always yields one item");
        let suffix = suffix_parts.next();
        if suffix_parts.next().is_some() || suffix.is_some_and(str::is_empty) {
            return Err(ParseError::Structure);
        }

        let mut base_parts = base.split('.');
        let year_text = base_parts.next().ok_or(ParseError::Structure)?;
        let month_text = base_parts.next().ok_or(ParseError::Structure)?;
        let increment_text = base_parts.next();
        if base_parts.next().is_some() || year_text.is_empty() || month_text.is_empty() {
            return Err(ParseError::Structure);
        }

        let year = parse_year(year_text)?;
        let month = parse_month(month_text)?;
        let increment = increment_text.map(parse_increment).transpose()?;

        match suffix {
            None => Ok(ChronoStamp::from_parts(
                year,
                month,
                VersionKind::Release {
                    increment: increment.unwrap_or(0),
                    tag: Stability::Final,
                },
            )),
            Some(suffix) => match suffix.parse::<Stability>() {
                Ok(tag) => Ok(ChronoStamp::from_parts(
                    year,
                    month,
                    VersionKind::Release {
                        increment: increment.unwrap_or(0),
                        tag,
                    },
                )),
                Err(ParseError::UnknownTag(_)) => {
                    let hash = GitHash::new(suffix).map_err(ParseError::InvalidHash)?;
                    let Some(increment) = increment else {
                        return Err(ParseError::HashRequiresIncrement);
                    };
                    if increment != 0 {
                        return Err(ParseError::HashRequiresZero(increment));
                    }
                    Ok(ChronoStamp::from_parts(
                        year,
                        month,
                        VersionKind::Git { hash },
                    ))
                }
                Err(error) => Err(error),
            },
        }
    }
}

fn parse_year(value: &str) -> Result<Year, ParseError> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::YearFormat);
    }
    let year = value.parse::<u16>().map_err(|_| ParseError::YearFormat)?;
    Year::new(year).map_err(ParseError::InvalidComponent)
}

fn parse_month(value: &str) -> Result<Month, ParseError> {
    if value.len() > 1 && value.starts_with('0') {
        return Err(ParseError::PaddedMonth);
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::MonthFormat);
    }
    let month = value.parse::<u8>().map_err(|_| ParseError::MonthFormat)?;
    Month::new(month).map_err(ParseError::InvalidComponent)
}

fn parse_increment(value: &str) -> Result<u64, ParseError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::IncrementFormat);
    }
    value.parse().map_err(|_| ParseError::IncrementOverflow)
}

/// Errors returned while parsing `ChronoStamp` text.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ParseError {
    /// The input is empty.
    #[error("ChronoStamp cannot be empty")]
    Empty,
    /// The input contains non-ASCII text.
    #[error("ChronoStamp must contain only ASCII characters")]
    NonAscii,
    /// Separators or component counts are invalid.
    #[error("expected YYYY.M[.INCREMENT][-TAG] or YYYY.M.0-HASH")]
    Structure,
    /// The year is not exactly four digits.
    #[error("year must contain exactly four digits")]
    YearFormat,
    /// The month is not numeric.
    #[error("month must be an integer from 1 to 12")]
    MonthFormat,
    /// The month has forbidden leading zero padding.
    #[error("month must not be zero-padded")]
    PaddedMonth,
    /// The increment is not an unsigned integer.
    #[error("increment must be a non-negative integer")]
    IncrementFormat,
    /// The increment does not fit the supported representation.
    #[error("increment exceeds the maximum supported value")]
    IncrementOverflow,
    /// A stability tag is unknown.
    #[error("unknown stability tag `{0}`")]
    UnknownTag(String),
    /// A hash did not satisfy the hash contract.
    #[error("invalid Git hash: {0}")]
    InvalidHash(ValidationError),
    /// A hash suffix omitted the required `.0` increment.
    #[error("Git hash builds must include an explicit `.0` increment")]
    HashRequiresIncrement,
    /// A hash suffix used a nonzero increment.
    #[error("Git hash builds require increment 0, got {0}")]
    HashRequiresZero(u64),
    /// A parsed component is outside its supported range.
    #[error(transparent)]
    InvalidComponent(ValidationError),
}

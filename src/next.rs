use thiserror::Error;

use crate::{ChronoStamp, Stability, ValidationError, VersionKind, YearMonth};

/// Options controlling calendar-aware version progression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NextVersionOptions {
    /// Requested stability for the next version.
    pub tag: Stability,
    /// Optional explicit release increment.
    pub increment: Option<u64>,
    /// Whether an existing future-dated version may be ignored.
    pub allow_future: bool,
}

impl Default for NextVersionOptions {
    fn default() -> Self {
        Self {
            tag: Stability::Final,
            increment: None,
            allow_future: false,
        }
    }
}

/// Calculates the next release from authoritative versions and a UTC month.
///
/// # Errors
///
/// Returns an error for a future authoritative version, increment overflow, or
/// an invalid calculated date.
pub fn next_version(
    versions: &[ChronoStamp],
    now: YearMonth,
    options: NextVersionOptions,
) -> Result<ChronoStamp, NextVersionError> {
    let latest_future = latest_future_version(versions, now);
    if !options.allow_future
        && let Some(future) = latest_future
    {
        return Err(NextVersionError::FutureVersion {
            version: future.clone(),
            now,
        });
    }

    let effective_now = latest_future.map_or(now, ChronoStamp::year_month);

    if let Some(increment) = options.increment {
        let candidate = ChronoStamp::release(
            effective_now.year().get(),
            effective_now.month().get(),
            increment,
            options.tag,
        )
        .map_err(NextVersionError::InvalidVersion)?;
        if let Some(latest) = latest_release(versions, effective_now)
            && candidate <= *latest
        {
            return Err(NextVersionError::NonMonotonic {
                candidate,
                latest: latest.clone(),
            });
        }
        return Ok(candidate);
    }

    let latest_release = latest_release(versions, effective_now);

    let (increment, tag) = match latest_release {
        None => (0, options.tag),
        Some(version) => {
            let VersionKind::Release { increment, tag } = version.kind() else {
                unreachable!("latest_release was filtered to releases")
            };
            if *tag != Stability::Final && options.tag > *tag {
                (*increment, options.tag)
            } else {
                (
                    increment
                        .checked_add(1)
                        .ok_or(NextVersionError::IncrementOverflow)?,
                    options.tag,
                )
            }
        }
    };

    ChronoStamp::release(
        effective_now.year().get(),
        effective_now.month().get(),
        increment,
        tag,
    )
    .map_err(NextVersionError::InvalidVersion)
}

/// Returns the highest authoritative version dated after `now`, if any.
pub fn latest_future_version<'a, I>(versions: I, now: YearMonth) -> Option<&'a ChronoStamp>
where
    I: IntoIterator<Item = &'a ChronoStamp>,
{
    versions
        .into_iter()
        .filter(|version| version.year_month() > now)
        .max()
}

fn latest_release(versions: &[ChronoStamp], month: YearMonth) -> Option<&ChronoStamp> {
    versions
        .iter()
        .filter(|version| version.year_month() == month)
        .filter(|version| matches!(version.kind(), VersionKind::Release { .. }))
        .max()
}

/// Failures from calendar-aware version calculation.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum NextVersionError {
    /// An authoritative version is dated after the supplied UTC month.
    #[error("version {version} is in the future relative to {now}")]
    FutureVersion {
        /// The future version.
        version: ChronoStamp,
        /// The requested current month.
        now: YearMonth,
    },
    /// Incrementing the latest release would overflow.
    #[error("release increment exceeds the maximum supported value")]
    IncrementOverflow,
    /// An explicit candidate would repeat or move behind an authoritative release.
    #[error("requested version {candidate} does not advance past authoritative version {latest}")]
    NonMonotonic {
        /// The explicitly requested candidate.
        candidate: ChronoStamp,
        /// The authoritative version that blocks it.
        latest: ChronoStamp,
    },
    /// Construction failed after calculation.
    #[error(transparent)]
    InvalidVersion(ValidationError),
}

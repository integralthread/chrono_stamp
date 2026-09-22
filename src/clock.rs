use time::OffsetDateTime;

use crate::{ValidationError, YearMonth};

/// A source of the current UTC calendar month.
pub trait Clock {
    /// Returns the current UTC year and month.
    fn year_month(&self) -> YearMonth;
}

/// The production UTC system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn year_month(&self) -> YearMonth {
        let now = OffsetDateTime::now_utc();
        let year = u16::try_from(now.year()).expect("system UTC year must be positive and fit u16");
        let month = u8::from(now.month());
        YearMonth::new(year, month).expect("system UTC date must be a valid ChronoStamp month")
    }
}

/// A deterministic clock for tests, previews, and reproducible automation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedClock {
    year_month: YearMonth,
}

impl FixedClock {
    /// Creates a fixed clock.
    ///
    /// # Errors
    ///
    /// Returns an error when the year or month is outside the supported range.
    pub fn new(year: u16, month: u8) -> Result<Self, ValidationError> {
        Ok(Self {
            year_month: YearMonth::new(year, month)?,
        })
    }
}

impl Clock for FixedClock {
    fn year_month(&self) -> YearMonth {
        self.year_month
    }
}

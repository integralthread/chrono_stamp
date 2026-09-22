//! Calendar-anchored versioning primitives.

#![forbid(unsafe_code)]

pub mod cli;
mod clock;
mod config;
mod git;
mod model;
mod next;
mod output;
mod parse;
mod project;
mod update;

pub use clock::{Clock, FixedClock, SystemClock};
pub use model::{
    ChronoStamp, GitHash, Month, Stability, ValidationError, VersionKind, Year, YearMonth,
};
pub use next::{NextVersionError, NextVersionOptions, latest_future_version, next_version};
pub use parse::ParseError;

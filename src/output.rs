use std::io::{self, Write};

use serde::Serialize;
use thiserror::Error;
use usage::ValueEnum;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

pub fn json<T: Serialize>(value: &T) -> Result<(), OutputError> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)?;
    writeln!(lock)?;
    Ok(())
}

pub fn line(value: impl std::fmt::Display) -> Result<(), OutputError> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    writeln!(lock, "{value}")?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("cannot encode JSON output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cannot write output: {0}")]
    Io(#[from] io::Error),
}

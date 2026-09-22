use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use thiserror::Error;

use crate::{ChronoStamp, GitHash, ValidationError};

#[derive(Clone, Debug)]
pub struct Git {
    root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionTag {
    pub name: String,
    pub version: ChronoStamp,
    pub created_at: Option<String>,
    pub subject: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidVersionTag {
    pub name: String,
    pub error: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagCatalog {
    pub tags: Vec<VersionTag>,
    pub invalid: Vec<InvalidVersionTag>,
}

impl Git {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn is_repository(&self) -> bool {
        self.output(&["rev-parse", "--is-inside-work-tree"])
            .is_ok_and(|value| value.trim() == "true")
    }

    pub fn hash(&self, length: usize) -> Result<GitHash, GitError> {
        if !(7..=16).contains(&length) {
            return Err(GitError::InvalidHashLength(length));
        }
        let full = self.output(&["rev-parse", "HEAD"])?;
        let full = full.trim();
        let abbreviated = full
            .get(..length)
            .ok_or_else(|| GitError::UnexpectedOutput("rev-parse HEAD".to_owned()))?;
        GitHash::new(abbreviated.to_owned()).map_err(GitError::InvalidHash)
    }

    pub fn branch(&self) -> Result<Option<String>, GitError> {
        let output = self.raw(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        if output.status.success() {
            Ok(Some(text(&output)?.trim().to_owned()))
        } else if output.status.code() == Some(1) {
            Ok(None)
        } else {
            Err(command_failure(
                &["symbolic-ref", "--quiet", "--short", "HEAD"],
                &output,
            ))
        }
    }

    pub fn is_dirty(&self) -> Result<bool, GitError> {
        Ok(!self.output(&["status", "--porcelain=v1"])?.is_empty())
    }

    pub fn dirty_paths(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>, GitError> {
        let relatives = self.relative_paths(paths)?;
        let mut dirty = Vec::new();
        for (path, relative) in paths.iter().zip(relatives) {
            let output = self.raw_owned(
                &["status", "--porcelain=v1", "--"],
                std::slice::from_ref(&relative),
            )?;
            if !output.status.success() {
                return Err(command_failure(
                    &["status", "--porcelain=v1", "--", "<managed path>"],
                    &output,
                ));
            }
            if !output.stdout.is_empty() {
                dirty.push(path.clone());
            }
        }
        Ok(dirty)
    }

    pub fn tags(&self, prefix: &str) -> Result<TagCatalog, GitError> {
        let output = self.output(&[
            "for-each-ref",
            "--format=%(refname:short)%09%(creatordate:iso-strict)%09%(contents:subject)",
            "refs/tags",
        ])?;
        let mut tags = Vec::new();
        let mut invalid = Vec::new();
        for line in output.lines() {
            let mut fields = line.splitn(3, '\t');
            let Some(name) = fields.next() else {
                continue;
            };
            let Some(value) = name.strip_prefix(prefix) else {
                continue;
            };
            match value.parse() {
                Ok(version) => {
                    let created_at = fields
                        .next()
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned);
                    let subject = fields
                        .next()
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned);
                    tags.push(VersionTag {
                        name: name.to_owned(),
                        version,
                        created_at,
                        subject,
                    });
                }
                Err(error) => invalid.push(InvalidVersionTag {
                    name: name.to_owned(),
                    error: error.to_string(),
                }),
            }
        }
        tags.sort_by(|left, right| right.version.cmp(&left.version));
        invalid.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(TagCatalog { tags, invalid })
    }

    pub fn tag_exists(&self, name: &str) -> Result<bool, GitError> {
        let reference = format!("refs/tags/{name}");
        let output = self.raw_owned(&["show-ref", "--verify", "--quiet", &reference], &[])?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(command_failure(
                &["show-ref", "--verify", "--quiet", &reference],
                &output,
            )),
        }
    }

    pub fn create_tag(&self, name: &str, message: &str, sign: bool) -> Result<(), GitError> {
        let mode = if sign { "-s" } else { "-a" };
        self.output(&["tag", mode, name, "-m", message])?;
        Ok(())
    }

    pub fn add_paths(&self, paths: &[PathBuf]) -> Result<(), GitError> {
        let relatives = self.relative_paths(paths)?;
        let output = self.raw_owned(&["add", "--"], &relatives)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_failure(&["add", "--", "<managed paths>"], &output))
        }
    }

    fn relative_paths(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>, GitError> {
        paths
            .iter()
            .map(|path| {
                path.strip_prefix(&self.root)
                    .map(Path::to_path_buf)
                    .map_err(|_| GitError::OutsideRepository(path.clone()))
            })
            .collect()
    }

    pub fn commit(&self, message: &str) -> Result<(), GitError> {
        self.output(&["commit", "-m", message])?;
        Ok(())
    }

    pub fn push_head_and_tag(&self, remote: &str, tag: &str) -> Result<(), GitError> {
        self.output(&["push", remote, "HEAD", tag])?;
        Ok(())
    }

    fn output(&self, args: &[&str]) -> Result<String, GitError> {
        let output = self.raw(args)?;
        if !output.status.success() {
            return Err(command_failure(args, &output));
        }
        Ok(text(&output)?.trim_end().to_owned())
    }

    fn raw(&self, args: &[&str]) -> Result<Output, GitError> {
        self.raw_owned(args, &[])
    }

    fn raw_owned(&self, args: &[&str], trailing: &[PathBuf]) -> Result<Output, GitError> {
        Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .args(trailing)
            .output()
            .map_err(|source| GitError::Spawn {
                root: self.root.clone(),
                source,
            })
    }
}

fn text(output: &Output) -> Result<String, GitError> {
    String::from_utf8(output.stdout.clone()).map_err(GitError::NonUtf8)
}

fn command_failure(args: &[&str], output: &Output) -> GitError {
    GitError::Command {
        command: format!("git {}", args.join(" ")),
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("failed to run Git in {root}: {source}")]
    Spawn {
        root: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{command} failed with status {status:?}: {stderr}")]
    Command {
        command: String,
        status: Option<i32>,
        stderr: String,
    },
    #[error("Git returned non-UTF-8 output: {0}")]
    NonUtf8(#[from] std::string::FromUtf8Error),
    #[error("Git returned unexpected output for {0}")]
    UnexpectedOutput(String),
    #[error("hash length must be between 7 and 16, got {0}")]
    InvalidHashLength(usize),
    #[error("Git returned an invalid hash: {0}")]
    InvalidHash(ValidationError),
    #[error("managed path is outside the Git repository: {0}")]
    OutsideRepository(PathBuf),
}

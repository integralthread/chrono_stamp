use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Serialize;
use thiserror::Error;
use toml_edit::{DocumentMut, Item};

use crate::ChronoStamp;
use crate::config::{UpdateKind, UpdateTarget};
use crate::project::{Project, SourceKind};

#[derive(Clone, Debug, Serialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub from: ChronoStamp,
    pub to: ChronoStamp,
    #[serde(skip)]
    before: Vec<u8>,
    #[serde(skip)]
    after: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdatePlan {
    pub changes: Vec<FileChange>,
}

impl UpdatePlan {
    pub fn for_project(
        project: &Project,
        next: &ChronoStamp,
        include_primary: bool,
        require_consistency: bool,
    ) -> Result<Self, UpdateError> {
        let mut changes = Vec::new();
        let mut seen = HashSet::new();

        if include_primary {
            let change = primary_change(project, next)?;
            seen.insert(project.source_path.clone());
            changes.push(change);
            if let Some(lock_path) = project.cargo_lock_path()
                && seen.insert(lock_path.clone())
            {
                changes.push(cargo_lock_change(project, &lock_path, next)?);
            }
        }

        for target in &project.config.config.updates {
            let path = project.config.resolve(&target.path)?;
            if !seen.insert(path.clone()) {
                continue;
            }
            changes.push(target_change(
                &project.root,
                &path,
                target,
                &project.version,
                next,
                require_consistency,
            )?);
        }

        Ok(Self { changes })
    }

    pub fn apply(&self) -> Result<(), UpdateError> {
        if self.changes.is_empty() {
            return Ok(());
        }

        for change in &self.changes {
            let current = fs::read(&change.path).map_err(|source| UpdateError::Io {
                path: change.path.clone(),
                source,
            })?;
            if current != change.before {
                return Err(UpdateError::ChangedSincePlanning(change.path.clone()));
            }
        }

        let nonce = format!("{}", std::process::id());
        let mut staged = Vec::with_capacity(self.changes.len());
        for (index, change) in self.changes.iter().enumerate() {
            match stage_file(change, &nonce, index) {
                Ok(paths) => staged.push(paths),
                Err(error) => {
                    cleanup_staged(&staged);
                    return Err(error);
                }
            }
        }

        for (installed, (original, temporary, backup)) in staged.iter().enumerate() {
            if let Err(source) = fs::rename(original, backup) {
                rollback(&staged[..installed]);
                cleanup_staged(&staged);
                return Err(UpdateError::Io {
                    path: original.clone(),
                    source,
                });
            }
            if let Err(source) = fs::rename(temporary, original) {
                let _ = fs::rename(backup, original);
                rollback(&staged[..installed]);
                cleanup_staged(&staged);
                return Err(UpdateError::Transaction {
                    path: original.clone(),
                    source,
                });
            }
        }

        for (_, _, backup) in &staged {
            fs::remove_file(backup).map_err(|source| UpdateError::Io {
                path: backup.clone(),
                source,
            })?;
        }
        Ok(())
    }

    pub fn restore(&self) -> Result<(), UpdateError> {
        let reversed = Self {
            changes: self
                .changes
                .iter()
                .map(|change| FileChange {
                    path: change.path.clone(),
                    from: change.to.clone(),
                    to: change.from.clone(),
                    before: change.after.clone(),
                    after: change.before.clone(),
                })
                .collect(),
        };
        reversed.apply()
    }
}

fn stage_file(
    change: &FileChange,
    nonce: &str,
    index: usize,
) -> Result<(PathBuf, PathBuf, PathBuf), UpdateError> {
    let temporary = sibling(&change.path, &format!("chrono-tmp-{nonce}-{index}"))?;
    let backup = sibling(&change.path, &format!("chrono-backup-{nonce}-{index}"))?;
    ensure_absent(&temporary)?;
    ensure_absent(&backup)?;
    let permissions = fs::metadata(&change.path)
        .map_err(|source| UpdateError::Io {
            path: change.path.clone(),
            source,
        })?
        .permissions();
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| UpdateError::Io {
                path: temporary.clone(),
                source,
            })?;
        file.write_all(&change.after)
            .and_then(|()| file.sync_all())
            .map_err(|source| UpdateError::Io {
                path: temporary.clone(),
                source,
            })?;
        fs::set_permissions(&temporary, permissions).map_err(|source| UpdateError::Io {
            path: temporary.clone(),
            source,
        })?;
        Ok((change.path.clone(), temporary.clone(), backup))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn read_target_versions(
    root: &Path,
    path: &Path,
    target: &UpdateTarget,
) -> Result<Vec<ChronoStamp>, UpdateError> {
    validate_target(root, path)?;
    let source = read_string(path)?;
    let values = match target.kind {
        UpdateKind::Toml => {
            let document: toml::Value = toml::from_str(&source).map_err(UpdateError::TomlRead)?;
            vec![lookup_toml(&document, target.key.as_deref().unwrap_or_default())?.to_owned()]
        }
        UpdateKind::Json => {
            let document: serde_json::Value =
                serde_json::from_str(&source).map_err(UpdateError::Json)?;
            vec![lookup_json(&document, target.key.as_deref().unwrap_or_default())?.to_owned()]
        }
        UpdateKind::Regex => regex_values(path, &source, target)?,
    };
    parse_versions(path, values)
}

fn primary_change(project: &Project, next: &ChronoStamp) -> Result<FileChange, UpdateError> {
    validate_target(&project.root, &project.source_path)?;
    let before = fs::read(&project.source_path).map_err(|source| UpdateError::Io {
        path: project.source_path.clone(),
        source,
    })?;
    let source = std::str::from_utf8(&before).map_err(|source| UpdateError::Utf8 {
        path: project.source_path.clone(),
        source,
    })?;
    let after = match project.source_kind {
        SourceKind::Cargo => update_toml(source, &project.source_key, next)?,
        SourceKind::PackageJson => update_json(source, "version", next)?,
        SourceKind::Mix => {
            let target = UpdateTarget {
                path: project.source_path.clone(),
                kind: UpdateKind::Regex,
                key: None,
                pattern: Some(r#"(?m)(?<prefix>version:\s*")[^"]+(?<suffix>")"#.to_owned()),
                expected_matches: 1,
            };
            update_regex(&project.source_path, source, &target, next)?
        }
        SourceKind::Text => {
            let newline = if source.ends_with('\n') { "\n" } else { "" };
            format!("{next}{newline}")
        }
    };
    Ok(FileChange {
        path: project.source_path.clone(),
        from: project.version.clone(),
        to: next.clone(),
        before,
        after: after.into_bytes(),
    })
}

fn cargo_lock_change(
    project: &Project,
    path: &Path,
    next: &ChronoStamp,
) -> Result<FileChange, UpdateError> {
    validate_target(&project.root, path)?;
    let before = fs::read(path).map_err(|source| UpdateError::Io {
        path: path.to_owned(),
        source,
    })?;
    let source = std::str::from_utf8(&before).map_err(|source| UpdateError::Utf8 {
        path: path.to_owned(),
        source,
    })?;
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(UpdateError::TomlEdit)?;
    let packages = document
        .get_mut("package")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or(UpdateError::InvalidCargoLock)?;
    let current = project.version.to_string();
    let mut updated = HashSet::new();
    for package in packages.iter_mut() {
        let Some(name) = package.get("name").and_then(Item::as_str) else {
            continue;
        };
        if project
            .lock_package_names
            .iter()
            .any(|candidate| candidate == name)
            && package.get("source").is_none()
            && package.get("version").and_then(Item::as_str) == Some(current.as_str())
        {
            updated.insert(name.to_owned());
            package["version"] = toml_edit::value(next.to_string());
        }
    }
    let missing = project
        .lock_package_names
        .iter()
        .filter(|name| !updated.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(UpdateError::CargoLockPackages(missing));
    }
    Ok(FileChange {
        path: path.to_owned(),
        from: project.version.clone(),
        to: next.clone(),
        before,
        after: document.to_string().into_bytes(),
    })
}

fn target_change(
    root: &Path,
    path: &Path,
    target: &UpdateTarget,
    expected: &ChronoStamp,
    next: &ChronoStamp,
    require_consistency: bool,
) -> Result<FileChange, UpdateError> {
    let versions = read_target_versions(root, path, target)?;
    if require_consistency {
        for version in &versions {
            if version != expected {
                return Err(UpdateError::VersionMismatch {
                    path: path.to_owned(),
                    expected: expected.clone(),
                    actual: version.clone(),
                });
            }
        }
    }
    let before = fs::read(path).map_err(|source| UpdateError::Io {
        path: path.to_owned(),
        source,
    })?;
    let source = std::str::from_utf8(&before).map_err(|source| UpdateError::Utf8 {
        path: path.to_owned(),
        source,
    })?;
    let after = match target.kind {
        UpdateKind::Toml => update_toml(source, target.key.as_deref().unwrap_or_default(), next)?,
        UpdateKind::Json => update_json(source, target.key.as_deref().unwrap_or_default(), next)?,
        UpdateKind::Regex => update_regex(path, source, target, next)?,
    };
    Ok(FileChange {
        path: path.to_owned(),
        from: versions
            .first()
            .cloned()
            .unwrap_or_else(|| expected.clone()),
        to: next.clone(),
        before,
        after: after.into_bytes(),
    })
}

fn update_toml(source: &str, key: &str, next: &ChronoStamp) -> Result<String, UpdateError> {
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(UpdateError::TomlEdit)?;
    let mut item: &mut Item = document.as_item_mut();
    for part in key.split('.') {
        item = item
            .get_mut(part)
            .ok_or_else(|| UpdateError::MissingKey(key.to_owned()))?;
    }
    if !item.is_str() {
        return Err(UpdateError::MissingKey(key.to_owned()));
    }
    *item = toml_edit::value(next.to_string());
    Ok(document.to_string())
}

fn update_json(source: &str, key: &str, next: &ChronoStamp) -> Result<String, UpdateError> {
    let mut document: serde_json::Value =
        serde_json::from_str(source).map_err(UpdateError::Json)?;
    let mut value = &mut document;
    for part in key.split('.') {
        value = value
            .get_mut(part)
            .ok_or_else(|| UpdateError::MissingKey(key.to_owned()))?;
    }
    if !value.is_string() {
        return Err(UpdateError::MissingKey(key.to_owned()));
    }
    *value = serde_json::Value::String(next.to_string());
    let mut rendered = serde_json::to_string_pretty(&document).map_err(UpdateError::Json)?;
    rendered.push('\n');
    Ok(rendered)
}

fn update_regex(
    path: &Path,
    source: &str,
    target: &UpdateTarget,
    next: &ChronoStamp,
) -> Result<String, UpdateError> {
    let mut rendered = source.to_owned();
    for (start, end) in checked_ranges(path, source, target)?.into_iter().rev() {
        rendered.replace_range(start..end, &next.to_string());
    }
    Ok(rendered)
}

fn regex_values(
    path: &Path,
    source: &str,
    target: &UpdateTarget,
) -> Result<Vec<String>, UpdateError> {
    Ok(checked_ranges(path, source, target)?
        .into_iter()
        .map(|(start, end)| source[start..end].to_owned())
        .collect())
}

fn checked_ranges(
    path: &Path,
    source: &str,
    target: &UpdateTarget,
) -> Result<Vec<(usize, usize)>, UpdateError> {
    let regex = Regex::new(target.pattern.as_deref().unwrap_or_default())?;
    let ranges = replacement_ranges(&regex, source)?;
    if ranges.len() != target.expected_matches {
        return Err(UpdateError::MatchCount {
            path: path.to_owned(),
            expected: target.expected_matches,
            actual: ranges.len(),
        });
    }
    Ok(ranges)
}

fn replacement_ranges(regex: &Regex, source: &str) -> Result<Vec<(usize, usize)>, UpdateError> {
    regex
        .captures_iter(source)
        .map(|captures| {
            if let Some(version) = captures.name("version") {
                return Ok((version.start(), version.end()));
            }
            let prefix = captures.name("prefix").ok_or(UpdateError::RegexCaptures)?;
            let suffix = captures.name("suffix").ok_or(UpdateError::RegexCaptures)?;
            if prefix.end() > suffix.start() {
                return Err(UpdateError::RegexCaptures);
            }
            Ok((prefix.end(), suffix.start()))
        })
        .collect()
}

fn parse_versions(path: &Path, values: Vec<String>) -> Result<Vec<ChronoStamp>, UpdateError> {
    values
        .into_iter()
        .map(|value| {
            value.parse().map_err(|source| UpdateError::InvalidVersion {
                path: path.to_owned(),
                value,
                source,
            })
        })
        .collect()
}

fn lookup_toml<'a>(value: &'a toml::Value, key: &str) -> Result<&'a str, UpdateError> {
    key.split('.')
        .try_fold(value, |current, part| {
            current
                .get(part)
                .ok_or_else(|| UpdateError::MissingKey(key.to_owned()))
        })?
        .as_str()
        .ok_or_else(|| UpdateError::MissingKey(key.to_owned()))
}

fn lookup_json<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str, UpdateError> {
    key.split('.')
        .try_fold(value, |current, part| {
            current
                .get(part)
                .ok_or_else(|| UpdateError::MissingKey(key.to_owned()))
        })?
        .as_str()
        .ok_or_else(|| UpdateError::MissingKey(key.to_owned()))
}

fn validate_target(root: &Path, path: &Path) -> Result<(), UpdateError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| UpdateError::Io {
        path: path.to_owned(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(UpdateError::Symlink(path.to_owned()));
    }
    let canonical_root = root.canonicalize().map_err(|source| UpdateError::Io {
        path: root.to_owned(),
        source,
    })?;
    let canonical_path = path.canonicalize().map_err(|source| UpdateError::Io {
        path: path.to_owned(),
        source,
    })?;
    if !canonical_path.starts_with(canonical_root) {
        return Err(UpdateError::OutsideProject(path.to_owned()));
    }
    Ok(())
}

fn read_string(path: &Path) -> Result<String, UpdateError> {
    fs::read_to_string(path).map_err(|source| UpdateError::Io {
        path: path.to_owned(),
        source,
    })
}

fn sibling(path: &Path, suffix: &str) -> Result<PathBuf, UpdateError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| UpdateError::InvalidPath(path.to_owned()))?;
    let mut name = file_name.to_os_string();
    name.push(format!(".{suffix}"));
    Ok(path.with_file_name(name))
}

fn ensure_absent(path: &Path) -> Result<(), UpdateError> {
    if path.exists() {
        return Err(UpdateError::TemporaryExists(path.to_owned()));
    }
    Ok(())
}

fn rollback(installed: &[(PathBuf, PathBuf, PathBuf)]) {
    for (original, _, backup) in installed.iter().rev() {
        let _ = fs::remove_file(original);
        let _ = fs::rename(backup, original);
    }
}

fn cleanup_staged(staged: &[(PathBuf, PathBuf, PathBuf)]) {
    for (_, temporary, _) in staged {
        let _ = fs::remove_file(temporary);
    }
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error("cannot access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not UTF-8 text: {source}")]
    Utf8 {
        path: PathBuf,
        #[source]
        source: std::str::Utf8Error,
    },
    #[error("refusing to update symlink {0}")]
    Symlink(PathBuf),
    #[error("refusing to update a path outside the project: {0}")]
    OutsideProject(PathBuf),
    #[error("invalid update path: {0}")]
    InvalidPath(PathBuf),
    #[error("temporary transaction path already exists: {0}")]
    TemporaryExists(PathBuf),
    #[error("{0} changed after the update was planned; nothing was written")]
    ChangedSincePlanning(PathBuf),
    #[error("configured key not found or not a string: {0}")]
    MissingKey(String),
    #[error("invalid TOML target: {0}")]
    TomlRead(toml::de::Error),
    #[error("cannot edit TOML target: {0}")]
    TomlEdit(toml_edit::TomlError),
    #[error("invalid JSON target: {0}")]
    Json(serde_json::Error),
    #[error("Cargo.lock does not contain a package table array")]
    InvalidCargoLock,
    #[error("Cargo.lock is missing current entries for package(s): {0:?}")]
    CargoLockPackages(Vec<String>),
    #[error("invalid configured regular expression: {0}")]
    Regex(#[from] regex::Error),
    #[error(
        "regex targets require either a `version` capture or ordered `prefix` and `suffix` captures"
    )]
    RegexCaptures,
    #[error("{path} matched {actual} time(s); expected {expected}")]
    MatchCount {
        path: PathBuf,
        expected: usize,
        actual: usize,
    },
    #[error("invalid ChronoStamp {value:?} in {path}: {source}")]
    InvalidVersion {
        path: PathBuf,
        value: String,
        source: crate::ParseError,
    },
    #[error("version mismatch in {path}: expected {expected}, found {actual}")]
    VersionMismatch {
        path: PathBuf,
        expected: ChronoStamp,
        actual: ChronoStamp,
    },
    #[error(
        "update failed while installing {path}; original files were restored where possible: {source}"
    )]
    Transaction {
        path: PathBuf,
        source: std::io::Error,
    },
}

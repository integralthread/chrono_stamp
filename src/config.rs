use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

/// The declarative project configuration stored in `.chrono.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub version_source: PathBuf,
    pub git: GitConfig,
    pub dev: DevConfig,
    pub updates: Vec<UpdateTarget>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            version_source: PathBuf::from("Cargo.toml"),
            git: GitConfig::default(),
            dev: DevConfig::default(),
            updates: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
    pub tag_prefix: String,
    pub remote: String,
    pub sign_tags: bool,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            tag_prefix: "v".to_owned(),
            remote: "origin".to_owned(),
            sign_tags: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevConfig {
    pub hash_length: usize,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self { hash_length: 7 }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateKind {
    Toml,
    Json,
    Regex,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTarget {
    pub path: PathBuf,
    pub kind: UpdateKind,
    pub key: Option<String>,
    pub pattern: Option<String>,
    #[serde(default = "one")]
    pub expected_matches: usize,
}

const fn one() -> usize {
    1
}

/// A validated config together with its project-relative location.
#[derive(Clone, Debug)]
pub struct LoadedConfig {
    pub root: PathBuf,
    pub path: Option<PathBuf>,
    pub config: Config,
}

impl LoadedConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let path = path.canonicalize().map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let root = path
            .parent()
            .ok_or_else(|| ConfigError::NoParent(path.clone()))?
            .to_owned();
        let source = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config: Config = toml::from_str(&source).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        validate(&config)?;
        Ok(Self {
            root,
            path: Some(path),
            config,
        })
    }

    pub fn implicit(root: PathBuf, version_source: PathBuf) -> Self {
        let config = Config {
            version_source,
            ..Config::default()
        };
        Self {
            root,
            path: None,
            config,
        }
    }

    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, ConfigError> {
        validate_relative_path(relative)?;
        Ok(self.root.join(relative))
    }
}

fn validate(config: &Config) -> Result<(), ConfigError> {
    if config.schema != 1 {
        return Err(ConfigError::Schema(config.schema));
    }
    validate_relative_path(&config.version_source)?;
    if !(7..=16).contains(&config.dev.hash_length) {
        return Err(ConfigError::HashLength(config.dev.hash_length));
    }

    let mut paths = HashSet::new();
    for target in &config.updates {
        validate_relative_path(&target.path)?;
        if !paths.insert(target.path.clone()) {
            return Err(ConfigError::DuplicateTarget(target.path.clone()));
        }
        if target.expected_matches == 0 {
            return Err(ConfigError::ZeroMatches(target.path.clone()));
        }
        match target.kind {
            UpdateKind::Toml | UpdateKind::Json
                if target.key.as_deref().is_none_or(str::is_empty) =>
            {
                return Err(ConfigError::MissingKey(target.path.clone()));
            }
            UpdateKind::Regex if target.pattern.as_deref().is_none_or(str::is_empty) => {
                return Err(ConfigError::MissingPattern(target.path.clone()));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(ConfigError::UnsafePath(path.to_owned()));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(ConfigError::UnsafePath(path.to_owned()));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read configuration {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid configuration {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("configuration path has no parent: {0}")]
    NoParent(PathBuf),
    #[error("unsupported configuration schema {0}; expected schema 1")]
    Schema(u32),
    #[error("dev.hash_length must be between 7 and 16, got {0}")]
    HashLength(usize),
    #[error("configured path must stay within the project: {0}")]
    UnsafePath(PathBuf),
    #[error("duplicate update target: {0}")]
    DuplicateTarget(PathBuf),
    #[error("expected_matches must be at least 1 for {0}")]
    ZeroMatches(PathBuf),
    #[error("update target {0} requires a key")]
    MissingKey(PathBuf),
    #[error("update target {0} requires a pattern")]
    MissingPattern(PathBuf),
}

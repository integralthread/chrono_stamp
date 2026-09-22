use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as JsonValue;
use thiserror::Error;
use toml::Value as TomlValue;

use crate::ChronoStamp;
use crate::config::{ConfigError, LoadedConfig};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    Cargo,
    Mix,
    PackageJson,
    Text,
}

impl SourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Mix => "mix",
            Self::PackageJson => "package_json",
            Self::Text => "text",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub declaration_path: PathBuf,
    pub source_path: PathBuf,
    pub source_key: String,
    pub source_kind: SourceKind,
    pub lock_package_names: Vec<String>,
    pub version: ChronoStamp,
    pub config: LoadedConfig,
}

impl Project {
    pub fn discover(
        start: &Path,
        project: Option<&Path>,
        config: Option<&Path>,
    ) -> Result<Self, ProjectError> {
        let loaded = if let Some(config_path) = config {
            LoadedConfig::load(&absolutize(start, config_path))?
        } else if let Some(project_path) = project {
            let root = canonical_dir(&absolutize(start, project_path))?;
            let config_path = root.join(".chrono.toml");
            if config_path.is_file() {
                LoadedConfig::load(&config_path)?
            } else {
                implicit_from_root(root)?
            }
        } else {
            discover_upward(start)?
        };

        Self::from_config_with_policy(loaded, project.is_some() || config.is_some())
    }

    fn from_config_with_policy(
        config: LoadedConfig,
        allow_non_git_parents: bool,
    ) -> Result<Self, ProjectError> {
        let declaration_path = config.resolve(&config.config.version_source)?;
        let source_kind = source_kind(&declaration_path);
        let source = read_source(
            &declaration_path,
            source_kind,
            &config.root,
            allow_non_git_parents,
        )?;
        let root = source
            .path
            .parent()
            .filter(|owner| config.root.starts_with(owner))
            .map_or_else(|| config.root.clone(), Path::to_path_buf);
        let lock_package_names =
            if source_kind == SourceKind::Cargo && root.join("Cargo.lock").is_file() {
                cargo_lock_package_names(&declaration_path, &source.key, &source.name)?
            } else {
                vec![source.name.clone()]
            };
        Ok(Self {
            root,
            name: source.name,
            declaration_path,
            source_path: source.path,
            source_key: source.key,
            source_kind,
            lock_package_names,
            version: source.version,
            config,
        })
    }

    pub fn cargo_lock_path(&self) -> Option<PathBuf> {
        (self.source_kind == SourceKind::Cargo)
            .then(|| self.root.join("Cargo.lock"))
            .filter(|path| path.is_file())
    }

    pub fn refresh_and_validate_cargo_lock(&self) -> Result<(), ProjectError> {
        if self.source_kind != SourceKind::Cargo || self.cargo_lock_path().is_none() {
            return Ok(());
        }
        let refresh = cargo_metadata(&self.declaration_path, false)?;
        if !refresh.status.success() {
            return Err(ProjectError::CargoRegeneration(stderr_text(&refresh)));
        }
        let validation = cargo_metadata(&self.declaration_path, true)?;
        if !validation.status.success() {
            return Err(ProjectError::CargoValidation(stderr_text(&validation)));
        }
        Ok(())
    }
}

fn cargo_metadata(manifest_path: &Path, locked: bool) -> Result<Output, ProjectError> {
    let mut command = Command::new("cargo");
    command.arg("metadata");
    if locked {
        command.arg("--locked");
    }
    command
        .args(["--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(manifest_path)
        .output()
        .map_err(ProjectError::CargoSpawn)
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

fn cargo_lock_package_names(
    declaration_path: &Path,
    source_key: &str,
    selected_name: &str,
) -> Result<Vec<String>, ProjectError> {
    if source_key != "workspace.package.version" {
        return Ok(vec![selected_name.to_owned()]);
    }

    let output = cargo_metadata(declaration_path, true)?;
    if !output.status.success() {
        return Err(ProjectError::CargoValidation(stderr_text(&output)));
    }
    let metadata: JsonValue =
        serde_json::from_slice(&output.stdout).map_err(ProjectError::CargoMetadataJson)?;
    let packages = metadata
        .get("packages")
        .and_then(JsonValue::as_array)
        .ok_or(ProjectError::CargoMetadataShape)?;
    let mut names = Vec::new();
    for package in packages {
        let Some(name) = package.get("name").and_then(JsonValue::as_str) else {
            return Err(ProjectError::CargoMetadataShape);
        };
        let Some(manifest_path) = package.get("manifest_path").and_then(JsonValue::as_str) else {
            return Err(ProjectError::CargoMetadataShape);
        };
        let manifest = fs::read_to_string(manifest_path).map_err(|source| ProjectError::Read {
            path: PathBuf::from(manifest_path),
            source,
        })?;
        let document: TomlValue = toml::from_str(&manifest).map_err(ProjectError::Toml)?;
        let inherited = document
            .get("package")
            .and_then(|value| value.get("version"))
            .and_then(TomlValue::as_table)
            .and_then(|table| table.get("workspace"))
            .and_then(TomlValue::as_bool)
            == Some(true);
        if inherited {
            names.push(name.to_owned());
        }
    }
    if names.is_empty() {
        return Err(ProjectError::MissingField(
            "workspace members inheriting package.version",
        ));
    }
    names.sort();
    names.dedup();
    Ok(names)
}

struct ResolvedSource {
    name: String,
    version: ChronoStamp,
    path: PathBuf,
    key: String,
}

fn absolutize(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}

fn canonical_dir(path: &Path) -> Result<PathBuf, ProjectError> {
    let canonical = path.canonicalize().map_err(|source| ProjectError::Read {
        path: path.to_owned(),
        source,
    })?;
    if !canonical.is_dir() {
        return Err(ProjectError::NotDirectory(canonical));
    }
    Ok(canonical)
}

fn discover_upward(start: &Path) -> Result<LoadedConfig, ProjectError> {
    let start = canonical_dir(start)?;
    for root in implicit_search_roots(&start, false) {
        let explicit = root.join(".chrono.toml");
        if explicit.is_file() {
            return Ok(LoadedConfig::load(&explicit)?);
        }
        if let Some(source) = detected_source(&root) {
            return Ok(LoadedConfig::implicit(root, source));
        }
    }
    Err(ProjectError::NotFound(start))
}

fn implicit_search_roots(start: &Path, allow_non_git_parents: bool) -> Vec<PathBuf> {
    let boundary = start.ancestors().find(|root| root.join(".git").exists());
    let Some(boundary) = boundary else {
        return if allow_non_git_parents {
            start.ancestors().map(Path::to_path_buf).collect()
        } else {
            vec![start.to_owned()]
        };
    };

    let mut roots = Vec::new();
    for root in start.ancestors() {
        roots.push(root.to_owned());
        if root == boundary {
            break;
        }
    }
    roots
}

fn implicit_from_root(root: PathBuf) -> Result<LoadedConfig, ProjectError> {
    let source =
        detected_source(&root).ok_or_else(|| ProjectError::NoVersionSource(root.clone()))?;
    Ok(LoadedConfig::implicit(root, source))
}

fn detected_source(root: &Path) -> Option<PathBuf> {
    ["Cargo.toml", "mix.exs", "package.json"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| root.join(path).is_file())
}

fn source_kind(path: &Path) -> SourceKind {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("Cargo.toml") => SourceKind::Cargo,
        Some("mix.exs") => SourceKind::Mix,
        Some("package.json") => SourceKind::PackageJson,
        _ => SourceKind::Text,
    }
}

fn read_source(
    path: &Path,
    kind: SourceKind,
    root: &Path,
    allow_non_git_parents: bool,
) -> Result<ResolvedSource, ProjectError> {
    let source = fs::read_to_string(path).map_err(|source| ProjectError::Read {
        path: path.to_owned(),
        source,
    })?;
    let (name, version, owner_path, key) = match kind {
        SourceKind::Cargo => read_cargo(&source, path, root, allow_non_git_parents)?,
        SourceKind::Mix => {
            let (name, version) = read_mix(&source)?;
            (name, version, path.to_owned(), "project.version".to_owned())
        }
        SourceKind::PackageJson => {
            let (name, version) = read_package_json(&source)?;
            (name, version, path.to_owned(), "version".to_owned())
        }
        SourceKind::Text => {
            let name = root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
                .to_owned();
            (
                name,
                source.trim().to_owned(),
                path.to_owned(),
                String::new(),
            )
        }
    };
    let version = version
        .parse()
        .map_err(|source| ProjectError::InvalidVersion {
            path: path.to_owned(),
            value: version,
            source,
        })?;
    Ok(ResolvedSource {
        name,
        version,
        path: owner_path,
        key,
    })
}

fn read_cargo(
    source: &str,
    path: &Path,
    root: &Path,
    allow_non_git_parents: bool,
) -> Result<(String, String, PathBuf, String), ProjectError> {
    let document: TomlValue = toml::from_str(source).map_err(ProjectError::Toml)?;
    let package = document.get("package").and_then(TomlValue::as_table);
    if package.is_none()
        && let Some(version) = document
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("version"))
            .and_then(TomlValue::as_str)
    {
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace")
            .to_owned();
        return Ok((
            name,
            version.to_owned(),
            path.to_owned(),
            "workspace.package.version".to_owned(),
        ));
    }
    let package = package.ok_or(ProjectError::MissingField("package"))?;
    let name = package
        .get("name")
        .and_then(TomlValue::as_str)
        .ok_or(ProjectError::MissingField("package.name"))?
        .to_owned();
    let (version, owner_path, key) = match package.get("version") {
        Some(TomlValue::String(value)) => {
            (value.clone(), path.to_owned(), "package.version".to_owned())
        }
        Some(TomlValue::Table(table))
            if table.get("workspace").and_then(TomlValue::as_bool) == Some(true) =>
        {
            read_workspace_version(root, allow_non_git_parents)?
        }
        _ => return Err(ProjectError::MissingField("package.version")),
    };
    Ok((name, version, owner_path, key))
}

fn read_workspace_version(
    start: &Path,
    allow_non_git_parents: bool,
) -> Result<(String, PathBuf, String), ProjectError> {
    for root in implicit_search_roots(start, allow_non_git_parents) {
        let manifest = root.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let source = fs::read_to_string(&manifest).map_err(|source| ProjectError::Read {
            path: manifest.clone(),
            source,
        })?;
        let document = toml::from_str::<TomlValue>(&source).map_err(|source| {
            ProjectError::InvalidWorkspaceManifest {
                path: manifest.clone(),
                source,
            }
        })?;
        if let Some(version) = document
            .get("workspace")
            .and_then(|value| value.get("package"))
            .and_then(|value| value.get("version"))
            .and_then(TomlValue::as_str)
        {
            return Ok((
                version.to_owned(),
                manifest,
                "workspace.package.version".to_owned(),
            ));
        }
    }
    Err(ProjectError::MissingField("workspace.package.version"))
}

fn read_mix(source: &str) -> Result<(String, String), ProjectError> {
    let name = quoted_after(source, "app:").map_or_else(
        || "mix-project".to_owned(),
        |value| value.trim_start_matches(':').to_owned(),
    );
    let version =
        quoted_after(source, "version:").ok_or(ProjectError::MissingField("project version"))?;
    Ok((name, version))
}

fn quoted_after(source: &str, marker: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let tail = line.split_once(marker)?.1.trim_start();
        if marker == "app:" {
            return tail
                .strip_prefix(':')
                .and_then(|value| {
                    value
                        .split(|character: char| {
                            !character.is_ascii_alphanumeric() && character != '_'
                        })
                        .next()
                })
                .map(ToOwned::to_owned);
        }
        let tail = tail.strip_prefix('"')?;
        Some(tail.split_once('"')?.0.to_owned())
    })
}

fn read_package_json(source: &str) -> Result<(String, String), ProjectError> {
    let document: JsonValue = serde_json::from_str(source).map_err(ProjectError::Json)?;
    let name = document
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or("package")
        .to_owned();
    let version = document
        .get("version")
        .and_then(JsonValue::as_str)
        .ok_or(ProjectError::MissingField("version"))?
        .to_owned();
    Ok((name, version))
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("project path is not a directory: {0}")]
    NotDirectory(PathBuf),
    #[error("no ChronoStamp project found from {0}")]
    NotFound(PathBuf),
    #[error("no supported version source found in {0}")]
    NoVersionSource(PathBuf),
    #[error("invalid TOML version source: {0}")]
    Toml(toml::de::Error),
    #[error("invalid JSON version source: {0}")]
    Json(serde_json::Error),
    #[error("failed to run Cargo for lockfile validation: {0}")]
    CargoSpawn(std::io::Error),
    #[error("Cargo.lock validation failed: {0}")]
    CargoValidation(String),
    #[error("Cargo.lock regeneration failed: {0}")]
    CargoRegeneration(String),
    #[error("invalid Cargo metadata JSON: {0}")]
    CargoMetadataJson(serde_json::Error),
    #[error("Cargo metadata has an unsupported response shape")]
    CargoMetadataShape,
    #[error("invalid Cargo workspace manifest {path}: {source}")]
    InvalidWorkspaceManifest {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("version source is missing {0}")]
    MissingField(&'static str),
    #[error("invalid ChronoStamp {value:?} in {path}: {source}")]
    InvalidVersion {
        path: PathBuf,
        value: String,
        #[source]
        source: crate::ParseError,
    },
}

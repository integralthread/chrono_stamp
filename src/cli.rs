use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use time::{Date, format_description};
use usage::{Args, Cli as UsageCli, RunWith, Subcommands, ValueEnum};

use crate::config::ConfigError;
use crate::git::{Git, GitError, TagCatalog, VersionTag};
use crate::output::{self, ColorMode, OutputError, OutputFormat};
use crate::project::{Project, ProjectError};
use crate::update::{UpdateError, UpdatePlan, read_target_versions};
use crate::{
    ChronoStamp, Clock, NextVersionError, NextVersionOptions, Stability, SystemClock, VersionKind,
    YearMonth, latest_future_version, next_version,
};

const JSON_SCHEMA_VERSION: u8 = 2;

/// Manage calendar-anchored `ChronoStamp` versions.
#[derive(UsageCli, Debug)]
#[usage(
    bin = "chrono",
    version = env!("CARGO_PKG_VERSION"),
    completion,
    unknown_flags = "error",
    args_override_self = false
)]
pub struct Cli {
    /// Project directory; disables upward discovery.
    #[usage(long, global, value_hint = usage::ValueHint::DirPath)]
    project: Option<PathBuf>,
    /// Explicit `.chrono.toml` path.
    #[usage(long, global, value_hint = usage::ValueHint::FilePath, extensions("toml"))]
    config: Option<PathBuf>,
    /// Result encoding.
    #[usage(long, global, default = "human", value_enum)]
    format: OutputFormat,
    /// Terminal color policy.
    #[usage(long, global, default = "auto", value_enum)]
    color: ColorMode,
    /// Suppress non-result human output.
    #[usage(long, global)]
    quiet: bool,
    /// Include diagnostic detail in human output.
    #[usage(long, global)]
    verbose: bool,
    #[usage(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommands, Debug)]
#[usage(run_with)]
enum Command {
    Init(Init),
    Status(Status),
    Parse(Parse),
    Compare(Compare),
    Bump(Bump),
    Dev(Dev),
    Check(Check),
    Sync(Sync),
    History(History),
    Tag(Tag),
    Release(Release),
    Completion(Completion),
}

/// Create a starter `.chrono.toml` configuration.
#[derive(Args, Debug)]
#[usage(effect = "write")]
struct Init {
    /// Replace an existing regular config file.
    #[usage(long)]
    force: bool,
    /// Show the destination and content without writing.
    #[usage(long)]
    dry_run: bool,
    /// Initial version for a generic project without a supported source.
    #[usage(long)]
    initial_version: Option<ChronoStamp>,
}

/// Show the managed version and repository state.
#[derive(Args, Debug)]
#[usage(effect = "read", visible_alias = "version")]
struct Status {
    /// Override the current UTC date.
    #[usage(long)]
    date: Option<DateArg>,
    /// Permit versions dated after the supplied date.
    #[usage(long)]
    allow_future: bool,
}

/// Parse and normalize one `ChronoStamp`.
#[derive(Args, Debug)]
#[usage(effect = "read")]
struct Parse {
    version: ChronoStamp,
    /// Print the explicit canonical form.
    #[usage(long, conflicts = "display")]
    canonical: bool,
    /// Print the ordinary display form.
    #[usage(long)]
    display: bool,
}

/// Compare two `ChronoStamp` values.
#[derive(Args, Debug)]
#[usage(effect = "read")]
struct Compare {
    left: ChronoStamp,
    right: ChronoStamp,
}

/// Calculate and write the next project version.
#[derive(Args, Debug)]
#[usage(effect = "write")]
struct Bump {
    /// Stability of the new release.
    #[usage(long, default = "final", value_enum)]
    tag: StabilityChoice,
    /// Use an explicit increment instead of calculating one.
    #[usage(long)]
    increment: Option<u64>,
    /// Override the current UTC date.
    #[usage(long)]
    date: Option<DateArg>,
    /// Permit versions dated after the supplied date.
    #[usage(long)]
    allow_future: bool,
    /// Validate and show changes without writing.
    #[usage(long)]
    dry_run: bool,
    /// Permit updates to managed paths that already differ from Git HEAD.
    #[usage(long)]
    allow_dirty_managed: bool,
}

/// Print a development version derived from UTC and HEAD.
#[derive(Args, Debug)]
#[usage(effect = "write")]
struct Dev {
    /// Abbreviated Git hash length.
    #[usage(long)]
    length: Option<usize>,
    /// Override the current UTC date.
    #[usage(long)]
    date: Option<DateArg>,
    /// Write the development version to configured files.
    #[usage(long)]
    write: bool,
    /// Validate and show file changes without writing.
    #[usage(long)]
    dry_run: bool,
}

/// Verify that configured version files agree.
#[derive(Args, Debug)]
#[usage(effect = "read")]
struct Check;

/// Make configured secondary files match the primary source.
#[derive(Args, Debug)]
#[usage(effect = "write")]
struct Sync {
    /// Validate and show changes without writing.
    #[usage(long)]
    dry_run: bool,
    /// Permit updates to managed paths that already differ from Git HEAD.
    #[usage(long)]
    allow_dirty_managed: bool,
}

/// List matching Git tags in `ChronoStamp` order.
#[derive(Args, Debug)]
#[usage(effect = "read")]
struct History {
    /// Maximum number of tags to show.
    #[usage(long)]
    limit: Option<usize>,
    /// Restrict results to one `YYYY.M` calendar month.
    #[usage(long)]
    month: Option<MonthArg>,
    /// Override the current UTC date for future-tag warnings.
    #[usage(long)]
    date: Option<DateArg>,
}

/// Create an annotated Git tag for the current version.
#[derive(Args, Debug)]
#[usage(effect = "destructive")]
struct Tag {
    /// Annotation message; defaults to `Release VERSION`.
    #[usage(short = 'm', long)]
    message: Option<String>,
    /// Create a signed tag.
    #[usage(long)]
    sign: bool,
    /// Override the configured tag prefix.
    #[usage(long)]
    prefix: Option<String>,
    /// Push the new tag after creating it.
    #[usage(long)]
    push: bool,
    /// Override the configured Git remote.
    #[usage(long)]
    remote: Option<String>,
    /// Validate and show the operation without changing Git.
    #[usage(long)]
    dry_run: bool,
}

/// Bump, commit, tag, and optionally push one release.
#[derive(Args, Debug)]
#[usage(effect = "destructive")]
#[allow(clippy::struct_excessive_bools)]
struct Release {
    /// Stability of the new release.
    #[usage(long, default = "final", value_enum)]
    tag: StabilityChoice,
    /// Override the current UTC date.
    #[usage(long)]
    date: Option<DateArg>,
    /// Permit versions dated after the supplied date.
    #[usage(long)]
    allow_future: bool,
    /// Commit and tag message; defaults to `Release VERSION`.
    #[usage(short = 'm', long)]
    message: Option<String>,
    /// Create a signed tag.
    #[usage(long)]
    sign: bool,
    /// Push HEAD and the new tag.
    #[usage(long)]
    push: bool,
    /// Override the configured Git remote.
    #[usage(long)]
    remote: Option<String>,
    /// Validate every phase without files, commits, tags, or pushes.
    #[usage(long)]
    dry_run: bool,
}

/// Print a shell completion script.
#[derive(Args, Debug)]
#[usage(effect = "read")]
struct Completion {
    #[usage(value_enum)]
    shell: Shell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[allow(clippy::enum_variant_names)]
enum Shell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    #[usage(name = "nushell", aliases = ["nu"])]
    Nushell,
    #[usage(name = "powershell", aliases = ["pwsh"])]
    PowerShell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum StabilityChoice {
    Dev,
    Alpha,
    Beta,
    Rc,
    Final,
}

impl From<StabilityChoice> for Stability {
    fn from(value: StabilityChoice) -> Self {
        match value {
            StabilityChoice::Dev => Self::Dev,
            StabilityChoice::Alpha => Self::Alpha,
            StabilityChoice::Beta => Self::Beta,
            StabilityChoice::Rc => Self::Rc,
            StabilityChoice::Final => Self::Final,
        }
    }
}

impl From<Shell> for usage::complete::Shell {
    fn from(value: Shell) -> Self {
        match value {
            Shell::Bash => Self::Bash,
            Shell::Zsh => Self::Zsh,
            Shell::Fish => Self::Fish,
            Shell::Elvish => Self::Elvish,
            Shell::Nushell => Self::Nu,
            Shell::PowerShell => Self::PowerShell,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DateArg(YearMonth);

impl std::str::FromStr for DateArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let description = format_description::parse_borrowed::<1>("[year]-[month]-[day]")
            .map_err(|error| error.to_string())?;
        let date = Date::parse(value, &description)
            .map_err(|_| "expected a valid date in YYYY-MM-DD form".to_owned())?;
        let year = u16::try_from(date.year())
            .map_err(|_| "date year must be between 0001 and 9999".to_owned())?;
        let month = u8::from(date.month());
        YearMonth::new(year, month)
            .map(Self)
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MonthArg(YearMonth);

impl std::str::FromStr for MonthArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (year, month) = value
            .split_once('.')
            .ok_or_else(|| "expected YYYY.M".to_owned())?;
        if month.starts_with('0') && month.len() > 1 {
            return Err("month must not be zero-padded".to_owned());
        }
        let year = year.parse().map_err(|_| "invalid year".to_owned())?;
        let month = month.parse().map_err(|_| "invalid month".to_owned())?;
        YearMonth::new(year, month)
            .map(Self)
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug)]
struct CliContext {
    start: PathBuf,
    project: Option<PathBuf>,
    config: Option<PathBuf>,
    format: OutputFormat,
    quiet: bool,
    verbose: bool,
}

#[derive(Clone, Debug, Serialize)]
struct AppWarning {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_month: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_month: Option<String>,
}

impl AppWarning {
    fn invalid_tag(name: &str, error: &str) -> Self {
        Self {
            code: "invalid_version_tag",
            message: format!("tag {name:?} uses the configured prefix but is invalid: {error}"),
            tag: Some(name.to_owned()),
            version: None,
            current_month: None,
            effective_month: None,
        }
    }

    fn duplicate_tags(version: &ChronoStamp, names: &[String]) -> Self {
        Self {
            code: "duplicate_version_tags",
            message: format!(
                "multiple tags represent version {version}: {}",
                names.join(", ")
            ),
            tag: None,
            version: Some(version.to_string()),
            current_month: None,
            effective_month: None,
        }
    }

    fn future_version(version: &ChronoStamp, now: YearMonth) -> Self {
        Self {
            code: "future_version",
            message: format!("version {version} is later than current month {now}"),
            tag: None,
            version: Some(version.to_string()),
            current_month: Some(now.to_string()),
            effective_month: None,
        }
    }

    fn future_month_adopted(version: &ChronoStamp, now: YearMonth) -> Self {
        Self {
            code: "future_month_adopted",
            message: format!(
                "using future authoritative month {} instead of {now}",
                version.year_month()
            ),
            tag: None,
            version: Some(version.to_string()),
            current_month: Some(now.to_string()),
            effective_month: Some(version.year_month().to_string()),
        }
    }
}

fn duplicate_tag_versions(catalog: &TagCatalog) -> Vec<(ChronoStamp, Vec<String>)> {
    let mut by_version = BTreeMap::<ChronoStamp, Vec<String>>::new();
    for tag in &catalog.tags {
        by_version
            .entry(tag.version.clone())
            .or_default()
            .push(tag.name.clone());
    }
    by_version
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .collect()
}

fn catalog_warnings(catalog: &TagCatalog) -> Vec<AppWarning> {
    let mut warnings = catalog
        .invalid
        .iter()
        .map(|tag| AppWarning::invalid_tag(&tag.name, &tag.error))
        .collect::<Vec<_>>();
    warnings.extend(
        duplicate_tag_versions(catalog)
            .into_iter()
            .map(|(version, names)| AppWarning::duplicate_tags(&version, &names)),
    );
    warnings
}

fn require_valid_tag_catalog(catalog: &TagCatalog) -> Result<(), AppError> {
    if !catalog.invalid.is_empty() {
        let details = catalog
            .invalid
            .iter()
            .map(|tag| format!("{} ({})", tag.name, tag.error))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(AppError::InvalidVersionTags(details));
    }
    let duplicates = duplicate_tag_versions(catalog);
    if !duplicates.is_empty() {
        let details = duplicates
            .iter()
            .map(|(version, names)| format!("{version}: {}", names.join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AppError::DuplicateVersionTags(details));
    }
    Ok(())
}

fn emit_human_warnings(context: &CliContext, warnings: &[AppWarning]) {
    if context.quiet || context.format == OutputFormat::Json {
        return;
    }
    for warning in warnings {
        eprintln!("warning: {}", warning.message);
    }
}

impl CliContext {
    fn new(cli: &Cli) -> Result<Self, AppError> {
        Ok(Self {
            start: std::env::current_dir().map_err(AppError::CurrentDirectory)?,
            project: cli.project.clone(),
            config: cli.config.clone(),
            format: cli.format,
            quiet: cli.quiet,
            verbose: cli.verbose,
        })
    }

    fn load_project(&self) -> Result<Project, AppError> {
        Project::discover(&self.start, self.project.as_deref(), self.config.as_deref())
            .map_err(AppError::Project)
    }

    fn month(date: Option<DateArg>) -> YearMonth {
        date.map_or_else(|| SystemClock.year_month(), |date| date.0)
    }
}

type CommandResult = Result<(), AppError>;

impl RunWith<&CliContext> for Init {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let root = context.project.as_deref().map_or_else(
            || Ok(context.start.clone()),
            |path| {
                let path = if path.is_absolute() {
                    path.to_owned()
                } else {
                    context.start.join(path)
                };
                path.canonicalize().map_err(|source| AppError::ProjectPath {
                    path: path.clone(),
                    source,
                })
            },
        )?;
        let path = context.config.as_deref().map_or_else(
            || root.join(".chrono.toml"),
            |path| {
                if path.is_absolute() {
                    path.to_owned()
                } else {
                    context.start.join(path)
                }
            },
        );
        let detected_source = ["Cargo.toml", "mix.exs", "package.json", "VERSION"]
            .into_iter()
            .find(|candidate| root.join(candidate).is_file());
        let source = detected_source.unwrap_or("VERSION");
        if detected_source.is_none() && self.initial_version.is_none() {
            return Err(AppError::MissingInitialVersion);
        }
        if detected_source.is_some() && self.initial_version.is_some() {
            return Err(AppError::UnexpectedInitialVersion(source.to_owned()));
        }
        let config_text = format!(
            "# ChronoStamp project configuration.\n\
             schema = 1\n\
             version_source = \"{source}\"\n\n\
             [git]\n\
             tag_prefix = \"v\"\n\
             remote = \"origin\"\n\
             sign_tags = false\n\n\
             [dev]\n\
             hash_length = 7\n"
        );

        if path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(AppError::ConfigSymlink(path));
        }
        if path.exists() && !self.force {
            return Err(AppError::ConfigExists(path));
        }
        if self.dry_run {
            emit_init_result(context, &path, source, self.initial_version.as_ref(), true)?;
            return Ok(());
        }

        if let Some(version) = &self.initial_version {
            write_new_file(&root.join("VERSION"), format!("{version}\n").as_bytes())?;
        }
        let written = if self.force {
            fs::write(&path, config_text).map_err(|source| AppError::Write {
                path: path.clone(),
                source,
            })
        } else {
            write_new_file(&path, config_text.as_bytes())
        };
        if let Err(error) = written {
            if self.initial_version.is_some() {
                let _ = fs::remove_file(root.join("VERSION"));
            }
            return Err(error);
        }
        emit_init_result(context, &path, source, self.initial_version.as_ref(), false)?;
        Ok(())
    }
}

/// Creates a file that must not already exist, removing it again if the write fails.
fn write_new_file(path: &std::path::Path, contents: &[u8]) -> Result<(), AppError> {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| AppError::Write {
            path: path.to_owned(),
            source,
        })?;
    if let Err(source) = file.write_all(contents).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(path);
        return Err(AppError::Write {
            path: path.to_owned(),
            source,
        });
    }
    Ok(())
}

fn emit_init_result(
    context: &CliContext,
    path: &PathBuf,
    source: &str,
    initial_version: Option<&ChronoStamp>,
    dry_run: bool,
) -> Result<(), AppError> {
    if context.format == OutputFormat::Json {
        emit_ok(
            &[],
            json!({
                "dry_run": dry_run,
                "path": path,
                "version_source": source,
                "initial_version": initial_version,
            }),
        )?;
    } else {
        output::line(format_args!(
            "{} {}",
            if dry_run { "would create" } else { "created" },
            path.display()
        ))?;
    }
    Ok(())
}

impl RunWith<&CliContext> for Status {
    type Output = CommandResult;

    #[allow(clippy::too_many_lines)]
    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let git = Git::new(&project.root);
        let repository = git.is_repository();
        let catalog = if repository {
            git.tags(&project.config.config.git.tag_prefix)?
        } else {
            TagCatalog {
                tags: Vec::new(),
                invalid: Vec::new(),
            }
        };
        let highest_tag = catalog.tags.first();
        let mut authoritative = vec![project.version.clone()];
        authoritative.extend(catalog.tags.iter().map(|tag| tag.version.clone()));
        let now = CliContext::month(self.date);
        let future = latest_future_version(&authoritative, now);
        let mut warnings = catalog_warnings(&catalog);
        let next = if let Some(future) = future {
            if self.allow_future {
                warnings.push(AppWarning::future_month_adopted(future, now));
                Some(next_version(
                    &authoritative,
                    now,
                    NextVersionOptions {
                        allow_future: true,
                        ..NextVersionOptions::default()
                    },
                )?)
            } else {
                warnings.push(AppWarning::future_version(future, now));
                None
            }
        } else {
            Some(next_version(
                &authoritative,
                now,
                NextVersionOptions::default(),
            )?)
        };
        let hash = repository
            .then(|| git.hash(project.config.config.dev.hash_length))
            .transpose()?;
        let branch = repository.then(|| git.branch()).transpose()?.flatten();
        let dirty = repository.then(|| git.is_dirty()).transpose()?;

        if context.format == OutputFormat::Json {
            emit_ok(
                &warnings,
                json!({
                    "project": {
                        "name": project.name,
                        "root": project.root,
                        "version_source": project.source_path,
                        "source_kind": project.source_kind.as_str(),
                        "config": project.config.path,
                    },
                    "version": project.version,
                    "canonical": project.version.canonical(),
                    "highest_tag": highest_tag,
                    "next": next,
                    "git": {
                        "repository": repository,
                        "hash": hash.as_ref().map(ToString::to_string),
                        "branch": branch,
                        "dirty": dirty,
                    }
                }),
            )?;
        } else if context.quiet {
            output::line(project.version)?;
        } else {
            output::line(format_args!("project: {}", project.name))?;
            output::line(format_args!("version: {}", project.version))?;
            output::line(format_args!(
                "highest tag: {}",
                highest_tag.map_or("none", |tag| tag.name.as_str())
            ))?;
            output::line(format_args!(
                "commit: {}",
                hash.as_ref().map_or("none", |value| value.as_str())
            ))?;
            output::line(format_args!(
                "branch: {}",
                branch
                    .as_deref()
                    .unwrap_or(if repository { "detached" } else { "none" })
            ))?;
            output::line(format_args!(
                "dirty: {}",
                dirty.map_or("unknown".to_owned(), |value| value.to_string())
            ))?;
            output::line(format_args!(
                "next: {}",
                next.as_ref()
                    .map_or_else(|| "unavailable".to_owned(), ToString::to_string)
            ))?;
            if context.verbose {
                output::line(format_args!("source: {}", project.source_path.display()))?;
            }
            emit_human_warnings(context, &warnings);
        }
        Ok(())
    }
}

impl RunWith<&CliContext> for Parse {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let kind = match self.version.kind() {
            VersionKind::Release { .. } => "release",
            VersionKind::Git { .. } => "git",
        };
        if context.format == OutputFormat::Json {
            emit_ok(
                &[],
                json!({
                    "version": self.version,
                    "display": self.version.to_string(),
                    "canonical": self.version.canonical(),
                    "year": self.version.year().get(),
                    "month": self.version.month().get(),
                    "kind": kind,
                    "increment": self.version.increment(),
                    "tag": self.version.stability().as_str(),
                    "hash": self.version.git_hash().map(crate::GitHash::as_str),
                    "prerelease": self.version.is_prerelease(),
                    "dev_build": self.version.is_dev_build(),
                }),
            )?;
        } else if self.canonical {
            output::line(self.version.canonical())?;
        } else {
            output::line(self.version)?;
        }
        Ok(())
    }
}

impl RunWith<&CliContext> for Compare {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let ordering = self.left.cmp(&self.right);
        let numeric = match ordering {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        };
        if context.format == OutputFormat::Json {
            emit_ok(
                &[],
                json!({
                    "left": self.left,
                    "right": self.right,
                    "ordering": numeric,
                }),
            )?;
        } else {
            output::line(numeric)?;
        }
        Ok(())
    }
}

impl RunWith<&CliContext> for Bump {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let git = Git::new(&project.root);
        let mut versions = vec![project.version.clone()];
        if git.is_repository() {
            let catalog = git.tags(&project.config.config.git.tag_prefix)?;
            require_valid_tag_catalog(&catalog)?;
            versions.extend(catalog.tags.into_iter().map(|tag| tag.version));
        }
        let next = next_version(
            &versions,
            CliContext::month(self.date),
            NextVersionOptions {
                tag: self.tag.into(),
                increment: self.increment,
                allow_future: self.allow_future,
            },
        )?;
        let plan = UpdatePlan::for_project(&project, &next, true, true)?;
        require_clean_managed_paths(&git, &plan, self.allow_dirty_managed)?;
        if !self.dry_run {
            apply_and_validate(&project, &plan)?;
        }
        emit_update_plan(context, &plan, self.dry_run)?;
        Ok(())
    }
}

impl RunWith<&CliContext> for Dev {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let length = self.length.unwrap_or(project.config.config.dev.hash_length);
        let hash = Git::new(&project.root).hash(length)?;
        let month = CliContext::month(self.date);
        let version = ChronoStamp::git(month.year().get(), month.month().get(), hash.to_string())?;
        if self.write || self.dry_run {
            let plan = UpdatePlan::for_project(&project, &version, true, true)?;
            if self.write && !self.dry_run {
                apply_and_validate(&project, &plan)?;
            }
            emit_update_plan(context, &plan, self.dry_run)?;
            return Ok(());
        }
        if context.format == OutputFormat::Json {
            emit_ok(
                &[],
                json!({
                    "version": version,
                    "canonical": version.canonical(),
                    "hash_length": length,
                }),
            )?;
        } else {
            output::line(version)?;
        }
        Ok(())
    }
}

impl RunWith<&CliContext> for Check {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let mut checked = Vec::new();
        for target in &project.config.config.updates {
            let path = project.config.resolve(&target.path)?;
            let values = read_target_versions(&project.root, &path, target)?;
            for value in values {
                if value != project.version {
                    return Err(AppError::VersionMismatch {
                        path,
                        expected: project.version.clone(),
                        actual: value,
                    });
                }
            }
            checked.push(target.path.clone());
        }
        if context.format == OutputFormat::Json {
            emit_ok(
                &[],
                json!({
                    "version": project.version,
                    "checked": checked,
                }),
            )?;
        } else if !context.quiet {
            output::line(format_args!(
                "{} is consistent across {} configured target(s)",
                project.version,
                checked.len()
            ))?;
        }
        Ok(())
    }
}

impl RunWith<&CliContext> for Sync {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let plan = UpdatePlan::for_project(&project, &project.version, false, false)?;
        require_clean_managed_paths(&Git::new(&project.root), &plan, self.allow_dirty_managed)?;
        if !self.dry_run {
            plan.apply()?;
        }
        emit_update_plan(context, &plan, self.dry_run)?;
        Ok(())
    }
}

impl RunWith<&CliContext> for History {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let catalog = Git::new(&project.root).tags(&project.config.config.git.tag_prefix)?;
        let mut warnings = catalog_warnings(&catalog);
        let now = CliContext::month(self.date);
        if let Some(future) =
            latest_future_version(catalog.tags.iter().map(|tag| &tag.version), now)
        {
            warnings.push(AppWarning::future_version(future, now));
        }
        let tags = catalog
            .tags
            .into_iter()
            .filter(|tag| {
                self.month
                    .is_none_or(|month| tag.version.year_month() == month.0)
            })
            .take(self.limit.unwrap_or(usize::MAX))
            .collect::<Vec<_>>();
        if context.format == OutputFormat::Json {
            emit_ok(&warnings, json!({ "tags": tags }))?;
        } else {
            for tag in tags {
                output::line(format_args!("{}\t{}", tag.version, tag.name))?;
            }
            emit_human_warnings(context, &warnings);
        }
        Ok(())
    }
}

impl RunWith<&CliContext> for Tag {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let git = Git::new(&project.root);
        require_clean_repository(&git)?;
        if project.version.git_hash().is_some() {
            return Err(AppError::GitBuildTag(project.version));
        }
        let prefix = self
            .prefix
            .unwrap_or_else(|| project.config.config.git.tag_prefix.clone());
        let catalog = git.tags(&prefix)?;
        require_valid_tag_catalog(&catalog)?;
        let mut authoritative = catalog
            .tags
            .iter()
            .map(|tag| tag.version.clone())
            .collect::<Vec<_>>();
        authoritative.push(project.version.clone());
        require_no_future_versions(&authoritative, SystemClock.year_month())?;
        if let Some(existing) = catalog
            .tags
            .iter()
            .find(|tag| tag.version == project.version)
        {
            return Err(AppError::VersionAlreadyTagged {
                version: project.version,
                tag: existing.name.clone(),
            });
        }
        let name = format!("{prefix}{}", project.version);
        if git.tag_exists(&name)? {
            return Err(AppError::DuplicateTag(name));
        }
        let message = self
            .message
            .unwrap_or_else(|| format!("Release {}", project.version));
        let sign = self.sign || project.config.config.git.sign_tags;
        let remote = self
            .remote
            .unwrap_or_else(|| project.config.config.git.remote.clone());
        if !self.dry_run {
            git.create_tag(&name, &message, sign)?;
            if self.push {
                git.push_head_and_tag(&remote, &name)?;
            }
        }
        if context.format == OutputFormat::Json {
            emit_ok(
                &[],
                json!({
                    "dry_run": self.dry_run,
                    "tag": name,
                    "signed": sign,
                    "push": self.push,
                    "remote": remote,
                }),
            )?;
        } else if !context.quiet {
            output::line(format_args!(
                "{} tag {name}{}",
                if self.dry_run {
                    "would create"
                } else {
                    "created"
                },
                if self.push { " and push it" } else { "" }
            ))?;
        }
        Ok(())
    }
}

impl RunWith<&CliContext> for Release {
    type Output = CommandResult;

    fn run_with(self, context: &CliContext) -> Self::Output {
        let project = context.load_project()?;
        let git = Git::new(&project.root);
        require_clean_repository(&git)?;
        let existing_tags = git.tags(&project.config.config.git.tag_prefix)?;
        require_valid_tag_catalog(&existing_tags)?;
        let mut versions = vec![project.version.clone()];
        versions.extend(existing_tags.tags.iter().map(|tag| tag.version.clone()));
        let next = next_version(
            &versions,
            CliContext::month(self.date),
            NextVersionOptions {
                tag: self.tag.into(),
                increment: None,
                allow_future: self.allow_future,
            },
        )?;
        let name = format!("{}{}", project.config.config.git.tag_prefix, next);
        if let Some(existing) = existing_tags.tags.iter().find(|tag| tag.version == next) {
            return Err(AppError::VersionAlreadyTagged {
                version: next,
                tag: existing.name.clone(),
            });
        }
        if git.tag_exists(&name)? {
            return Err(AppError::DuplicateTag(name));
        }
        let plan = UpdatePlan::for_project(&project, &next, true, true)?;
        let message = self.message.unwrap_or_else(|| format!("Release {next}"));
        let sign = self.sign || project.config.config.git.sign_tags;
        let remote = self
            .remote
            .unwrap_or_else(|| project.config.config.git.remote.clone());

        if self.dry_run {
            emit_update_plan(context, &plan, true)?;
            return Ok(());
        }

        apply_and_validate(&project, &plan).map_err(|source| AppError::ReleasePhase {
            phase: "file update and validation",
            recovery: "No Git commit or tag was created. Automatic restoration was attempted; inspect the managed files before retrying.",
            source: Box::new(source),
        })?;
        let paths = plan
            .changes
            .iter()
            .map(|change| change.path.clone())
            .collect::<Vec<_>>();
        git.add_paths(&paths).map_err(|source| AppError::ReleasePhase {
            phase: "staging",
            recovery: "Managed files were updated but no commit or tag was created. Review and stage them, or restore them before retrying.",
            source: Box::new(source),
        })?;
        git.commit(&message).map_err(|source| AppError::ReleasePhase {
            phase: "commit",
            recovery: "Managed files may be staged. Resolve the Git error, then commit or restore them before retrying.",
            source: Box::new(source),
        })?;
        git.create_tag(&name, &message, sign)
            .map_err(|source| AppError::ReleasePhase {
                phase: "tag",
                recovery: "The release commit exists locally without its tag. Create the shown tag on HEAD, then retry the push if needed.",
                source: Box::new(source),
            })?;
        if self.push {
            git.push_head_and_tag(&remote, &name)
                .map_err(|source| AppError::ReleasePhase {
                    phase: "push",
                    recovery: "The release commit and tag are safe locally. Fix the remote issue and push HEAD plus the tag again.",
                    source: Box::new(source),
                })?;
        }
        emit_update_plan(context, &plan, false)?;
        Ok(())
    }
}

fn require_clean_repository(git: &Git) -> Result<(), AppError> {
    if !git.is_repository() {
        return Err(AppError::NotGitRepository);
    }
    if git.is_dirty()? {
        return Err(AppError::DirtyTree);
    }
    Ok(())
}

fn require_no_future_versions(versions: &[ChronoStamp], now: YearMonth) -> Result<(), AppError> {
    if let Some(future) = latest_future_version(versions, now) {
        Err(AppError::Next(NextVersionError::FutureVersion {
            version: future.clone(),
            now,
        }))
    } else {
        Ok(())
    }
}

fn apply_and_validate(project: &Project, plan: &UpdatePlan) -> Result<(), AppError> {
    plan.apply()?;
    if let Err(validation) = project.refresh_and_validate_cargo_lock() {
        if let Err(rollback) = plan.restore() {
            return Err(AppError::CargoValidationRollback {
                validation: Box::new(validation),
                rollback: Box::new(rollback),
            });
        }
        return Err(AppError::Project(validation));
    }
    Ok(())
}

fn require_clean_managed_paths(
    git: &Git,
    plan: &UpdatePlan,
    allow_dirty: bool,
) -> Result<(), AppError> {
    if allow_dirty || !git.is_repository() {
        return Ok(());
    }
    let paths = plan
        .changes
        .iter()
        .map(|change| change.path.clone())
        .collect::<Vec<_>>();
    let dirty = git.dirty_paths(&paths)?;
    if dirty.is_empty() {
        Ok(())
    } else {
        Err(AppError::DirtyManagedPaths(dirty))
    }
}

impl RunWith<&CliContext> for Completion {
    type Output = CommandResult;

    fn run_with(self, _context: &CliContext) -> Self::Output {
        output::line(Cli::completion_script(self.shell.into()))?;
        Ok(())
    }
}

/// Emits the JSON success envelope with command-specific fields merged in.
fn emit_ok(warnings: &[AppWarning], fields: serde_json::Value) -> Result<(), AppError> {
    let mut envelope = json!({
        "schema_version": JSON_SCHEMA_VERSION,
        "ok": true,
        "warnings": warnings,
    });
    if let (serde_json::Value::Object(envelope_fields), serde_json::Value::Object(fields)) =
        (&mut envelope, fields)
    {
        envelope_fields.extend(fields);
    }
    output::json(&envelope)?;
    Ok(())
}

fn emit_update_plan(
    context: &CliContext,
    plan: &UpdatePlan,
    dry_run: bool,
) -> Result<(), AppError> {
    if context.format == OutputFormat::Json {
        emit_ok(
            &[],
            json!({
                "dry_run": dry_run,
                "changes": plan.changes,
            }),
        )?;
    } else if !context.quiet {
        for change in &plan.changes {
            output::line(format_args!(
                "{}: {} -> {}{}",
                change.path.display(),
                change.from,
                change.to,
                if dry_run { " (dry run)" } else { "" }
            ))?;
        }
    }
    Ok(())
}

/// Run the process-facing CLI and return its exit status.
pub fn entry(args: impl IntoIterator<Item = OsString>) -> i32 {
    let args = args.into_iter().collect::<Vec<_>>();
    let refs = args.iter().map(OsString::as_os_str).collect::<Vec<_>>();
    let json_requested = requests_json(&refs);

    if let Some(spec) = Cli::spec_request(&refs) {
        print!("{spec}");
        return 0;
    }
    if let Some(completion) = Cli::completion_request(&args) {
        print!("{completion}");
        return 0;
    }

    let cli = match Cli::parse_from(&refs) {
        Ok(cli) => cli,
        Err(error) => {
            if json_requested {
                let report = usage::diagnostic::report(Cli::spec(), &refs, &error);
                let envelope = json!({
                    "schema_version": JSON_SCHEMA_VERSION,
                    "ok": false,
                    "warnings": [],
                    "error": {
                        "code": report.code.as_str(),
                        "message": report.rendered.trim_end(),
                        "subject": report.subject,
                        "location": report.location.map(|span| json!({
                            "argument": span.index,
                            "start": span.start,
                            "end": span.end,
                        })),
                    }
                });
                let _ = output::json(&envelope);
                return 2;
            }
            if !matches!(
                error,
                usage::Error::Help { .. }
                    | usage::Error::HelpAll { .. }
                    | usage::Error::MissingArgsHelp { .. }
                    | usage::Error::Version { .. }
            ) && let Some(color) = requested_color(&refs)
            {
                let style = match color {
                    ColorMode::Always => usage::diagnostic::Style::COLOURED,
                    ColorMode::Never => usage::diagnostic::Style::PLAIN,
                    ColorMode::Auto => usage::diagnostic::Style::auto(),
                };
                eprint!(
                    "{}",
                    usage::diagnostic::render(Cli::spec(), &refs, &error, style)
                );
                return 2;
            }
            let usage::embedded::Outcome::Exit(exit) = Cli::embedded_outcome(&args) else {
                unreachable!("an argv that failed parse_from cannot parse through embedded_outcome")
            };
            if exit.stderr {
                eprint!("{}", exit.text);
            } else {
                print!("{}", exit.text);
            }
            return exit.code;
        }
    };

    if cli.command.is_none() {
        if let Some(help) = Cli::render_help(Cli::command(), false) {
            print!("{help}");
        }
        return 0;
    }

    let context = match CliContext::new(&cli) {
        Ok(context) => context,
        Err(error) => return render_app_error(&error, cli.format),
    };
    let Some(command) = cli.command else {
        return 0;
    };
    let result = command.run_with(&context);
    match result {
        Ok(()) => 0,
        Err(error) => render_app_error(&error, context.format),
    }
}

fn requests_json(args: &[&OsStr]) -> bool {
    args.windows(2)
        .any(|pair| pair == [OsStr::new("--format"), OsStr::new("json")])
        || args.iter().any(|arg| arg == &OsStr::new("--format=json"))
}

fn requested_color(args: &[&OsStr]) -> Option<ColorMode> {
    for pair in args.windows(2) {
        if pair[0] == OsStr::new("--color") {
            return match pair[1].to_str() {
                Some("always") => Some(ColorMode::Always),
                Some("never") => Some(ColorMode::Never),
                Some("auto") => Some(ColorMode::Auto),
                _ => None,
            };
        }
    }
    args.iter().find_map(|arg| match arg.to_str() {
        Some("--color=always") => Some(ColorMode::Always),
        Some("--color=never") => Some(ColorMode::Never),
        Some("--color=auto") => Some(ColorMode::Auto),
        _ => None,
    })
}

fn render_app_error(error: &AppError, format: OutputFormat) -> i32 {
    if format == OutputFormat::Json {
        let _ = output::json(&json!({
            "schema_version": JSON_SCHEMA_VERSION,
            "ok": false,
            "warnings": [],
            "error": {
                "code": error.code(),
                "message": error.to_string(),
            }
        }));
    } else {
        eprintln!("error: {error}");
    }
    1
}

impl Serialize for VersionTag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Tag<'a> {
            name: &'a str,
            version: &'a ChronoStamp,
            created_at: &'a Option<String>,
            subject: &'a Option<String>,
        }
        Tag {
            name: &self.name,
            version: &self.version,
            created_at: &self.created_at,
            subject: &self.subject,
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Error)]
enum AppError {
    #[error("cannot determine the current directory: {0}")]
    CurrentDirectory(std::io::Error),
    #[error("cannot access project path {path}: {source}")]
    ProjectPath {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Next(#[from] NextVersionError),
    #[error(transparent)]
    Validation(#[from] crate::ValidationError),
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error(transparent)]
    Update(#[from] UpdateError),
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("configuration already exists at {0}; use --force to replace it")]
    ConfigExists(PathBuf),
    #[error("refusing to replace configuration symlink {0}")]
    ConfigSymlink(PathBuf),
    #[error("generic initialization requires --initial-version VERSION")]
    MissingInitialVersion,
    #[error("--initial-version is only valid when no version source exists; found {0}")]
    UnexpectedInitialVersion(String),
    #[error("this command requires a Git repository")]
    NotGitRepository,
    #[error("the Git worktree must be clean before this operation")]
    DirtyTree,
    #[error("Git development build {0} cannot be tagged as a release")]
    GitBuildTag(ChronoStamp),
    #[error("Git tag already exists: {0}")]
    DuplicateTag(String),
    #[error("configured tag prefix contains invalid ChronoStamp tags: {0}")]
    InvalidVersionTags(String),
    #[error("multiple Git tags represent the same ChronoStamp: {0}")]
    DuplicateVersionTags(String),
    #[error("version {version} is already represented by Git tag {tag}")]
    VersionAlreadyTagged { version: ChronoStamp, tag: String },
    #[error(
        "managed paths have uncommitted changes; commit them or use --allow-dirty-managed: {0:?}"
    )]
    DirtyManagedPaths(Vec<PathBuf>),
    #[error("release failed during {phase}: {source}\nrecovery: {recovery}")]
    ReleasePhase {
        phase: &'static str,
        recovery: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + std::marker::Sync>,
    },
    #[error("version mismatch in {path}: expected {expected}, found {actual}")]
    VersionMismatch {
        path: PathBuf,
        expected: ChronoStamp,
        actual: ChronoStamp,
    },
    #[error(
        "Cargo validation failed after managed files were installed: {validation}; rollback also failed: {rollback}"
    )]
    CargoValidationRollback {
        validation: Box<ProjectError>,
        rollback: Box<UpdateError>,
    },
}

impl AppError {
    const fn code(&self) -> &'static str {
        match self {
            Self::CurrentDirectory(_)
            | Self::ProjectPath { .. }
            | Self::Write { .. }
            | Self::Output(_) => "io_error",
            Self::Project(_) => "project_error",
            Self::Config(_)
            | Self::ConfigExists(_)
            | Self::ConfigSymlink(_)
            | Self::MissingInitialVersion
            | Self::UnexpectedInitialVersion(_) => "config_error",
            Self::Git(_) => "git_error",
            Self::NotGitRepository
            | Self::DirtyTree
            | Self::GitBuildTag(_)
            | Self::DuplicateTag(_)
            | Self::InvalidVersionTags(_)
            | Self::DuplicateVersionTags(_)
            | Self::VersionAlreadyTagged { .. }
            | Self::DirtyManagedPaths(_) => "git_preflight_error",
            Self::ReleasePhase { .. } => "release_error",
            Self::Next(_) => "next_version_error",
            Self::Validation(_) => "invalid_version",
            Self::Update(_) | Self::CargoValidationRollback { .. } => "update_error",
            Self::VersionMismatch { .. } => "version_mismatch",
        }
    }
}

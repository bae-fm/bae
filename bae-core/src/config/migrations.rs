//! `config.yaml`'s migration ladder: the ordered steps that carry a file an
//! older bae wrote to the shape this build reads.
//!
//! The file records its own `config_version`, and every step above that version
//! runs, in order, before the typed read. A file with no `config_version` is
//! version 0 — the era before the key existed. The version this build reads is
//! the ladder's length, so a new step is the only thing that moves it.
//!
//! A step edits the parsed mapping rather than typed values: a step exists
//! precisely because the typed shape does not fit the file yet. Adding,
//! renaming, or removing a key in [`ConfigYaml`](super::ConfigYaml) lands with
//! the step that carries the older shape to it — the read is strict and has no
//! defaults to fall back on.

use super::ConfigError;
use serde_yaml::{Mapping, Value};
use tracing::info;

/// The key a file records its version under, and the first key `config.yaml`
/// writes.
pub(crate) const CONFIG_VERSION_KEY: &str = "config_version";

/// One ordered step in `config.yaml`'s ladder.
pub(crate) struct ConfigMigration {
    /// 1-based and contiguous: a step's version is its position in [`all`], and
    /// the ladder's length is the version this build reads.
    pub version: u32,
    /// Logged when the step runs.
    pub name: &'static str,
    /// Edits the parsed mapping into the shape the next version reads.
    pub up: fn(&mut Mapping) -> Result<(), ConfigError>,
}

/// The ordered migration ladder. Versions are 1-based and contiguous.
pub(crate) fn all() -> Vec<ConfigMigration> {
    vec![ConfigMigration {
        version: 1,
        name: "snapshot_threshold_and_prefill_with_tags",
        up: snapshot_threshold_and_prefill_with_tags,
    }]
}

/// The `config_version` this build reads: the ladder's length.
pub(crate) fn current_version() -> u32 {
    all().len() as u32
}

/// Carry `mapping` to the shape this build reads, reporting the version it came
/// from, or `None` when it was already at this one.
///
/// A file above the current version is refused: this build cannot know what a
/// newer bae's keys mean, so it says so rather than downgrading the file.
pub(crate) fn upgrade(mapping: &mut Mapping) -> Result<Option<u32>, ConfigError> {
    let from = recorded_version(mapping)?;
    let current = current_version();
    if from > current {
        return Err(ConfigError::Upgrade(format!(
            "version {from} was written by a newer bae; this build reads up to version {current}"
        )));
    }
    if from == current {
        return Ok(None);
    }
    for step in all().into_iter().filter(|step| step.version > from) {
        info!("upgrading config.yaml to v{}: {}", step.version, step.name);
        (step.up)(mapping)?;
    }
    mapping.insert(Value::from(CONFIG_VERSION_KEY), Value::from(current));
    Ok(Some(from))
}

/// The version `mapping` records. An absent `config_version` is version 0: the
/// file was written before the key was.
fn recorded_version(mapping: &Mapping) -> Result<u32, ConfigError> {
    let Some(value) = mapping.get(CONFIG_VERSION_KEY) else {
        return Ok(0);
    };
    value
        .as_u64()
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(|| {
            ConfigError::Upgrade(format!(
                "{CONFIG_VERSION_KEY} must be a non-negative integer, found {value:?}"
            ))
        })
}

const SNAPSHOT_COMMIT_THRESHOLD_KEY: &str = "snapshot_commit_threshold";
const IMPORT_METADATA_SOURCE_KEY: &str = "default_import_metadata_source";
const PREFILL_WITH_TAGS_KEY: &str = "prefill_with_tags";

/// Covers a file that carries `identify_automatically` and `metadata_sources`
/// but no `config_version`. That shape records no snapshot policy, and it names
/// where a candidate's draft comes from with a key the current shape dropped.
///
/// `snapshot_commit_threshold` takes the value a new library starts at.
/// `default_import_metadata_source` named the draft's origin, and only
/// `file_tags` drew it from the folder's own tags — `find_online` and `none`
/// did not — so that is the one value carrying `prefill_with_tags: true`.
///
/// Nothing else is touched: a key this step does not name is one the shape it
/// covers already wrote.
fn snapshot_threshold_and_prefill_with_tags(mapping: &mut Mapping) -> Result<(), ConfigError> {
    if !mapping.contains_key(SNAPSHOT_COMMIT_THRESHOLD_KEY) {
        mapping.insert(
            Value::from(SNAPSHOT_COMMIT_THRESHOLD_KEY),
            Value::from(coven::Config::DEFAULT_SNAPSHOT_COMMIT_THRESHOLD.get()),
        );
    }

    let Some(source) = mapping.remove(IMPORT_METADATA_SOURCE_KEY) else {
        return Ok(());
    };
    // The retired key says nothing a file that already records the current one
    // does not say, so there it is only dropped.
    if mapping.contains_key(PREFILL_WITH_TAGS_KEY) {
        return Ok(());
    }
    let prefill = match source.as_str() {
        Some("file_tags") => true,
        Some("find_online" | "none") => false,
        _ => {
            return Err(ConfigError::Upgrade(format!(
                "{IMPORT_METADATA_SOURCE_KEY} has a value bae never wrote: {source:?}"
            )))
        }
    };
    mapping.insert(Value::from(PREFILL_WITH_TAGS_KEY), Value::from(prefill));
    Ok(())
}

#[cfg(test)]
#[path = "migrations_tests.rs"]
mod tests;

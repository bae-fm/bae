use super::*;
use crate::config::{
    discover_libraries_from_bae_dir, parse_config_yaml, registered_library_path, CloudProvider,
    Config, ConfigYaml,
};
use serial_test::serial;
use std::num::NonZeroU32;
use std::path::PathBuf;
use tempfile::TempDir;

fn make_config(library_id: &str) -> Config {
    Config::with_defaults(
        library_id.to_string(),
        "device-a".to_string(),
        PathBuf::from("unused"),
        "Test Library".to_string(),
    )
}

/// The file a library at the current shape writes, as a mapping to edit.
fn to_mapping(config: &Config) -> Mapping {
    match serde_yaml::to_value(ConfigYaml::from(config)).unwrap() {
        Value::Mapping(mapping) => mapping,
        other => panic!("config.yaml is one mapping, not {other:?}"),
    }
}

fn to_yaml(mapping: &Mapping) -> String {
    serde_yaml::to_string(&Value::Mapping(mapping.clone())).unwrap()
}

/// That file rewritten into the last unversioned shape: no `config_version`, no
/// `snapshot_commit_threshold`, and the draft's origin written as the retired
/// source picker instead of `prefill_with_tags`.
fn unversioned(config: &Config, source: &str) -> Mapping {
    let mut mapping = to_mapping(config);
    mapping.remove(CONFIG_VERSION_KEY).unwrap();
    mapping.remove(SNAPSHOT_COMMIT_THRESHOLD_KEY).unwrap();
    mapping.remove(PREFILL_WITH_TAGS_KEY).unwrap();
    mapping.insert(
        Value::from(IMPORT_METADATA_SOURCE_KEY),
        Value::from(source.to_string()),
    );
    mapping
}

fn write_library(bae_dir: &std::path::Path, library_id: &str, mapping: &Mapping) -> PathBuf {
    let library_dir = registered_library_path(bae_dir, library_id);
    std::fs::create_dir_all(&library_dir).unwrap();
    let config_path = library_dir.join("config.yaml");
    std::fs::write(&config_path, to_yaml(mapping)).unwrap();
    config_path
}

/// The version a file records is an index into the ladder, and the version this
/// build reads is the ladder's length — so a gap, a duplicate, or a first step
/// above 1 would leave a recorded version no step answers for.
#[test]
fn the_ladder_is_contiguous_from_one_and_its_length_is_the_current_version() {
    let ladder = all();
    for (index, step) in ladder.iter().enumerate() {
        assert_eq!(
            step.version,
            index as u32 + 1,
            "step {} is not at its own version",
            step.name
        );
    }
    assert_eq!(current_version(), ladder.len() as u32);
}

#[test]
fn migration_one_fills_the_snapshot_threshold_only_when_the_file_lacks_it() {
    let mut absent = Mapping::new();
    snapshot_threshold_and_prefill_with_tags(&mut absent).unwrap();
    assert_eq!(
        absent.get(SNAPSHOT_COMMIT_THRESHOLD_KEY).unwrap().as_u64(),
        Some(100)
    );

    let mut present = Mapping::new();
    present.insert(
        Value::from(SNAPSHOT_COMMIT_THRESHOLD_KEY),
        Value::from(7u32),
    );
    snapshot_threshold_and_prefill_with_tags(&mut present).unwrap();
    assert_eq!(
        present.get(SNAPSHOT_COMMIT_THRESHOLD_KEY).unwrap().as_u64(),
        Some(7),
        "a threshold the person set is theirs"
    );
}

/// Only `file_tags` drew the draft from the folder's own tags; the other two
/// values did not, so they both arrive as `prefill_with_tags: false`.
#[test]
fn migration_one_carries_the_draft_source_to_prefill_with_tags() {
    for (source, prefill) in [("file_tags", true), ("find_online", false), ("none", false)] {
        let mut mapping = Mapping::new();
        mapping.insert(
            Value::from(IMPORT_METADATA_SOURCE_KEY),
            Value::from(source.to_string()),
        );

        snapshot_threshold_and_prefill_with_tags(&mut mapping).unwrap();

        assert!(
            !mapping.contains_key(IMPORT_METADATA_SOURCE_KEY),
            "{source}"
        );
        assert_eq!(
            mapping.get(PREFILL_WITH_TAGS_KEY).unwrap().as_bool(),
            Some(prefill),
            "{source}"
        );
    }
}

#[test]
fn migration_one_refuses_a_draft_source_bae_never_wrote() {
    let mut mapping = Mapping::new();
    mapping.insert(
        Value::from(IMPORT_METADATA_SOURCE_KEY),
        Value::from("sideways".to_string()),
    );

    let error = snapshot_threshold_and_prefill_with_tags(&mut mapping).unwrap_err();

    assert!(matches!(error, ConfigError::Upgrade(_)), "{error}");
    assert!(error.to_string().contains("sideways"), "{error}");
}

/// A file that already records the setting keeps the value it records; the
/// retired key alongside it is only dropped.
#[test]
fn migration_one_drops_the_retired_key_over_a_recorded_prefill() {
    let mut mapping = Mapping::new();
    mapping.insert(
        Value::from(IMPORT_METADATA_SOURCE_KEY),
        Value::from("file_tags".to_string()),
    );
    mapping.insert(Value::from(PREFILL_WITH_TAGS_KEY), Value::from(false));

    snapshot_threshold_and_prefill_with_tags(&mut mapping).unwrap();

    assert!(!mapping.contains_key(IMPORT_METADATA_SOURCE_KEY));
    assert_eq!(
        mapping.get(PREFILL_WITH_TAGS_KEY).unwrap().as_bool(),
        Some(false)
    );
}

#[test]
fn migration_one_leaves_a_file_that_records_only_the_current_key_alone() {
    let mut mapping = Mapping::new();
    mapping.insert(Value::from(PREFILL_WITH_TAGS_KEY), Value::from(false));

    snapshot_threshold_and_prefill_with_tags(&mut mapping).unwrap();

    assert_eq!(
        mapping.get(PREFILL_WITH_TAGS_KEY).unwrap().as_bool(),
        Some(false)
    );
}

/// A file older than the shape the step covers records neither key, and the
/// step invents nothing: the strict read says which key is missing.
#[test]
fn a_file_older_than_the_shape_the_step_covers_fails_on_the_key_it_lacks() {
    let mut mapping = unversioned(&make_config("lib-ancient"), "none");
    mapping.remove(IMPORT_METADATA_SOURCE_KEY).unwrap();

    let error = parse_config_yaml(&to_yaml(&mapping)).unwrap_err();

    assert!(error.to_string().contains("prefill_with_tags"), "{error}");
}

/// The typed read requires `config_version` like every other key; what makes an
/// unversioned file readable is the ladder stamping it first.
#[test]
fn the_ladder_stamps_the_version_the_strict_read_requires() {
    let mapping = unversioned(&make_config("lib-v0"), "file_tags");

    assert!(
        ConfigYaml::from_value(&Value::Mapping(mapping.clone())).is_err(),
        "config_version is read as strictly as every other key"
    );

    let parsed = parse_config_yaml(&to_yaml(&mapping)).unwrap();
    assert_eq!(parsed.upgraded_from, Some(0));
    assert_eq!(parsed.config.config_version, current_version());
    assert!(parsed.config.prefs.prefill_with_tags);
}

/// The whole file a library at the unversioned shape has on disk: it opens, and
/// nothing the person set moves.
#[test]
fn an_unversioned_library_keeps_every_setting_it_recorded() {
    let mut config = make_config("lib-v0");
    config.store_name = "Shelf".to_string();
    config.prefs.pause_between_sides = false;
    config.prefs.max_concurrent_uploads = NonZeroU32::new(6).unwrap();
    config.prefs.identify_automatically = false;
    config.prefs.save_presets[0].name = "Archive".to_string();

    let parsed = parse_config_yaml(&to_yaml(&unversioned(&config, "none"))).unwrap();

    assert_eq!(parsed.upgraded_from, Some(0));
    assert_eq!(parsed.config.config_version, 1);
    assert_eq!(parsed.config.snapshot_commit_threshold.get(), 100);
    assert!(!parsed.config.prefs.prefill_with_tags);
    assert_eq!(parsed.config.identity.library_name, "Shelf");
    assert!(!parsed.config.prefs.pause_between_sides);
    assert_eq!(parsed.config.prefs.max_concurrent_uploads.get(), 6);
    assert!(!parsed.config.prefs.identify_automatically);
    assert_eq!(parsed.config.prefs.save_presets, config.prefs.save_presets);
}

/// The ladder does not run at the current version, so a key the file is missing
/// is still a failed load rather than a value taken from somewhere.
#[test]
fn a_file_at_the_current_version_still_fails_on_a_missing_key() {
    let mut mapping = to_mapping(&make_config("lib-current"));
    mapping.remove(PREFILL_WITH_TAGS_KEY).unwrap();

    let error = parse_config_yaml(&to_yaml(&mapping)).unwrap_err();

    assert!(error.to_string().contains("prefill_with_tags"), "{error}");
}

#[test]
fn a_config_version_that_is_not_a_version_is_refused() {
    let mut mapping = to_mapping(&make_config("lib-odd"));
    mapping.insert(
        Value::from(CONFIG_VERSION_KEY),
        Value::from("one".to_string()),
    );

    let error = parse_config_yaml(&to_yaml(&mapping)).unwrap_err();

    assert!(matches!(error, ConfigError::Upgrade(_)), "{error}");
}

/// A file a newer bae wrote is refused for what it is: this build cannot know
/// what its keys mean. The library is there, so the failure must not read as a
/// library that is gone.
#[test]
#[serial]
fn a_file_from_a_newer_bae_is_refused_by_version_and_listed_as_broken() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let ahead = current_version() + 1;
    let mut mapping = to_mapping(&make_config("lib-future"));
    mapping.insert(Value::from(CONFIG_VERSION_KEY), Value::from(ahead));
    write_library(bae_dir, "lib-future", &mapping);

    let error = Config::load_registered_library_from_bae_dir(
        bae_dir,
        "lib-future",
        &coven::SequentialIdProvider::new("device"),
    )
    .unwrap_err();

    assert!(
        matches!(error, ConfigError::Upgrade(_)),
        "a version this build cannot read is not a missing library: {error}"
    );
    let message = error.to_string();
    assert!(message.contains(&ahead.to_string()), "{message}");
    assert!(
        message.contains(&format!("version {}", current_version())),
        "{message}"
    );

    let libraries = discover_libraries_from_bae_dir(bae_dir).unwrap();
    assert_eq!(libraries.len(), 1);
    assert_eq!(libraries[0].error.as_deref(), Some(message.as_str()));
}

/// Opening is what writes the upgraded file back, once, after the typed read
/// succeeded — so the next open finds a file already at this shape.
#[test]
#[serial]
fn opening_an_unversioned_library_writes_the_upgraded_file_back() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let config_path = write_library(
        bae_dir,
        "lib-v0",
        &unversioned(&make_config("lib-v0"), "file_tags"),
    );

    let loaded = Config::load_registered_library_from_bae_dir(
        bae_dir,
        "lib-v0",
        &coven::SequentialIdProvider::new("device"),
    )
    .unwrap();
    assert!(loaded.prefs.prefill_with_tags);

    let written = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        written.starts_with(&format!("{CONFIG_VERSION_KEY}: {}\n", current_version())),
        "{written}"
    );
    assert_eq!(parse_config_yaml(&written).unwrap().upgraded_from, None);
}

/// Listing reads through the same ladder, so an unversioned library shows its
/// real name and provider — but a listing does not edit every library on disk.
#[test]
#[serial]
fn listing_an_unversioned_library_leaves_its_file_alone() {
    let tmp = TempDir::new().unwrap();
    let bae_dir = tmp.path();
    let mut config = make_config("lib-v0");
    config.store_name = "Shelf".to_string();
    config.cloud_home.provider = Some(CloudProvider::Dropbox);
    let config_path = write_library(bae_dir, "lib-v0", &unversioned(&config, "none"));
    let before = std::fs::read(&config_path).unwrap();

    let libraries = discover_libraries_from_bae_dir(bae_dir).unwrap();

    assert_eq!(libraries.len(), 1);
    assert_eq!(libraries[0].error, None);
    assert_eq!(libraries[0].name, "Shelf");
    assert_eq!(libraries[0].cloud_provider, Some(CloudProvider::Dropbox));
    assert_eq!(std::fs::read(&config_path).unwrap(), before);
}

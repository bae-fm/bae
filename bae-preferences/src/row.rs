//! This device's `preferences` row and its save presets: read by what each
//! setting decides, followed by the settings screens' live query, edited one
//! write at a time by the setters, and carried into the copy a restore over
//! this device's copy puts in its place.
//!
//! Its tables are `row.sql`. The fragments here are what a composed read
//! spreads to read them, beside the storage coven's views record, which the
//! settings show.

use super::{
    CastTranscodeFormat, Config, DiscogsValidation, IdentificationPreferences, ImportDestination,
    LookupCatalogPreferences, McpConfig, NamePattern, NamePiece, Preferences, PreferencesError,
    ReplayGainMode, SaveBitDepth, SaveCodec, SaveExtras, SaveFilenameValue, SaveFolderValue,
    SavePreset, SidePauseCountdown, SubsonicConfig, SubsonicNetwork,
};
use bae_store::{statement_row, DbError, Store};
use coven::CovenWrite;
use rusqlite::params;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use std::path::PathBuf;

// The `preferences` row.
sqlive_compose::fragment!(
    StoredPreferences {
        discogs_key: Option<crate::DiscogsValidation>,
        replay_gain_mode: crate::ReplayGainMode,
        save_embed_cover: bool,
        save_cover_file: bool,
        save_loudness_tags: bool,
        pause_between_sides: bool,
        side_pause_countdown_seconds: Option<crate::SidePauseCountdown>,
        max_concurrent_uploads: std::num::NonZeroU32,
        max_concurrent_downloads: std::num::NonZeroU32,
        show_remaining_time: bool,
        library_full_width: bool,
        ask_before_unpinning: bool,
        import_destination: crate::ImportDestination,
        identify_automatically: bool,
        import_when_identified: bool,
        ask_musicbrainz: bool,
        ask_discogs: bool,
        cast_enabled: bool,
        cast_transcode_format: crate::CastTranscodeFormat,
        mcp_enabled: bool,
        mcp_port: u16,
        subsonic_enabled: bool,
        subsonic_port: u32
    },
    r#"
    SELECT stored.discogs_key, stored.replay_gain_mode, stored.last_save_preset,
        stored.save_embed_cover, stored.save_cover_file, stored.save_loudness_tags,
        stored.pause_between_sides, stored.side_pause_countdown_seconds,
        stored.max_concurrent_uploads, stored.max_concurrent_downloads,
        stored.show_remaining_time, stored.library_full_width, stored.ask_before_unpinning,
        stored.import_destination, stored.identify_automatically,
        stored.import_when_identified, stored.ask_musicbrainz, stored.ask_discogs,
        stored.cast_enabled, stored.cast_transcode_format, stored.mcp_enabled, stored.mcp_port,
        stored.subsonic_enabled, stored.subsonic_port, stored.subsonic_username,
        stored.subsonic_network, stored.subsonic_network_interface
    FROM preferences AS stored
"#
);

// The save presets whose ids `wanted` holds, in the order the settings list
// them: one row, whatever the number of presets.
sqlive_compose::fragment!(
    SavePresetsAmong with wanted(id TEXT) {
        struct SavePresetColumns {
            id: String,
            name: String,
            codec: String,
            bit_depth: Option<String>,
            bitrate_kbps: Option<u32>
        }
        save_presets: json<Vec<SavePresetColumns>>
    },
    r#"
    SELECT json_group_array(json_object(
               'id', preset.id, 'name', preset.name, 'codec', preset.codec,
               'bit_depth', preset.bit_depth, 'bitrate_kbps', preset.bitrate_kbps)
               ORDER BY preset.position) AS save_presets
    FROM wanted
    JOIN save_preset AS preset ON preset.id = wanted.id
"#
);

// Every save preset.
sqlive_compose::fragment!(
    StoredSavePresets,
    r#"
    SELECT every.*
    FROM @crate::row::SavePresetsAmong(wanted: (SELECT held.id FROM save_preset AS held))
        AS every
"#
);

// The pieces of the file name pattern and of the folder name pattern, each in
// order: one row, whatever their number.
sqlive_compose::fragment!(
    StoredNamePatterns {
        struct NameTokenColumns { value: Option<String>, text: Option<String> }
        filename_tokens: json<Vec<NameTokenColumns>>,
        folder_tokens: json<Vec<NameTokenColumns>>
    },
    r#"
    SELECT
        (SELECT json_group_array(json_object('value', token.value, 'text', token.text)
                    ORDER BY token.position)
         FROM save_filename_token AS token) AS filename_tokens,
        (SELECT json_group_array(json_object('value', token.value, 'text', token.text)
                    ORDER BY token.position)
         FROM save_folder_token AS token) AS folder_tokens
"#
);

// The location recorded on this device with its account and folder labels, as
// coven's sync status row holds it, none while no storage is set up; then this
// person's access as the library records it, their member row's `account` and
// `access_key_id`.
sqlive_compose::fragment!(
    RecordedStorage {
        location: Option<coven::StorageConfig>,
        account: Option<String>,
        access_key_id: Option<String>
    },
    r#"
    SELECT status.location, member.account, member.access_key_id
    FROM coven_sync_status AS status
    LEFT JOIN coven_members AS member ON member.is_self = 1
"#
);

// This device's preferences: the `preferences` row, every save preset and the
// name patterns' pieces, in one row.
sqlive_compose::fragment!(
    PreferenceRows,
    r#"
    SELECT stored.*, presets.*, patterns.*
    FROM @crate::row::StoredPreferences AS stored
    JOIN @crate::row::StoredSavePresets AS presets ON 1
    JOIN @crate::row::StoredNamePatterns AS patterns ON 1
"#
);

sqlive_compose::read!(
    writer PreferencesRead,
    r#"
    SELECT preferences.* FROM @crate::row::PreferenceRows AS preferences
"#
);

// What the settings screens show: this device's preferences, the preset the
// last save used, and the storage coven's views record.
sqlive_compose::read!(
    writer SettingsRead,
    r#"
    SELECT preferences.*, last_saved.*, recorded.*
    FROM @crate::row::PreferenceRows AS preferences
    JOIN @crate::row::SavePresetsAmong(wanted: (
        SELECT held.last_save_preset AS id FROM preferences AS held)) AS last_saved ON 1
    JOIN @crate::row::RecordedStorage AS recorded ON 1
"#
);

// The preset `id`, none once no preset has the id.
sqlive_compose::read!(
    writer SavePresetRead(id: String),
    r#"
    SELECT presets.*
    FROM @crate::row::SavePresetsAmong(wanted: (SELECT :id AS id)) AS presets
"#
);

// Every save preset, with the id of the one a save panel shows chosen when
// the person has selected `selected`.
sqlive_compose::read!(
    writer SaveChoiceRead(selected: String) { chosen_id: Option<String> },
    r#"
    SELECT presets.*,
        COALESCE(
            (SELECT chosen.id FROM save_preset AS chosen WHERE chosen.id = :selected),
            (SELECT stored.last_save_preset FROM preferences AS stored)) AS chosen_id
    FROM @crate::row::StoredSavePresets AS presets
"#
);

/// This device's preferences for the library.
pub async fn preferences(store: &Store) -> Result<Preferences, DbError> {
    store.read(|sql| preferences_on(&sql)).await
}

/// This device's preferences for the library `store` holds, kept in
/// `library_path`, with the save preset the last save used, as the settings
/// screens read them.
pub async fn config(store: &Store, library_path: PathBuf) -> Result<Config, DbError> {
    let store_id = store.store_id();
    store.read(move |sql| config_on(&sql, store_id, library_path)).await
}

/// Follow [`config`]: the first value at once, then one after each write
/// that changes it.
pub fn subscribe_config(
    store: &Store,
    library_path: PathBuf,
) -> coven::LiveQuery<Config, bae_store::DbError> {
    let store_id = store.store_id();
    store.subscribe(move |sql| config_on(&sql, store_id, library_path.clone()))
}

fn config_on(
    sql: &impl bae_store::WriterRead,
    store_id: coven::StoreId,
    library_path: PathBuf,
) -> Result<Config, DbError> {
    let SettingsReadRow { preferences, last_saved, recorded } =
        statement_row(sql, &SettingsReadParams {})?;
    let storage_access =
        coven::MemberAccess::from_columns(recorded.account, recorded.access_key_id)
            .map_err(|error| DbError::unreadable("this person's access to storage", error))?;
    let prefs = preferences_of(preferences)?;
    let last_save_preset = one_save_preset(last_saved)?
        .ok_or_else(|| invalid("last_save_preset", &prefs.last_save_preset))?;
    Ok(Config {
        store_id,
        library_path,
        prefs,
        last_save_preset,
        storage: recorded.location,
        storage_access,
    })
}

/// Follow this device's preferences: the first value at once, then one
/// after each write that changes them.
pub fn subscribe_preferences(store: &Store) -> coven::LiveQuery<Preferences, bae_store::DbError> {
    store.subscribe(|sql| preferences_on(&sql))
}

/// Follow the save preset `id`: the first value at once, then one after each
/// write that changes it; none once no preset has the id.
pub fn subscribe_save_preset(
    store: &Store,
    id: String,
) -> coven::LiveQuery<Option<SavePreset>, bae_store::DbError> {
    store.subscribe(move |sql| {
        let request = SavePresetReadParams { id: id.clone() };
        one_save_preset(statement_row(&sql, &request)?.presets)
    })
}

/// One save format as a save panel's menu lists it, with whether the panel
/// shows it as chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveChoiceRow {
    pub preset: SavePreset,
    pub chosen: bool,
}

/// Follow the save formats in the order a save panel's menu lists them, with
/// the one the panel shows chosen when the person has selected the format
/// `selected`: that format while a row holds it, else the format the last
/// save used. The selection changes in place through the query's request, so
/// a selection that no row holds yet shows the last used format as chosen
/// until the row exists.
pub fn subscribe_save_choice(
    store: &Store,
    selected: String,
) -> coven::ReconfigurableLiveQuery<String, Vec<SaveChoiceRow>, bae_store::DbError> {
    store.subscribe_reconfigurable(selected, |selected, sql| {
        let request = SaveChoiceReadParams { selected: selected.clone() };
        let row = statement_row(&sql, &request)?;
        row.presets
            .every
            .save_presets
            .into_iter()
            .map(|columns| {
                let preset = save_preset_of(columns)?;
                let chosen = row.chosen_id.as_deref() == Some(preset.id.as_str());
                Ok(SaveChoiceRow { preset, chosen })
            })
            .collect()
    })
}

/// The owner of the `preferences` row: the one place it is written, one
/// edit at a time.
#[derive(Clone)]
pub struct PreferencesRow {
    store: Store,
}

impl PreferencesRow {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Edit the preferences in one write. `edit` is handed the stored
    /// preferences and may refuse the change, which then writes nothing; a
    /// change that leaves them as they were writes nothing either.
    pub async fn edit(
        &self,
        edit: impl FnOnce(&mut Preferences) -> Result<(), PreferencesError> + Send + 'static,
    ) -> Result<(), PreferencesError> {
        self.store
            .call(move |sql| {
                let stored = preferences_on(sql)?;
                let mut edited = stored.clone();
                if let Err(refused) = edit(&mut edited) {
                    return Ok(Err(refused));
                }
                write_preferences_on(sql, &stored, &edited)?;
                Ok(Ok(()))
            })
            .await?
    }
}

fn preferences_on(sql: &impl bae_store::WriterRead) -> Result<Preferences, DbError> {
    preferences_of(statement_row(sql, &PreferencesReadParams {})?.preferences)
}

/// This device's preferences as [`PreferenceRows`] holds them.
fn preferences_of(rows: PreferenceRows) -> Result<Preferences, DbError> {
    let PreferenceRows { stored, presets, patterns } = rows;
    let save_presets =
        presets.every.save_presets.into_iter().map(save_preset_of).collect::<Result<_, _>>()?;
    let save_filename_pattern = pattern_of(patterns.filename_tokens)?;
    let save_folder_pattern = pattern_of(patterns.folder_tokens)?;
    let subsonic_network =
        match (stored.subsonic_network.as_str(), stored.subsonic_network_interface) {
            ("this_machine", None) => SubsonicNetwork::ThisMachine,
            ("every", None) => SubsonicNetwork::Every,
            ("interface", Some(interface)) => SubsonicNetwork::Interface(interface),
            (other, _) => return Err(invalid("subsonic_network", other)),
        };
    Ok(Preferences {
        discogs: stored.discogs_key,
        replay_gain_mode: stored.replay_gain_mode,
        save_presets,
        save_filename_pattern,
        save_folder_pattern,
        last_save_preset: stored.last_save_preset,
        save_extras: SaveExtras {
            embed_cover: stored.save_embed_cover,
            cover_file: stored.save_cover_file,
            loudness_tags: stored.save_loudness_tags,
        },
        pause_between_sides: stored.pause_between_sides,
        side_pause_countdown: stored
            .side_pause_countdown_seconds
            .unwrap_or(SidePauseCountdown::Off),
        max_concurrent_uploads: stored.max_concurrent_uploads,
        max_concurrent_downloads: stored.max_concurrent_downloads,
        show_remaining_time: stored.show_remaining_time,
        library_full_width: stored.library_full_width,
        ask_before_unpinning: stored.ask_before_unpinning,
        import_destination: stored.import_destination,
        identification: IdentificationPreferences {
            automatic: stored.identify_automatically,
            import_when_identified: stored.import_when_identified,
            catalogs: LookupCatalogPreferences {
                musicbrainz: stored.ask_musicbrainz,
                discogs: stored.ask_discogs,
            },
        },
        cast_enabled: stored.cast_enabled,
        cast_transcode_format: stored.cast_transcode_format,
        mcp: McpConfig { enabled: stored.mcp_enabled, port: stored.mcp_port },
        subsonic: SubsonicConfig {
            enabled: stored.subsonic_enabled,
            port: stored.subsonic_port,
            username: stored.subsonic_username,
            network: subsonic_network,
        },
    })
}

/// The preset of `presets`, which a read of the presets under one id
/// returned; none when no preset has the id.
fn one_save_preset(presets: SavePresetsAmong) -> Result<Option<SavePreset>, DbError> {
    presets.save_presets.into_iter().next().map(save_preset_of).transpose()
}

/// One preset as the columns of its `save_preset` row hold it.
fn save_preset_of(columns: SavePresetColumns) -> Result<SavePreset, DbError> {
    let SavePresetColumns { id, name, codec, bit_depth, bitrate_kbps } = columns;
    let bit_depth = || match bit_depth.as_deref() {
        Some("same_as_source") => Ok(SaveBitDepth::SameAsSource),
        Some("bits_16") => Ok(SaveBitDepth::Bits16),
        other => Err(invalid("bit_depth", other)),
    };
    let bitrate_kbps = || bitrate_kbps.ok_or_else(|| invalid("bitrate_kbps", bitrate_kbps));
    Ok(SavePreset {
        codec: match codec.as_str() {
            "flac" => SaveCodec::Flac { bit_depth: bit_depth()? },
            "wav" => SaveCodec::Wav { bit_depth: bit_depth()? },
            "aiff" => SaveCodec::Aiff { bit_depth: bit_depth()? },
            "mp3" => SaveCodec::Mp3 { bitrate_kbps: bitrate_kbps()? },
            "aac" => SaveCodec::Aac { bitrate_kbps: bitrate_kbps()? },
            "opus_ogg" => SaveCodec::OpusOgg { bitrate_kbps: bitrate_kbps()? },
            _ => return Err(invalid("codec", codec)),
        },
        id,
        name,
    })
}

fn bit_depth_column(bit_depth: SaveBitDepth) -> &'static str {
    match bit_depth {
        SaveBitDepth::SameAsSource => "same_as_source",
        SaveBitDepth::Bits16 => "bits_16",
    }
}

/// Write what `edited` changed of `stored`: the row's columns, the
/// presets whole when any of them changed, and the filename pattern whole
/// when it changed.
fn write_preferences_on(
    sql: &CovenWrite<'_>,
    stored: &Preferences,
    edited: &Preferences,
) -> rusqlite::Result<()> {
    if edited == stored {
        return Ok(());
    }
    if edited.save_presets != stored.save_presets {
        sql.execute("DELETE FROM save_preset", [])?;
        for (position, preset) in edited.save_presets.iter().enumerate() {
            let (codec, bit_depth, bitrate_kbps) = match preset.codec {
                SaveCodec::Flac { bit_depth } => ("flac", Some(bit_depth_column(bit_depth)), None),
                SaveCodec::Wav { bit_depth } => ("wav", Some(bit_depth_column(bit_depth)), None),
                SaveCodec::Aiff { bit_depth } => ("aiff", Some(bit_depth_column(bit_depth)), None),
                SaveCodec::Mp3 { bitrate_kbps } => ("mp3", None, Some(bitrate_kbps)),
                SaveCodec::Aac { bitrate_kbps } => ("aac", None, Some(bitrate_kbps)),
                SaveCodec::OpusOgg { bitrate_kbps } => ("opus_ogg", None, Some(bitrate_kbps)),
            };
            sql.execute(
                "INSERT INTO save_preset (id, position, name, codec, bit_depth, bitrate_kbps) \
                 VALUES (?, ?, ?, ?, ?, ?)",
                params![preset.id, position as i64, preset.name, codec, bit_depth, bitrate_kbps,],
            )?;
        }
    }
    if edited.save_filename_pattern != stored.save_filename_pattern {
        write_pattern(sql, &edited.save_filename_pattern)?;
    }
    if edited.save_folder_pattern != stored.save_folder_pattern {
        write_pattern(sql, &edited.save_folder_pattern)?;
    }
    let Preferences {
        discogs,
        replay_gain_mode,
        save_presets: _,
        save_filename_pattern: _,
        save_folder_pattern: _,
        last_save_preset,
        save_extras,
        pause_between_sides,
        side_pause_countdown,
        max_concurrent_uploads,
        max_concurrent_downloads,
        show_remaining_time,
        library_full_width,
        ask_before_unpinning,
        import_destination,
        identification,
        cast_enabled,
        cast_transcode_format,
        mcp,
        subsonic,
    } = edited;
    sql.execute(
        "UPDATE preferences SET discogs_key = ?, replay_gain_mode = ?, \
             last_save_preset = ?, save_embed_cover = ?, save_cover_file = ?, \
             save_loudness_tags = ?, \
             pause_between_sides = ?, side_pause_countdown_seconds = ?, \
             max_concurrent_uploads = ?, max_concurrent_downloads = ?, \
             show_remaining_time = ?, library_full_width = ?, \
             ask_before_unpinning = ?, \
             import_destination = ?, identify_automatically = ?, \
             import_when_identified = ?, ask_musicbrainz = ?, ask_discogs = ?, \
             cast_enabled = ?, cast_transcode_format = ?, mcp_enabled = ?, mcp_port = ?, \
             subsonic_enabled = ?, subsonic_port = ?, subsonic_username = ?, \
             subsonic_network = ?, subsonic_network_interface = ?",
        params![
            discogs.map(|state| match state {
                DiscogsValidation::Unvalidated => "unvalidated",
                DiscogsValidation::Valid => "valid",
                DiscogsValidation::Rejected => "rejected",
            }),
            match replay_gain_mode {
                ReplayGainMode::Off => "off",
                ReplayGainMode::Track => "track",
                ReplayGainMode::Album => "album",
            },
            last_save_preset,
            save_extras.embed_cover,
            save_extras.cover_file,
            save_extras.loudness_tags,
            pause_between_sides,
            side_pause_countdown.duration().map(|duration| duration.as_secs() as i64),
            max_concurrent_uploads.get(),
            max_concurrent_downloads.get(),
            show_remaining_time,
            library_full_width,
            ask_before_unpinning,
            match import_destination {
                ImportDestination::ThisDevice => "this_device",
                ImportDestination::Cloud => "cloud",
            },
            identification.automatic,
            identification.import_when_identified,
            identification.catalogs.musicbrainz,
            identification.catalogs.discogs,
            cast_enabled,
            match cast_transcode_format {
                CastTranscodeFormat::Mp3 => "mp3",
                CastTranscodeFormat::Wav => "wav",
            },
            mcp.enabled,
            mcp.port,
            subsonic.enabled,
            subsonic.port,
            subsonic.username,
            match &subsonic.network {
                SubsonicNetwork::ThisMachine => "this_machine",
                SubsonicNetwork::Interface(_) => "interface",
                SubsonicNetwork::Every => "every",
            },
            match &subsonic.network {
                SubsonicNetwork::Interface(interface) => Some(interface.as_str()),
                SubsonicNetwork::ThisMachine | SubsonicNetwork::Every => None,
            },
        ],
    )?;
    Ok(())
}

/// A value a name pattern holds, and the table its pattern's pieces are
/// rows of.
trait PatternValue: Copy {
    const TABLE: &'static str;
    fn from_column(column: &str) -> Option<Self>;
    fn column(self) -> &'static str;
}

impl PatternValue for SaveFilenameValue {
    const TABLE: &'static str = "save_filename_token";

    fn from_column(column: &str) -> Option<Self> {
        Some(match column {
            "title" => Self::Title,
            "artist" => Self::Artist,
            "album" => Self::Album,
            "album_year" => Self::AlbumYear,
            "release_year" => Self::ReleaseYear,
            "track_number" => Self::TrackNumber,
            "disc_number" => Self::DiscNumber,
            "track_total" => Self::TrackTotal,
            _ => return None,
        })
    }

    fn column(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumYear => "album_year",
            Self::ReleaseYear => "release_year",
            Self::TrackNumber => "track_number",
            Self::DiscNumber => "disc_number",
            Self::TrackTotal => "track_total",
        }
    }
}

impl PatternValue for SaveFolderValue {
    const TABLE: &'static str = "save_folder_token";

    fn from_column(column: &str) -> Option<Self> {
        Some(match column {
            "artist" => Self::Artist,
            "album" => Self::Album,
            "album_year" => Self::AlbumYear,
            "release_year" => Self::ReleaseYear,
            "label" => Self::Label,
            "catalog_number" => Self::CatalogNumber,
            _ => return None,
        })
    }

    fn column(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumYear => "album_year",
            Self::ReleaseYear => "release_year",
            Self::Label => "label",
            Self::CatalogNumber => "catalog_number",
        }
    }
}

/// The pattern whose stored pieces, in order, are `tokens`, rows of
/// `Value`'s table.
fn pattern_of<Value: PatternValue>(
    tokens: Vec<NameTokenColumns>,
) -> Result<NamePattern<Value>, DbError> {
    let pieces = tokens
        .into_iter()
        .map(|token| match (token.value, token.text) {
            (Some(value), None) => Value::from_column(&value)
                .map(NamePiece::Value)
                .ok_or_else(|| invalid("value", value)),
            (None, Some(text)) => Ok(NamePiece::Text(text)),
            (value, text) => Err(invalid("value", (value, text))),
        })
        .collect::<Result<_, _>>()?;
    Ok(NamePattern::new(pieces))
}

/// Replace the stored pattern of `Value`'s table with `pattern`, piece by
/// piece.
fn write_pattern<Value: PatternValue>(
    sql: &CovenWrite<'_>,
    pattern: &NamePattern<Value>,
) -> rusqlite::Result<()> {
    sql.execute(&format!("DELETE FROM {}", Value::TABLE), [])?;
    for (position, piece) in pattern.pieces().iter().enumerate() {
        let (value, text) = match piece {
            NamePiece::Value(value) => (Some(value.column()), None),
            NamePiece::Text(text) => (None, Some(text.as_str())),
        };
        sql.execute(
            &format!("INSERT INTO {} (position, value, text) VALUES (?, ?, ?)", Value::TABLE),
            params![position as i64, value, text],
        )?;
    }
    Ok(())
}

/// A stored value the schema's checks should have refused.
fn invalid(column: &str, value: impl std::fmt::Debug) -> DbError {
    DbError::unreadable(format!("the preferences' {column}"), value)
}

/// A column's value that the schema's checks should have refused.
fn invalid_column(value: impl std::fmt::Debug) -> FromSqlError {
    FromSqlError::Other(format!("preferences hold {value:?}").into())
}

impl FromSql for DiscogsValidation {
    fn column_result(column: ValueRef<'_>) -> FromSqlResult<Self> {
        match column.as_str()? {
            "unvalidated" => Ok(Self::Unvalidated),
            "valid" => Ok(Self::Valid),
            "rejected" => Ok(Self::Rejected),
            other => Err(invalid_column(other)),
        }
    }
}

impl FromSql for ReplayGainMode {
    fn column_result(column: ValueRef<'_>) -> FromSqlResult<Self> {
        match column.as_str()? {
            "off" => Ok(Self::Off),
            "track" => Ok(Self::Track),
            "album" => Ok(Self::Album),
            other => Err(invalid_column(other)),
        }
    }
}

/// A stored countdown is its seconds; the column holds NULL for
/// [`SidePauseCountdown::Off`].
impl FromSql for SidePauseCountdown {
    fn column_result(column: ValueRef<'_>) -> FromSqlResult<Self> {
        match column.as_i64()? {
            5 => Ok(Self::Seconds5),
            15 => Ok(Self::Seconds15),
            30 => Ok(Self::Seconds30),
            45 => Ok(Self::Seconds45),
            60 => Ok(Self::Seconds60),
            other => Err(invalid_column(other)),
        }
    }
}

impl FromSql for ImportDestination {
    fn column_result(column: ValueRef<'_>) -> FromSqlResult<Self> {
        match column.as_str()? {
            "this_device" => Ok(Self::ThisDevice),
            "cloud" => Ok(Self::Cloud),
            other => Err(invalid_column(other)),
        }
    }
}

impl FromSql for CastTranscodeFormat {
    fn column_result(column: ValueRef<'_>) -> FromSqlResult<Self> {
        match column.as_str()? {
            "mp3" => Ok(Self::Mp3),
            "wav" => Ok(Self::Wav),
            other => Err(invalid_column(other)),
        }
    }
}

/// Plant damage for a test: the `preferences` row of the library stored
/// under `directory` names a replay gain mode no build knows. The column's
/// CHECK refuses that value, so no write stores it; a damaged file could
/// hold it, and the read checks for it.
#[cfg(any(test, feature = "test-utils"))]
pub fn plant_unknown_replay_gain_mode_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "preferences",
        "PRAGMA ignore_check_constraints = 1; \
         UPDATE preferences SET replay_gain_mode = 'unknown';",
    );
}

/// Plant damage for a test: every write of the `preferences` row of the
/// library stored under `directory` fails, as a full disk fails it.
#[cfg(any(test, feature = "test-utils"))]
pub fn fail_preferences_writes_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "preferences",
        "CREATE TRIGGER fail_preferences_write BEFORE UPDATE ON preferences \
         BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;",
    );
}

#[cfg(test)]
#[path = "row_tests.rs"]
mod tests;

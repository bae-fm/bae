//! This device's `preferences` row and its save presets: read by what each
//! setting decides, followed by the settings screens' live query, edited one
//! write at a time by the setters, and carried into the copy a restore over
//! this device's copy puts in its place.
//!
//! Its tables are `row.sql`.

use super::{
    CastTranscodeFormat, DiscogsValidation, IdentificationPreferences, ImportDestination,
    LookupCatalogPreferences, McpConfig, Preferences, PreferencesError, ReplayGainMode,
    SaveBitDepth, SaveCodec, SaveExtras, SaveFilenameToken, SaveFolderToken, SavePreset,
    SidePauseCountdown, SubsonicConfig,
};
use bae_store::{DbError, Store};
use coven::{BulkSqlContext, CovenWrite, SqlRead};
use rusqlite::{params, Params, Row};
use std::num::NonZeroU32;

/// The preferences' part of the library's schema.
pub(super) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("row.sql")]);

/// This device's preferences for the library.
pub async fn preferences(store: &Store) -> Result<Preferences, DbError> {
    store.read(|sql| Ok(preferences_on(&sql)?)).await
}

/// Follow this device's preferences: the first value at once, then one
/// after each write that changes them.
pub fn subscribe_preferences(store: &Store) -> coven::LiveQuery<Preferences, bae_store::DbError> {
    store.subscribe(|sql| Ok(preferences_on(&sql)?))
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

/// Write `carried`, the preferences of the copy a restore replaces, over the
/// new copy's own, in the restore's step on that copy: what the restore puts
/// in place keeps this device's settings.
pub fn carry_preferences(sql: &BulkSqlContext<'_>, carried: &Preferences) -> rusqlite::Result<()> {
    let stored = preferences_on(sql)?;
    write_preferences_on(sql, &stored, carried)
}

fn preferences_on(sql: &impl SqlRead) -> rusqlite::Result<Preferences> {
    let save_presets = save_presets_on(sql)?;
    let save_filename_tokens =
        sql.query("SELECT token FROM save_filename_token ORDER BY position", [], |row| {
            let token: String = row.get("token")?;
            filename_token(&token).ok_or_else(|| invalid(row, "token", token))
        })?;
    let save_folder_tokens =
        sql.query("SELECT token FROM save_folder_token ORDER BY position", [], |row| {
            let token: String = row.get("token")?;
            folder_token(&token).ok_or_else(|| invalid(row, "token", token))
        })?;
    sql.query_row("SELECT * FROM preferences", [], |row| {
        let flag = |column: &str| row.get::<_, bool>(column);
        let concurrency = |column: &str| {
            let value: u32 = row.get(column)?;
            NonZeroU32::new(value).ok_or_else(|| invalid(row, column, value))
        };
        Ok(Preferences {
            discogs: row
                .get::<_, Option<String>>("discogs_key")?
                .map(|state| match state.as_str() {
                    "unvalidated" => Ok(DiscogsValidation::Unvalidated),
                    "valid" => Ok(DiscogsValidation::Valid),
                    "rejected" => Ok(DiscogsValidation::Rejected),
                    _ => Err(invalid(row, "discogs_key", state)),
                })
                .transpose()?,
            replay_gain_mode: match row.get::<_, String>("replay_gain_mode")?.as_str() {
                "off" => ReplayGainMode::Off,
                "track" => ReplayGainMode::Track,
                "album" => ReplayGainMode::Album,
                other => return Err(invalid(row, "replay_gain_mode", other)),
            },
            save_presets,
            save_filename_tokens,
            save_folder_tokens,
            last_save_preset: row.get("last_save_preset")?,
            save_extras: SaveExtras {
                embed_cover: flag("save_embed_cover")?,
                cover_file: flag("save_cover_file")?,
                loudness_tags: flag("save_loudness_tags")?,
            },
            pause_between_sides: flag("pause_between_sides")?,
            side_pause_countdown: match row.get::<_, Option<u32>>("side_pause_countdown_seconds")? {
                None => SidePauseCountdown::Off,
                Some(5) => SidePauseCountdown::Seconds5,
                Some(15) => SidePauseCountdown::Seconds15,
                Some(30) => SidePauseCountdown::Seconds30,
                Some(45) => SidePauseCountdown::Seconds45,
                Some(60) => SidePauseCountdown::Seconds60,
                Some(other) => return Err(invalid(row, "side_pause_countdown_seconds", other)),
            },
            max_concurrent_uploads: concurrency("max_concurrent_uploads")?,
            max_concurrent_downloads: concurrency("max_concurrent_downloads")?,
            show_remaining_time: flag("show_remaining_time")?,
            library_full_width: flag("library_full_width")?,
            import_destination: match row.get::<_, String>("import_destination")?.as_str() {
                "this_device" => ImportDestination::ThisDevice,
                "cloud" => ImportDestination::Cloud,
                "cloud_pinned" => ImportDestination::CloudPinned,
                other => return Err(invalid(row, "import_destination", other)),
            },
            identification: IdentificationPreferences {
                automatic: flag("identify_automatically")?,
                import_when_identified: flag("import_when_identified")?,
                catalogs: LookupCatalogPreferences {
                    musicbrainz: flag("ask_musicbrainz")?,
                    discogs: flag("ask_discogs")?,
                },
            },
            cast_enabled: flag("cast_enabled")?,
            cast_transcode_format: match row.get::<_, String>("cast_transcode_format")?.as_str() {
                "mp3" => CastTranscodeFormat::Mp3,
                "wav" => CastTranscodeFormat::Wav,
                other => return Err(invalid(row, "cast_transcode_format", other)),
            },
            mcp: McpConfig { enabled: flag("mcp_enabled")?, port: row.get("mcp_port")? },
            subsonic: SubsonicConfig {
                enabled: flag("subsonic_enabled")?,
                port: row.get("subsonic_port")?,
                username: row.get("subsonic_username")?,
                bind_address: row.get("subsonic_bind_address")?,
            },
        })
    })
}

fn save_presets_on(sql: &impl SqlRead) -> rusqlite::Result<Vec<SavePreset>> {
    sql.query("SELECT * FROM save_preset ORDER BY position", [], |row| {
        let id: String = row.get("id")?;
        let codec: String = row.get("codec")?;
        let bit_depth = || -> rusqlite::Result<SaveBitDepth> {
            let stored: String = row.get("bit_depth")?;
            match stored.as_str() {
                "same_as_source" => Ok(SaveBitDepth::SameAsSource),
                "bits_16" => Ok(SaveBitDepth::Bits16),
                _ => Err(invalid(row, "bit_depth", stored)),
            }
        };
        let bitrate_kbps = || row.get::<_, u32>("bitrate_kbps");
        Ok(SavePreset {
            name: row.get("name")?,
            codec: match codec.as_str() {
                "flac" => SaveCodec::Flac { bit_depth: bit_depth()? },
                "wav" => SaveCodec::Wav { bit_depth: bit_depth()? },
                "aiff" => SaveCodec::Aiff { bit_depth: bit_depth()? },
                "mp3" => SaveCodec::Mp3 { bitrate_kbps: bitrate_kbps()? },
                "aac" => SaveCodec::Aac { bitrate_kbps: bitrate_kbps()? },
                "opus_ogg" => SaveCodec::OpusOgg { bitrate_kbps: bitrate_kbps()? },
                _ => return Err(invalid(row, "codec", codec)),
            },
            id,
        })
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
    sql: &impl Execute,
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
    if edited.save_filename_tokens != stored.save_filename_tokens {
        sql.execute("DELETE FROM save_filename_token", [])?;
        for (position, token) in edited.save_filename_tokens.iter().enumerate() {
            sql.execute(
                "INSERT INTO save_filename_token (position, token) VALUES (?, ?)",
                params![position as i64, filename_token_column(*token)],
            )?;
        }
    }
    if edited.save_folder_tokens != stored.save_folder_tokens {
        sql.execute("DELETE FROM save_folder_token", [])?;
        for (position, token) in edited.save_folder_tokens.iter().enumerate() {
            sql.execute(
                "INSERT INTO save_folder_token (position, token) VALUES (?, ?)",
                params![position as i64, folder_token_column(*token)],
            )?;
        }
    }
    let Preferences {
        discogs,
        replay_gain_mode,
        save_presets: _,
        save_filename_tokens: _,
        save_folder_tokens: _,
        last_save_preset,
        save_extras,
        pause_between_sides,
        side_pause_countdown,
        max_concurrent_uploads,
        max_concurrent_downloads,
        show_remaining_time,
        library_full_width,
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
             import_destination = ?, identify_automatically = ?, \
             import_when_identified = ?, ask_musicbrainz = ?, ask_discogs = ?, \
             cast_enabled = ?, cast_transcode_format = ?, mcp_enabled = ?, mcp_port = ?, \
             subsonic_enabled = ?, subsonic_port = ?, subsonic_username = ?, \
             subsonic_bind_address = ?",
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
            match import_destination {
                ImportDestination::ThisDevice => "this_device",
                ImportDestination::Cloud => "cloud",
                ImportDestination::CloudPinned => "cloud_pinned",
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
            subsonic.bind_address,
        ],
    )?;
    Ok(())
}

/// A statement in either of coven's writes the preferences are written in:
/// an ordinary write, and a bulk write on a new copy before it swaps in.
trait Execute {
    fn execute<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize>;
}

impl Execute for CovenWrite<'_> {
    fn execute<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        CovenWrite::execute(self, sql, params)
    }
}

impl Execute for BulkSqlContext<'_> {
    fn execute<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        BulkSqlContext::execute(self, sql, params)
    }
}

fn filename_token(column: &str) -> Option<SaveFilenameToken> {
    Some(match column {
        "title" => SaveFilenameToken::Title,
        "artist" => SaveFilenameToken::Artist,
        "album" => SaveFilenameToken::Album,
        "year" => SaveFilenameToken::Year,
        "track_number" => SaveFilenameToken::TrackNumber,
        "disc_number" => SaveFilenameToken::DiscNumber,
        "track_total" => SaveFilenameToken::TrackTotal,
        _ => return None,
    })
}

fn filename_token_column(token: SaveFilenameToken) -> &'static str {
    match token {
        SaveFilenameToken::Title => "title",
        SaveFilenameToken::Artist => "artist",
        SaveFilenameToken::Album => "album",
        SaveFilenameToken::Year => "year",
        SaveFilenameToken::TrackNumber => "track_number",
        SaveFilenameToken::DiscNumber => "disc_number",
        SaveFilenameToken::TrackTotal => "track_total",
    }
}

fn folder_token(column: &str) -> Option<SaveFolderToken> {
    Some(match column {
        "artist" => SaveFolderToken::Artist,
        "album" => SaveFolderToken::Album,
        "year" => SaveFolderToken::Year,
        "label" => SaveFolderToken::Label,
        "catalog_number" => SaveFolderToken::CatalogNumber,
        _ => return None,
    })
}

fn folder_token_column(token: SaveFolderToken) -> &'static str {
    match token {
        SaveFolderToken::Artist => "artist",
        SaveFolderToken::Album => "album",
        SaveFolderToken::Year => "year",
        SaveFolderToken::Label => "label",
        SaveFolderToken::CatalogNumber => "catalog_number",
    }
}

/// A stored value the schema's checks should have refused.
fn invalid(row: &Row<'_>, column: &str, value: impl std::fmt::Display) -> rusqlite::Error {
    bae_store::read::column_conversion_error(
        row,
        column,
        format!("preferences hold {value} in {column}"),
    )
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

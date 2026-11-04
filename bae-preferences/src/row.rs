//! This device's `preferences` row and its save presets: read by what each
//! setting decides, followed by the settings screens' live query, and edited
//! one write at a time by the setters.

use super::{
    CastTranscodeFormat, DiscogsValidation, IdentificationPreferences, ImportStoragePreferences,
    LookupCatalogPreferences, McpConfig, Preferences, PreferencesError, ReplayGainMode,
    SaveBitDepth, SaveCodec, SaveFilenameToken, SavePregapPlacement, SavePreset,
    SidePauseCountdown, SubsonicConfig,
};
use bae_db::client::query::{QueryOne, QueryRows};
use bae_db::{Database, DbError};
use coven::{CovenError, CovenWrite};
use rusqlite::{params, Row};
use std::collections::HashMap;
use std::num::NonZeroU32;

/// This device's preferences for the library.
pub async fn preferences(database: &Database) -> Result<Preferences, DbError> {
    database.read(|sql| Ok(preferences_on(&sql)?)).await
}

/// Follow this device's preferences: the first value at once, then one
/// after each write that changes them.
pub fn subscribe_preferences(database: &Database) -> coven::LiveQuery<Preferences> {
    database.subscribe(|sql| preferences_on(&sql).map_err(CovenError::from))
}

/// Edit the preferences in one write. `edit` is handed the stored
/// preferences and may refuse the change, which then writes nothing; a
/// change that leaves them as they were writes nothing either.
pub async fn edit_preferences(
    database: &Database,
    edit: impl FnOnce(&mut Preferences) -> Result<(), PreferencesError> + Send + 'static,
) -> Result<(), PreferencesError> {
    database
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

fn preferences_on(sql: &(impl QueryOne + QueryRows)) -> rusqlite::Result<Preferences> {
    let save_presets = save_presets_on(sql)?;
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
            default_track_save_preset: row.get("default_track_save_preset")?,
            default_release_save_preset: row.get("default_release_save_preset")?,
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
            import_storage: ImportStoragePreferences {
                cloud: flag("import_to_cloud")?,
                pinned: flag("import_pinned")?,
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

fn save_presets_on(sql: &impl QueryRows) -> rusqlite::Result<Vec<SavePreset>> {
    let tokens = sql.query(
        "SELECT preset_id, token FROM save_preset_filename_token ORDER BY preset_id, position",
        [],
        |row| {
            let token: String = row.get("token")?;
            Ok((
                row.get::<_, String>("preset_id")?,
                filename_token(&token).ok_or_else(|| invalid(row, "token", token))?,
            ))
        },
    )?;
    let mut tokens_by_preset: HashMap<String, Vec<SaveFilenameToken>> = HashMap::new();
    for (preset, token) in tokens {
        tokens_by_preset.entry(preset).or_default().push(token);
    }
    sql.query("SELECT * FROM save_preset ORDER BY position", [], |row| {
        let id: String = row.get("id")?;
        let codec: String = row.get("codec")?;
        let bit_depth = || -> rusqlite::Result<SaveBitDepth> {
            match row.get::<_, u32>("bit_depth")? {
                16 => Ok(SaveBitDepth::Bits16),
                24 => Ok(SaveBitDepth::Bits24),
                32 => Ok(SaveBitDepth::Bits32),
                other => Err(invalid(row, "bit_depth", other)),
            }
        };
        let bitrate_kbps = || row.get::<_, u32>("bitrate_kbps");
        let pregap: String = row.get("pregap_placement")?;
        Ok(SavePreset {
            filename_tokens: tokens_by_preset.remove(&id).unwrap_or_default(),
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
            pregap_placement: match pregap.as_str() {
                "append_to_previous_except_htoa" => SavePregapPlacement::AppendToPreviousExceptHtoa,
                "append_to_previous_including_htoa" => {
                    SavePregapPlacement::AppendToPreviousIncludingHtoa
                }
                "exclude" => SavePregapPlacement::Exclude,
                "single_file_with_cue" => SavePregapPlacement::SingleFileWithCue,
                _ => return Err(invalid(row, "pregap_placement", pregap)),
            },
            applies_to_track: row.get("applies_to_track")?,
            applies_to_release: row.get("applies_to_release")?,
            embed_cover: row.get("embed_cover")?,
            id,
        })
    })
}

/// Write what `edited` changed of `stored`: the row's columns, and the
/// presets whole when any of them changed.
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
                SaveCodec::Flac { bit_depth } => ("flac", Some(bit_depth.bits()), None),
                SaveCodec::Wav { bit_depth } => ("wav", Some(bit_depth.bits()), None),
                SaveCodec::Aiff { bit_depth } => ("aiff", Some(bit_depth.bits()), None),
                SaveCodec::Mp3 { bitrate_kbps } => ("mp3", None, Some(bitrate_kbps)),
                SaveCodec::Aac { bitrate_kbps } => ("aac", None, Some(bitrate_kbps)),
                SaveCodec::OpusOgg { bitrate_kbps } => ("opus_ogg", None, Some(bitrate_kbps)),
            };
            sql.execute(
                "INSERT INTO save_preset (id, position, name, codec, bit_depth, bitrate_kbps, \
                     pregap_placement, applies_to_track, applies_to_release, embed_cover) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    preset.id,
                    position as i64,
                    preset.name,
                    codec,
                    bit_depth,
                    bitrate_kbps,
                    match preset.pregap_placement {
                        SavePregapPlacement::AppendToPreviousExceptHtoa => {
                            "append_to_previous_except_htoa"
                        }
                        SavePregapPlacement::AppendToPreviousIncludingHtoa => {
                            "append_to_previous_including_htoa"
                        }
                        SavePregapPlacement::Exclude => "exclude",
                        SavePregapPlacement::SingleFileWithCue => "single_file_with_cue",
                    },
                    preset.applies_to_track,
                    preset.applies_to_release,
                    preset.embed_cover,
                ],
            )?;
            for (position, token) in preset.filename_tokens.iter().enumerate() {
                sql.execute(
                    "INSERT INTO save_preset_filename_token (preset_id, position, token) \
                     VALUES (?, ?, ?)",
                    params![preset.id, position as i64, filename_token_column(*token)],
                )?;
            }
        }
    }
    let Preferences {
        discogs,
        replay_gain_mode,
        save_presets: _,
        default_track_save_preset,
        default_release_save_preset,
        pause_between_sides,
        side_pause_countdown,
        max_concurrent_uploads,
        max_concurrent_downloads,
        show_remaining_time,
        library_full_width,
        import_storage,
        identification,
        cast_enabled,
        cast_transcode_format,
        mcp,
        subsonic,
    } = edited;
    sql.execute(
        "UPDATE preferences SET discogs_key = ?, replay_gain_mode = ?, \
             default_track_save_preset = ?, default_release_save_preset = ?, \
             pause_between_sides = ?, side_pause_countdown_seconds = ?, \
             max_concurrent_uploads = ?, max_concurrent_downloads = ?, \
             show_remaining_time = ?, library_full_width = ?, \
             import_to_cloud = ?, import_pinned = ?, identify_automatically = ?, \
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
            default_track_save_preset,
            default_release_save_preset,
            pause_between_sides,
            side_pause_countdown.duration().map(|duration| duration.as_secs() as i64),
            max_concurrent_uploads.get(),
            max_concurrent_downloads.get(),
            show_remaining_time,
            library_full_width,
            import_storage.cloud,
            import_storage.pinned,
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

/// A stored value the schema's checks should have refused.
fn invalid(row: &Row<'_>, column: &str, value: impl std::fmt::Display) -> rusqlite::Error {
    bae_db::client::read::column_conversion_error(
        row,
        column,
        format!("preferences hold {value} in {column}"),
    )
}

#[cfg(test)]
#[path = "row_tests.rs"]
mod tests;

//! This device's preferences for a library: its `preferences` row and save
//! presets, the types they hold, and the row's SQL.

#[cfg(any(test, feature = "test-utils"))]
use bae_app_dir::default_transfer_concurrency;
use bae_app_dir::usize_bound;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::path::PathBuf;

mod catalog_availability;
mod identification;
mod import_destination;
mod name_pattern;
mod presentation;
mod row;
mod save;
mod server;

pub use catalog_availability::{
    asked_sources, asks_a_source, is_the_only_asked_source, CatalogAvailability, SourceAvailability,
};
pub use identification::{IdentificationPreferences, LookupCatalogPreferences};
pub use import_destination::ImportDestination;
pub use name_pattern::{NamePattern, NamePatternError, NamePiece};
pub use presentation::LookupCatalogSetting;
pub use row::{carry_preferences, preferences, subscribe_preferences, PreferencesRow};
#[cfg(any(test, feature = "test-utils"))]
pub use row::{fail_preferences_writes_for_test, plant_unknown_replay_gain_mode_for_test};
pub use save::{
    SaveBitDepth, SaveCodec, SaveExtras, SaveFilenamePattern, SaveFilenameValue, SaveFolderPattern,
    SaveFolderValue, SaveFormat, SavePreset,
};
pub use server::{
    McpConfig, SubsonicConfig, SubsonicCredential, MCP_DEFAULT_PORT, SUBSONIC_DEFAULT_PORT,
};

#[cfg(any(test, feature = "test-utils"))]
use save::default_save_presets;

/// The preferences' part of the library's schema.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] = &[row::SCHEMA];

/// The schema this crate's tests open their stores on: the preferences' part.
#[cfg(test)]
const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[SCHEMA_PARTS]);

/// The largest transfer concurrency the UI offers and the setters accept.
pub const MAX_CONCURRENT_TRANSFERS: u32 = 8;

/// A file-transfer concurrency setting must be at least 1 — coven's drain admits
/// nothing at 0 and never completes — and at most [`MAX_CONCURRENT_TRANSFERS`],
/// the ceiling the UI offers. Returns the validated value for storage.
pub fn validate_concurrency(n: u32) -> Result<NonZeroU32, PreferencesError> {
    NonZeroU32::new(n).filter(|n| n.get() <= MAX_CONCURRENT_TRANSFERS).ok_or_else(|| {
        PreferencesError::Invalid(format!(
            "transfer concurrency must be between 1 and {MAX_CONCURRENT_TRANSFERS}"
        ))
    })
}

/// The validation state of a stored Discogs API key. Only carried when a key
/// exists (`Config::discogs` is `Some`). Distinct from `DiscogsTokenStatus`:
/// this only describes a key that exists, whereas the status folds in the
/// no-key case for the UI.
///
/// - `Unvalidated` — a key is stored but Discogs hasn't confirmed it yet,
///   because it was saved while offline. Used optimistically; re-checked when
///   possible.
/// - `Valid` — Discogs accepted the key.
/// - `Rejected` — Discogs returned 401 for the key. Not used until re-saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscogsValidation {
    Unvalidated,
    Valid,
    Rejected,
}

#[cfg(desktop)]
impl DiscogsValidation {
    /// The validation a key is stored under once Discogs was asked about it:
    /// a check made offline leaves it unvalidated.
    pub fn of(check: &bae_catalogs::providers::DiscogsCheck) -> Self {
        use bae_catalogs::providers::DiscogsCheck;
        match check {
            DiscogsCheck::Valid => Self::Valid,
            DiscogsCheck::Rejected => Self::Rejected,
            DiscogsCheck::Offline(_) => Self::Unvalidated,
        }
    }
}

/// How loudness normalization is applied at playback.
///
/// - `Off` — no normalization; tracks play at their stored level (unity gain).
/// - `Track` — normalize each track to the target using its own loudness.
/// - `Album` — normalize whole albums to the target using album loudness, so
///   the loudness relationship between an album's tracks is preserved.
///
/// The gain is derived at playback from the stored loudness measurements and a
/// constant target; this only selects which measurement (track vs album) drives
/// it. Defaults to `Off`. No settings screen offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayGainMode {
    Off,
    Track,
    Album,
}

/// What bae converts a track to for a Cast or UPnP device that cannot be
/// served the track as stored: a window of a CUE image, a track whose pregap
/// lives in another file or is generated, or a codec the device doesn't
/// decode. A device flavor that doesn't take WAV gets MP3 whatever this says
/// (see `RendererFlavor::stream_format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastTranscodeFormat {
    /// MP3 at 320 kbps.
    Mp3,
    /// Uncompressed PCM at the depth the track was stored at: lossless, and
    /// several times the data of MP3.
    Wav,
}

/// How long a side or disc pause waits before the next side starts on its
/// own — exactly the choices the settings offer. `Off` waits for Play.
///
/// Read only when `pause_between_sides` is on; the choice is kept while that
/// setting is off, so turning pausing back on brings the countdown back too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidePauseCountdown {
    Off,
    Seconds5,
    Seconds15,
    Seconds30,
    Seconds45,
    Seconds60,
}

impl SidePauseCountdown {
    /// How long the pause lasts before the next side starts, or `None` when it
    /// waits for Play.
    pub fn duration(self) -> Option<std::time::Duration> {
        let seconds = match self {
            Self::Off => return None,
            Self::Seconds5 => 5,
            Self::Seconds15 => 15,
            Self::Seconds30 => 30,
            Self::Seconds45 => 45,
            Self::Seconds60 => 60,
        };
        Some(std::time::Duration::from_secs(seconds))
    }
}

/// Whether a usable Discogs API key is configured. Folds the no-key case and
/// the validation state into the four states a UI shows, so each binding
/// doesn't re-derive the precedence.
#[derive(Serialize, Deserialize)]
pub enum DiscogsTokenStatus {
    NotConfigured,
    Valid,
    Unvalidated,
    Rejected,
}

impl DiscogsTokenStatus {
    /// Whether Discogs can be used as a metadata source. A stored key is usable
    /// optimistically unless Discogs has rejected it. The single source of truth
    /// for this policy — both the macOS and Windows UIs read a flag derived from
    /// it rather than re-deciding which states count as usable.
    pub fn is_usable(&self) -> bool {
        matches!(self, DiscogsTokenStatus::Valid | DiscogsTokenStatus::Unvalidated)
    }
}

impl Preferences {
    /// The Discogs key's state for display: not configured, or the stored key's
    /// validation state.
    pub fn discogs_token_status(&self) -> DiscogsTokenStatus {
        match self.discogs {
            None => DiscogsTokenStatus::NotConfigured,
            Some(DiscogsValidation::Valid) => DiscogsTokenStatus::Valid,
            Some(DiscogsValidation::Unvalidated) => DiscogsTokenStatus::Unvalidated,
            Some(DiscogsValidation::Rejected) => DiscogsTokenStatus::Rejected,
        }
    }

    /// Which metadata sources this library asks, one entry per
    /// [`LookupCatalog`](bae_catalogs::catalog::LookupCatalog).
    ///
    /// The one answer every place that asks the sources together reads — a
    /// run's provider list, a typed search's per-source parts, the switches a
    /// surface renders. Two things decide it and they are not the same
    /// question: whether the person wants the source asked, and whether this
    /// library holds what it needs. The person's switch comes first: a source
    /// switched off is `Off`, and one switched on without its credential
    /// reports `NotConfigured`, which is the surface's cue to ask for it.
    ///
    /// A fact about the stored preferences and nothing else, which is why it
    /// lives here: the bridge builds the list a surface renders out of the
    /// same value it reads every other setting off.
    pub fn metadata_sources(&self) -> Vec<CatalogAvailability> {
        bae_catalogs::catalog::LookupCatalog::ALL
            .into_iter()
            .map(|catalog| CatalogAvailability {
                catalog,
                state: if !self.identification.catalogs.enabled(catalog) {
                    SourceAvailability::Off
                } else if !self.source_is_configured(catalog) {
                    SourceAvailability::NotConfigured
                } else {
                    SourceAvailability::On
                },
            })
            .collect()
    }

    /// Whether this library holds the credentials `catalog` needs.
    /// MusicBrainz's API is open, so it needs none; Discogs needs a key it has
    /// not rejected. Total over the asked catalogs, so a new one has to state
    /// what it needs.
    fn source_is_configured(&self, catalog: bae_catalogs::catalog::LookupCatalog) -> bool {
        match catalog {
            bae_catalogs::catalog::LookupCatalog::MusicBrainz => true,
            bae_catalogs::catalog::LookupCatalog::Discogs => {
                self.discogs_token_status().is_usable()
            }
        }
    }
}

/// Why a preference could not be changed: the change was refused, or the
/// `preferences` row could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum PreferencesError {
    #[error("invalid configuration: {0}")]
    Invalid(String),
    /// The preferences row could not be read or written.
    #[error(transparent)]
    Database(#[from] bae_store::DbError),
}

/// A bug the preferences row's read or write carried back, which the store
/// reported as it returned.
impl bae_util::task_failure::ReportedBug for PreferencesError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Invalid(_) => None,
            Self::Database(error) => error.reported_bug(),
        }
    }
}

impl PreferencesError {
    /// The class the person is shown: a row that can't be read or written
    /// is the database's failure, not a setting to fix.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Invalid(_) => bae_diagnostics::UiErrorCategory::Config,
            Self::Database(error) => bae_diagnostics::database_category(error),
        }
    }
}

/// This device's settings for the library: its `preferences` row and save
/// presets (Appendix C), never synced, since a concurrency limit reflects one
/// machine's link and CPU and a server one machine's network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    /// The stored Discogs key's validation state, or `None` when no key is
    /// configured. `Some` doubles as the hint that a key is in the keyring, so
    /// settings render without a keyring read.
    pub discogs: Option<DiscogsValidation>,
    /// How loudness normalization is applied at playback. Defaults to `Off`.
    pub replay_gain_mode: ReplayGainMode,
    /// Configured export presets offered by release and track export.
    pub save_presets: Vec<SavePreset>,
    /// The one file name pattern every save names its files by, whatever the
    /// format, stored as typed: one that can't make a name is kept, and saves
    /// name by the default pattern until it is fixed.
    pub save_filename_pattern: SaveFilenamePattern,
    /// The folder name pattern a release save names its folder by, stored as
    /// typed like the file name pattern.
    pub save_folder_pattern: SaveFolderPattern,
    /// The cover and loudness settings every save format shares.
    pub save_extras: SaveExtras,
    /// Id of the preset the last save, of a track or a release, used; the
    /// one a save starts with. A required, valid preset id (config validation
    /// keeps it non-dangling).
    pub last_save_preset: String,
    /// Whether playback pauses between vinyl/cassette sides and CD discs.
    /// Defaults to `true`: a side break is part of how the release was made,
    /// so bae plays it unless the person turns it off.
    pub pause_between_sides: bool,
    /// Whether a side or disc pause ends on its own after a countdown, and how
    /// long it is. Defaults to `Off`: the pause waits for Play.
    pub side_pause_countdown: SidePauseCountdown,
    /// How many file uploads coven's upload drain runs at once.
    pub max_concurrent_uploads: NonZeroU32,
    /// How many file downloads a pin fetches at once.
    pub max_concurrent_downloads: NonZeroU32,
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. Defaults to `false` (elapsed).
    pub show_remaining_time: bool,
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. Defaults to `false`
    /// (capped).
    pub library_full_width: bool,
    /// Where an import puts its release: this device, the cloud, or the cloud
    /// with a whole copy kept here.
    pub import_destination: ImportDestination,
    /// How identification runs: on its own or not, what an automatic run goes
    /// on to do, and the catalogs it asks.
    pub identification: IdentificationPreferences,
    /// Whether casting to a network receiver (Cast, UPnP, AirPlay) is available.
    /// Defaults to `false`: casting browses the local network and serves audio
    /// off this machine, so it stays off until the user asks for it. While off,
    /// no discovery runs and no cast session can be started.
    pub cast_enabled: bool,
    /// What a track a Cast or UPnP device can't be served as stored is
    /// converted to. Defaults to `Mp3`.
    pub cast_transcode_format: CastTranscodeFormat,
    /// Local automation server configuration. The bearer token is keyring-only.
    pub mcp: McpConfig,
    /// Subsonic/OpenSubsonic server settings (`enabled`, `port`, `username`).
    /// The password is keyring-only, like the MCP bearer token; the server
    /// controller combines this `username` with it into the runtime credential.
    pub subsonic: SubsonicConfig,
}

impl Preferences {
    /// The file transfers the library's store runs at once, per direction.
    pub fn transfer_limits(&self) -> coven::TransferLimits {
        coven::TransferLimits {
            uploads: usize_bound(self.max_concurrent_uploads),
            downloads: usize_bound(self.max_concurrent_downloads),
        }
    }
}

/// The preferences a new library's row starts with, as the schema writes
/// them, for tests that build preferences without a library.
#[cfg(any(test, feature = "test-utils"))]
impl Default for Preferences {
    fn default() -> Self {
        Self {
            discogs: None,
            replay_gain_mode: ReplayGainMode::Off,
            save_presets: default_save_presets(),
            save_filename_pattern: SaveFilenamePattern::default(),
            save_folder_pattern: SaveFolderPattern::default(),
            save_extras: SaveExtras::default(),
            last_save_preset: "flac".to_string(),
            pause_between_sides: true,
            side_pause_countdown: SidePauseCountdown::Off,
            max_concurrent_uploads: default_transfer_concurrency(),
            max_concurrent_downloads: default_transfer_concurrency(),
            show_remaining_time: false,
            library_full_width: false,
            import_destination: ImportDestination::default(),
            identification: IdentificationPreferences::default(),
            cast_enabled: false,
            cast_transcode_format: CastTranscodeFormat::Mp3,
            mcp: McpConfig::disabled_default(),
            subsonic: SubsonicConfig::disabled_default(),
        }
    }
}

/// This device's preferences for an open library, with the library they
/// belong to and bae's directory for it, as the settings screens read them.
/// Store names, device identity and storage settings are owned and read by
/// coven.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub store_id: coven::StoreId,
    pub library_path: PathBuf,
    pub prefs: Preferences,
}

impl Config {
    /// The default preferences for `store_id`, kept in `library_path`, for
    /// tests that build a value without a library.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_defaults(store_id: coven::StoreId, library_path: impl Into<PathBuf>) -> Self {
        Self { store_id, library_path: library_path.into(), prefs: Preferences::default() }
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

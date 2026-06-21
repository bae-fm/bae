//! C ABI over bae-core for the native Windows app (WinUI 3 / C#).
//!
//! Windows binds bae-core through this hand-written `extern "C"` surface, the way
//! macOS/Android bind it through uniffi (bae-bridge): every platform reaches the
//! same core with its own binding. C# holds an opaque handle pointer and
//! P/Invokes these entry points; strings this library returns are owned by it and
//! must be released with [`bae_string_free`].
//!
//! This is the binding foundation — methods are added as the WinUI app needs them.

use std::{
    ffi::{c_char, CStr, CString},
    sync::{OnceLock, RwLock},
};

use bae_core::album_detail::ReleaseStorageState;
use bae_core::app::{bootstrap, RunningApp};
use bae_core::db::{AlbumSortCriterion, AlbumSortField, SortDirection};
use bae_core::diagnostics::{
    AppDiagnosticMetadata, DatadogDiagnosticsConfig, DiagnosticLevel, Diagnostics,
    DiagnosticsConfig,
};
use bae_core::ui::UiBusEvent;
use serde::{Deserialize, Serialize};
use tracing_subscriber::{prelude::*, util::SubscriberInitExt};

mod loc;

static DIAGNOSTICS: OnceLock<RwLock<Diagnostics>> = OnceLock::new();
static LOGGING_INSTALLED: OnceLock<()> = OnceLock::new();

fn diagnostics_cell() -> &'static RwLock<Diagnostics> {
    DIAGNOSTICS.get_or_init(|| RwLock::new(Diagnostics::noop()))
}

fn current_diagnostics() -> Diagnostics {
    diagnostics_cell()
        .read()
        .expect("diagnostics lock poisoned")
        .clone()
}

fn replace_diagnostics(diagnostics: Diagnostics) {
    *diagnostics_cell()
        .write()
        .expect("diagnostics lock poisoned") = diagnostics;
}

fn windows_env_filter() -> Result<tracing_subscriber::EnvFilter, String> {
    match std::env::var("RUST_LOG") {
        Err(std::env::VarError::NotPresent) => Ok(tracing_subscriber::EnvFilter::new("info")),
        Err(std::env::VarError::NotUnicode(_)) => Err("RUST_LOG is not valid Unicode".to_string()),
        Ok(value) => tracing_subscriber::EnvFilter::try_new(&value)
            .map_err(|e| format!("RUST_LOG={value:?} is malformed: {e}")),
    }
}

fn install_logging(diagnostics: Diagnostics) -> Result<(), String> {
    if LOGGING_INSTALLED.get().is_some() {
        return Ok(());
    }

    let filter = windows_env_filter()?;
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(bae_core::diagnostics::tracing_layer(diagnostics))
        .with(
            tracing_subscriber::fmt::layer()
                .with_line_number(true)
                .with_target(false)
                .with_file(true),
        );
    if let Err(error) = subscriber.try_init() {
        tracing::debug!(%error, "tracing subscriber already installed");
    }
    let _ = LOGGING_INSTALLED.set(());
    Ok(())
}

/// Allocate an owned C string for a catalog key result, or null when the value
/// has no key (the C# falls back to a passthrough / generic line). The result
/// is freed with [`bae_string_free`], like every other string this library
/// returns.
fn key_cstring(key: Option<&str>) -> *mut c_char {
    match key {
        // Catalog keys are static `core.…` identifiers with no interior NUL, so
        // `CString::new` can't fail here; `expect` surfaces a catalog bug rather
        // than masking it as the null "no key" result the `None` arm returns.
        Some(key) => CString::new(key)
            .expect("catalog key has no interior NUL byte")
            .into_raw(),
        None => std::ptr::null_mut(),
    }
}

/// Catalog key for a cloud provider's display name, or null for the brand-name
/// providers the UI passes through verbatim. `provider` is the wire tag
/// ("s3"/"google_drive"/…) or null for local-only. Mirrors macOS's uniffi
/// `bridge_cloud_provider_label_key`. Free with [`bae_string_free`].
///
/// # Safety
/// `provider` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_cloud_provider_label_key(provider: *const c_char) -> *mut c_char {
    let provider = cstr(provider);
    key_cstring(loc::cloud_provider_label_key(provider.as_deref()))
}

/// Catalog key for a channel count's word ("mono"/"stereo"), or null for counts
/// the C# renders as "{n}ch". Mirrors `bridge_audio_channels_key`. Free with
/// [`bae_string_free`].
#[no_mangle]
pub extern "C" fn bae_audio_channels_key(channels: i64) -> *mut c_char {
    key_cstring(loc::audio_channels_key(channels))
}

/// Catalog key for a diagnostic error category's generic line, or null for an
/// unknown tag. `category` is the wire tag carried by an `FfiError`
/// (`{"kind":"diagnostic","category":…}`). Mirrors `bridge_error_category_key`.
/// Free with [`bae_string_free`].
///
/// # Safety
/// `category` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_error_category_key(category: *const c_char) -> *mut c_char {
    let Some(category) = cstr(category) else {
        return std::ptr::null_mut();
    };
    key_cstring(loc::error_category_key(&category))
}

/// Catalog key for a missing entity's "… not found" line, or null for an
/// unknown tag. `entity` is the wire tag carried by an `FfiError`
/// (`{"kind":"not_found","entity":…}`). Mirrors `bridge_entity_not_found_key`.
/// Free with [`bae_string_free`].
///
/// # Safety
/// `entity` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_entity_not_found_key(entity: *const c_char) -> *mut c_char {
    let Some(entity) = cstr(entity) else {
        return std::ptr::null_mut();
    };
    key_cstring(loc::entity_not_found_key(&entity))
}

/// Catalog key for an actionable playback-error reason, or null for the
/// `diagnostic` reason (which renders through the error-category path) and
/// unknown tags. `kind` is the wire tag carried by an `FfiPlaybackErrorReason`.
/// Mirrors `bridge_playback_error_reason_key`. Free with [`bae_string_free`].
///
/// # Safety
/// `kind` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_playback_error_reason_key(kind: *const c_char) -> *mut c_char {
    let Some(kind) = cstr(kind) else {
        return std::ptr::null_mut();
    };
    key_cstring(loc::playback_error_reason_key(&kind))
}

/// Catalog key for an import prepare-step wire tag (`FfiImportStep.Preparing`),
/// or null for an unknown tag. Mirrors `bridge_prepare_step_key`. Free with
/// [`bae_string_free`].
///
/// # Safety
/// `step` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_prepare_step_key(step: *const c_char) -> *mut c_char {
    let Some(step) = cstr(step) else {
        return std::ptr::null_mut();
    };
    key_cstring(loc::prepare_step_key(&step))
}

/// Catalog key for an import-phase wire tag (`FfiImportStep.Running`), or null
/// for an unknown tag. Mirrors `bridge_import_phase_key`. Free with
/// [`bae_string_free`].
///
/// # Safety
/// `phase` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_import_phase_key(phase: *const c_char) -> *mut c_char {
    let Some(phase) = cstr(phase) else {
        return std::ptr::null_mut();
    };
    key_cstring(loc::import_phase_key(&phase))
}

/// Catalog key for a transfer action's present-continuous progress verb, or
/// null for an unknown tag. `action` is a wire tag from `FfiStorageRow.actions`
/// ("pin"/"unpin"/"manage"/"unmanage"). Mirrors `bridge_transfer_action_key`.
/// Free with [`bae_string_free`].
///
/// # Safety
/// `action` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_transfer_action_key(action: *const c_char) -> *mut c_char {
    let Some(action) = cstr(action) else {
        return std::ptr::null_mut();
    };
    key_cstring(loc::transfer_action_key(&action))
}

/// Opaque app handle. Created by [`bae_init`], passed back to every call, freed
/// with [`bae_handle_free`].
pub struct BaeHandle(RunningApp);

/// Read a borrowed C string into an owned `String`, or `None` if null/not UTF-8.
///
/// # Safety
/// `ptr` must be null or a valid NUL-terminated C string.
unsafe fn cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(str::to_owned)
}

/// Read a required borrowed C string into an owned `String`.
///
/// # Safety
/// `ptr` must be a valid NUL-terminated UTF-8 C string.
unsafe fn required_cstr(ptr: *const c_char, name: &str) -> Result<String, String> {
    optional_cstr(ptr, name)?.ok_or_else(|| format!("{name} is required"))
}

/// Read an optional borrowed C string into an owned `String`.
///
/// # Safety
/// `ptr` must be null or a valid NUL-terminated UTF-8 C string.
unsafe fn optional_cstr(ptr: *const c_char, name: &str) -> Result<Option<String>, String> {
    if ptr.is_null() {
        return Ok(None);
    }
    CStr::from_ptr(ptr)
        .to_str()
        .map(|value| Some(value.to_owned()))
        .map_err(|_| format!("{name} is not valid UTF-8"))
}

fn configured_value(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.starts_with("$(") {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Initialize the app for `library_id`. Returns a handle pointer, or null on
/// failure (the error is logged). Free the result with [`bae_handle_free`].
///
/// # Safety
/// `library_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_init(
    library_id: *const c_char,
    position_update_interval_ms: u32,
) -> *mut BaeHandle {
    let Some(library_id) = cstr(library_id) else {
        tracing::error!("bae_init: null or non-UTF-8 library_id");
        return std::ptr::null_mut();
    };
    match bootstrap(library_id, position_update_interval_ms) {
        Ok(app) => Box::into_raw(Box::new(BaeHandle(app))),
        Err(e) => {
            tracing::error!("bae_init failed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// One-time process startup: register the OS credential store (Windows
/// Credential Manager) before any key is read or written. Call once at launch,
/// before [`bae_libraries`] / [`bae_create_library`] / [`bae_init`].
#[no_mangle]
pub extern "C" fn bae_startup() {
    bae_core::config::init_keyring();
}

fn app_diagnostic_metadata_from_parts(
    service: String,
    environment: Option<String>,
    app_version: String,
    edition: String,
    git_commit: Option<String>,
) -> Option<AppDiagnosticMetadata> {
    Some(AppDiagnosticMetadata {
        service,
        environment: environment?,
        app_version,
        edition,
        git_commit: git_commit?,
    })
}

fn diagnostics_config_from_parts(
    datadog_site: Option<String>,
    client_token: Option<String>,
    source: String,
    app: Option<AppDiagnosticMetadata>,
) -> DiagnosticsConfig {
    let Some(app) = app else {
        return DiagnosticsConfig::Disabled;
    };

    if app.edition != "bae" {
        return DiagnosticsConfig::Disabled;
    }

    let (Some(datadog_site), Some(client_token)) = (datadog_site, client_token) else {
        return DiagnosticsConfig::Disabled;
    };

    DiagnosticsConfig::Enabled(DatadogDiagnosticsConfig {
        datadog_site,
        client_token,
        source,
        app,
    })
}

/// Configure process diagnostics. Empty/missing Datadog settings and baeium
/// builds install no-op diagnostics. Returns null on success, else an error
/// string. Free with [`bae_string_free`].
///
/// # Safety
/// String pointers must be null where optional or valid NUL-terminated UTF-8 C
/// strings.
#[no_mangle]
pub unsafe extern "C" fn bae_configure_diagnostics(
    datadog_site: *const c_char,
    client_token: *const c_char,
    source: *const c_char,
    service: *const c_char,
    environment: *const c_char,
    app_version: *const c_char,
    edition: *const c_char,
    git_commit: *const c_char,
) -> *mut c_char {
    let result: Result<(), String> = (|| {
        let source = required_cstr(source, "source")?;
        let app = app_diagnostic_metadata_from_parts(
            required_cstr(service, "service")?,
            configured_value(optional_cstr(environment, "environment")?),
            required_cstr(app_version, "app_version")?,
            required_cstr(edition, "edition")?,
            configured_value(optional_cstr(git_commit, "git_commit")?),
        );
        let config = diagnostics_config_from_parts(
            configured_value(optional_cstr(datadog_site, "datadog_site")?),
            configured_value(optional_cstr(client_token, "client_token")?),
            source,
            app,
        );
        let diagnostics =
            Diagnostics::configure(config).map_err(|e| format!("diagnostics setup failed: {e}"))?;
        install_logging(diagnostics.clone())?;
        replace_diagnostics(diagnostics);
        Ok(())
    })();

    match result {
        Ok(()) => std::ptr::null_mut(),
        Err(error) => error_cstring(&error),
    }
}

#[derive(Deserialize)]
struct FfiDiagnosticField {
    key: String,
    value: String,
}

fn diagnostic_fields_from_json(fields_json: &str) -> Result<Vec<(String, String)>, String> {
    serde_json::from_str::<Vec<FfiDiagnosticField>>(fields_json)
        .map(|fields| {
            fields
                .into_iter()
                .map(|field| (field.key, field.value))
                .collect()
        })
        .map_err(|e| format!("invalid diagnostic fields JSON: {e}"))
}

fn diagnostic_level_from_str(level: &str) -> Option<DiagnosticLevel> {
    match level {
        "trace" => Some(DiagnosticLevel::Trace),
        "debug" => Some(DiagnosticLevel::Debug),
        "info" => Some(DiagnosticLevel::Info),
        "warn" => Some(DiagnosticLevel::Warn),
        "error" => Some(DiagnosticLevel::Error),
        _ => None,
    }
}

/// Emit a host-originated log event. Returns null on success, else an error
/// string. Free with [`bae_string_free`].
///
/// # Safety
/// String pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_diagnostics_log(
    level: *const c_char,
    target: *const c_char,
    message: *const c_char,
    fields_json: *const c_char,
) -> *mut c_char {
    let result: Result<(), String> = (|| {
        let level = required_cstr(level, "level")?;
        let Some(level) = diagnostic_level_from_str(&level) else {
            return Err(format!("unknown diagnostic level: {level}"));
        };
        let fields = diagnostic_fields_from_json(&required_cstr(fields_json, "fields_json")?)?;
        current_diagnostics()
            .log(
                level,
                required_cstr(target, "target")?,
                required_cstr(message, "message")?,
                fields,
            )
            .map_err(|e| e.to_string())
    })();

    match result {
        Ok(()) => std::ptr::null_mut(),
        Err(error) => error_cstring(&error),
    }
}

/// Emit a host-originated telemetry event. Returns null on success, else an
/// error string. Free with [`bae_string_free`].
///
/// # Safety
/// String pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_diagnostics_event(
    name: *const c_char,
    fields_json: *const c_char,
) -> *mut c_char {
    let result: Result<(), String> = (|| {
        let fields = diagnostic_fields_from_json(&required_cstr(fields_json, "fields_json")?)?;
        current_diagnostics()
            .event(required_cstr(name, "name")?, fields)
            .map_err(|e| e.to_string())
    })();

    match result {
        Ok(()) => std::ptr::null_mut(),
        Err(error) => error_cstring(&error),
    }
}

/// Flush queued diagnostics. Returns null on success, else an error string.
/// Free with [`bae_string_free`].
#[no_mangle]
pub extern "C" fn bae_flush_diagnostics() -> *mut c_char {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => return error_cstring(&format!("diagnostics flush runtime: {e}")),
    };
    match runtime.block_on(current_diagnostics().flush()) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// Register the host's OAuth client credentials so coven can build authorization
/// URLs and refresh tokens for the cloud providers that need them (Google Drive,
/// Dropbox, OneDrive). `creds_json` is an object keyed by provider name
/// (`"google_drive"` / `"dropbox"` / `"onedrive"`), each value carrying at least
/// `client_id` and an optional `client_secret`:
///
/// ```json
/// { "google_drive": { "client_id": "<id>", "client_secret": null } }
/// ```
///
/// coven and bae ship no credentials of their own — the app registers its own at
/// launch, before any OAuth flow (sign-in, restore). Extra fields (e.g. a
/// `redirect_uri` the mobile builds carry) are ignored: the desktop loopback flow
/// builds its own `127.0.0.1` redirect. Returns null on success, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `creds_json` must be a valid NUL-terminated UTF-8 C string.
#[cfg(feature = "oauth-providers")]
#[no_mangle]
pub unsafe extern "C" fn bae_set_oauth_client_creds(creds_json: *const c_char) -> *mut c_char {
    let Some(creds_json) = cstr(creds_json) else {
        return error_cstring("oauth creds json is null or not valid UTF-8");
    };
    let parsed: std::collections::HashMap<String, serde_json::Value> =
        match serde_json::from_str(&creds_json) {
            Ok(parsed) => parsed,
            Err(e) => return error_cstring(&format!("invalid oauth creds json: {e}")),
        };
    let mut creds = std::collections::HashMap::new();
    for (provider, value) in parsed {
        let Some(client_id) = value.get("client_id").and_then(|v| v.as_str()) else {
            return error_cstring(&format!("oauth creds for {provider} missing client_id"));
        };
        let client_secret = value
            .get("client_secret")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        creds.insert(
            provider,
            bae_core::oauth::OAuthClientCreds {
                client_id: client_id.to_string(),
                client_secret,
            },
        );
    }
    if creds.is_empty() {
        return error_cstring("oauth creds json registered no providers");
    }
    bae_core::oauth::set_oauth_client_creds(creds);
    std::ptr::null_mut()
}

/// A library discovered under the bae data directory.
#[derive(Serialize)]
struct FfiLibrary {
    id: String,
    name: String,
    is_active: bool,
}

/// The discovered libraries as a JSON array of `{id, name, is_active}`. Free the
/// result with [`bae_string_free`].
#[no_mangle]
pub extern "C" fn bae_libraries() -> *mut c_char {
    let libraries: Vec<FfiLibrary> = bae_core::config::Config::discover_libraries()
        .into_iter()
        .map(|library| FfiLibrary {
            id: library.id,
            name: library.name,
            is_active: library.is_active,
        })
        .collect();
    json_cstring(&libraries)
}

/// Create a new library (with a generated name). Returns its id as a C string,
/// or null on error. Free with [`bae_string_free`]. Requires [`bae_startup`].
#[no_mangle]
pub extern "C" fn bae_create_library() -> *mut c_char {
    match bae_core::library::create_library_default(&bae_core::id_provider::UuidProvider) {
        Ok(config) => error_cstring(&config.library_id),
        Err(e) => {
            tracing::error!("bae_create_library failed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Whether this library's encryption key is loaded. False for an encrypted
/// library whose key isn't in the OS credential store on this device — the UI
/// prompts the user to unlock with [`bae_unlock_library`], then re-init.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_has_encryption_key(handle: *const BaeHandle) -> bool {
    let Some(handle) = handle.as_ref() else {
        return false;
    };
    handle.0.services.library_manager().has_encryption()
}

/// Store a library's 64-char hex encryption key in the OS credential store so it
/// can be opened. Pre-init (takes the library id, not a handle); re-init the
/// library afterward to bring sync online. Returns null on success, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `library_id` and `key_hex` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_unlock_library(
    library_id: *const c_char,
    key_hex: *const c_char,
) -> *mut c_char {
    let (Some(library_id), Some(key_hex)) = (cstr(library_id), cstr(key_hex)) else {
        return error_cstring("invalid unlock argument");
    };
    match bae_core::library::unlock_library(&library_id, &key_hex) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// Lock the active library: forget its encryption key on this device. Sync stops
/// until it's unlocked again; local files stay. Returns null on success, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_lock_active_library(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    match handle.0.services.library_manager().forget_encryption_key() {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Rename a library by id (the active one or an inactive one — the core renames
/// the live library in memory or the on-disk config as appropriate). Returns
/// null on success, or an error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `library_id` and `name` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_rename_library(
    handle: *const BaeHandle,
    library_id: *const c_char,
    name: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (Some(library_id), Some(name)) = (cstr(library_id), cstr(name)) else {
        return error_cstring("invalid rename argument");
    };
    match handle
        .0
        .services
        .library_manager()
        .rename_library(&library_id, &name)
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Set an album's primary (canonical) release — the one shown first and used as
/// the album's default. Returns null on success, or an error-message C string
/// (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `album_id` and `release_id` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_set_primary_release(
    handle: *const BaeHandle,
    album_id: *const c_char,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (Some(album_id), Some(release_id)) = (cstr(album_id), cstr(release_id)) else {
        return error_cstring("invalid set-primary argument");
    };
    let app = &handle.0;
    match app.runtime.block_on(
        app.services
            .library_manager()
            .set_album_primary_release(&album_id, &release_id),
    ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// Export one track's audio to `output_path`. `format` is "flac" (lossless) or
/// "mp3" (320 kbps). Returns null on success, or an error-message C string (free
/// with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `track_id`, `output_path`, and `format` must be valid NUL-terminated UTF-8 C
/// strings.
#[no_mangle]
pub unsafe extern "C" fn bae_export_track(
    handle: *const BaeHandle,
    track_id: *const c_char,
    output_path: *const c_char,
    format: *const c_char,
) -> *mut c_char {
    use bae_core::library::ExportFormat;

    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (Some(track_id), Some(output_path), Some(format)) =
        (cstr(track_id), cstr(output_path), cstr(format))
    else {
        return error_cstring("invalid export argument");
    };
    let format = match format.as_str() {
        "flac" => ExportFormat::Flac,
        "mp3" => ExportFormat::Mp3 {
            bitrate: bae_core::library::MP3_EXPORT_BITRATE,
        },
        other => return error_cstring(&format!("unknown export format: {other}")),
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.library_manager().export_track(
            &track_id,
            std::path::Path::new(&output_path),
            format,
        )) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// One of a release's local image files, offered as a cover-art choice. The UI
/// loads the thumbnail from [`bae_image_path`] of `id`.
#[derive(Serialize)]
struct FfiReleaseImage {
    id: String,
    original_filename: String,
}

/// A release's local image files (cover-art candidates) as a JSON array of
/// `{id, original_filename}`, or null on error. Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_get_release_images(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_get_release_images: null handle");
        return std::ptr::null_mut();
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_get_release_images: null or non-UTF-8 release_id");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let files = match app.runtime.block_on(
        app.services
            .library_manager()
            .get_files_for_release(&release_id),
    ) {
        Ok(files) => files,
        Err(e) => {
            tracing::error!("bae_get_release_images failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let images: Vec<FfiReleaseImage> = files
        .into_iter()
        .filter(|file| file.content_type.is_image())
        .map(|file| FfiReleaseImage {
            id: file.id,
            original_filename: file.original_filename,
        })
        .collect();
    json_cstring(&images)
}

/// A remote cover-art candidate from an external metadata source. `source` is
/// the wire name ("musicbrainz" / "discogs") the UI passes back to
/// [`bae_change_cover`].
#[derive(Serialize)]
struct FfiRemoteCover {
    url: String,
    thumbnail_url: String,
    label: String,
    source: String,
}

fn remote_cover_to_ffi(cover: &bae_core::import::cover_art::RemoteCover) -> FfiRemoteCover {
    FfiRemoteCover {
        url: cover.url.clone(),
        thumbnail_url: cover.thumbnail_url.clone(),
        label: cover.label.clone(),
        source: cover.source.as_str().to_string(),
    }
}

/// A local image file in an import candidate's folder, offered as a cover
/// choice in the import confirmation picker. `file_id` is the folder-relative
/// path the import worker matches when this cover is selected (passed back as a
/// `release_image` `FfiCoverSelection`); `path` is the absolute on-disk path
/// the UI loads the thumbnail from.
#[derive(Serialize)]
struct FfiLocalArtwork {
    file_id: String,
    path: String,
}

/// The import confirmation seed: the editor's raw form plus the cover-art
/// choices the user can pick before committing. Returned by
/// [`bae_prefetch_candidate_edit`].
#[derive(Serialize)]
struct FfiPrefetchedEdit {
    /// The raw edit form, seeded from the chosen release's metadata.
    edit: bae_core::import::RawReleaseEdit,
    /// Remote cover art carried by the prefetched release detail.
    remote_covers: Vec<FfiRemoteCover>,
    /// Image files discovered in the candidate's import folder.
    local_artwork: Vec<FfiLocalArtwork>,
}

/// Remote cover-art candidates for a release (MusicBrainz / Discogs) as a JSON
/// array of `{url, thumbnail_url, label, source}`, or null on error. Free with
/// [`bae_string_free`]. Performs network I/O — call off the UI thread.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_fetch_remote_covers(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_fetch_remote_covers: null handle");
        return std::ptr::null_mut();
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_fetch_remote_covers: null or non-UTF-8 release_id");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let covers = match app
        .runtime
        .block_on(app.services.import().fetch_remote_covers(&release_id))
    {
        Ok(covers) => covers,
        Err(e) => {
            tracing::error!("bae_fetch_remote_covers failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let covers: Vec<FfiRemoteCover> = covers
        .into_iter()
        .map(|cover| remote_cover_to_ffi(&cover))
        .collect();
    json_cstring(&covers)
}

/// The UI's choice of cover art, decoded from `selection_json` in
/// [`bae_change_cover`] and the import confirmation's `selected_cover_json` in
/// [`bae_import_candidate`]. One wire shape decoded into either core cover-type:
/// [`bae_core::library::CoverSelection`] for a library cover change, or
/// [`bae_core::import::CoverSelection`] for an import (see
/// [`ffi_cover_to_import`]).
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FfiCoverSelection {
    /// An image file already in the library (change-cover) or in the import
    /// folder (import) — `file_id` is the library file id, respectively the
    /// folder-relative path of the picked artwork.
    ReleaseImage { file_id: String },
    /// A remote cover URL from a metadata source ("musicbrainz" / "discogs").
    RemoteCover { url: String, source: String },
}

/// Decode an `FfiCoverSelection` into the import-time
/// [`bae_core::import::CoverSelection`]: `ReleaseImage`'s `file_id` is the
/// folder-relative path the import worker matches against the scanned files;
/// `RemoteCover` carries the URL and its source for download + attribution.
/// `Err` carries a user-facing message for an unknown source tag.
fn ffi_cover_to_import(
    selection: FfiCoverSelection,
) -> Result<bae_core::import::CoverSelection, String> {
    use bae_core::import::CoverSelection;
    Ok(match selection {
        FfiCoverSelection::ReleaseImage { file_id } => CoverSelection::Local(file_id),
        FfiCoverSelection::RemoteCover { url, source } => {
            let source = source.parse::<bae_core::import::MetadataSource>()?;
            CoverSelection::Remote(url, source)
        }
    })
}

/// Change an album release's cover art. `selection_json` is one of
/// `{"type":"release_image","file_id":"…"}` or
/// `{"type":"remote_cover","url":"…","source":"musicbrainz"}`. Returns null on
/// success, or an error-message C string (free with [`bae_string_free`]).
/// Performs network I/O for a remote cover — call off the UI thread.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `album_id`, `release_id`, and `selection_json` must be valid NUL-terminated
/// UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_change_cover(
    handle: *const BaeHandle,
    album_id: *const c_char,
    release_id: *const c_char,
    selection_json: *const c_char,
) -> *mut c_char {
    use bae_core::import::MetadataSource;
    use bae_core::library::CoverSelection;

    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (Some(album_id), Some(release_id), Some(selection_json)) =
        (cstr(album_id), cstr(release_id), cstr(selection_json))
    else {
        return error_cstring("invalid change-cover argument");
    };
    let selection = match serde_json::from_str::<FfiCoverSelection>(&selection_json) {
        Ok(FfiCoverSelection::ReleaseImage { file_id }) => CoverSelection::ReleaseImage { file_id },
        Ok(FfiCoverSelection::RemoteCover { url, source }) => {
            let source = match source.parse::<MetadataSource>() {
                Ok(source) => source,
                Err(e) => return error_cstring(&e),
            };
            CoverSelection::RemoteCover { url, source }
        }
        Err(e) => return error_cstring(&format!("invalid cover selection: {e}")),
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(
            app.services
                .library_manager()
                .change_cover(&album_id, &release_id, selection),
        ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// What a restore code decodes to, before committing to a restore.
#[derive(Serialize)]
struct FfiRestoreCodeInfo {
    library_id: String,
    library_name: String,
    provider: String,
    /// Whether restoring this library needs an OAuth sign-in (Google Drive,
    /// Dropbox, OneDrive). The caller runs `bae_oauth_authorize` for the
    /// provider and passes the token JSON to [`bae_restore_from_code`].
    needs_oauth: bool,
}

/// Decode a restore code into its `{library_id, library_name, provider,
/// needs_oauth}` (without restoring), as JSON, or null when the code is
/// malformed. Free with [`bae_string_free`]. No handle — runs before [`bae_init`].
///
/// # Safety
/// `code` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_decode_restore_code(code: *const c_char) -> *mut c_char {
    let Some(code) = cstr(code) else {
        tracing::error!("bae_decode_restore_code: null or non-UTF-8 code");
        return std::ptr::null_mut();
    };
    match bae_core::sync::restore_code::decode_restore_code_info(&code) {
        Ok(info) => json_cstring(&FfiRestoreCodeInfo {
            library_id: info.library_id,
            library_name: info.library_name,
            provider: cloud_provider_name(&info.cloud_provider).to_string(),
            needs_oauth: info.needs_oauth,
        }),
        Err(e) => {
            tracing::error!("bae_decode_restore_code failed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// The outcome of a restore: the new library's id on success, or an error
/// message.
#[derive(Serialize)]
struct FfiRestoreResult {
    library_id: Option<String>,
    error: Option<String>,
}

/// The outcome of an OAuth flow: the provider's token JSON to hand on to a
/// restore, or a message describing why it failed (denied, cancelled, timed
/// out, network).
#[cfg(feature = "oauth-providers")]
#[derive(Serialize)]
struct FfiOAuthResult {
    token: Option<String>,
    error: Option<String>,
}

/// Run the desktop OAuth flow for `provider` (`"google_drive"` / `"dropbox"` /
/// `"onedrive"`) and return `{token, error}` as JSON: `token` is the provider's
/// token JSON to pass to [`bae_restore_from_code`], `error` the
/// reason it failed. The core opens the system browser and runs a `127.0.0.1`
/// callback listener, blocking until the user authorizes, cancels, or it times
/// out — call off the UI thread. Requires client credentials registered via
/// [`bae_set_oauth_client_creds`]. No handle — runs before [`bae_init`]. Free
/// with [`bae_string_free`].
///
/// # Safety
/// `provider` must be a valid NUL-terminated UTF-8 C string.
#[cfg(feature = "oauth-providers")]
#[no_mangle]
pub unsafe extern "C" fn bae_oauth_authorize(provider: *const c_char) -> *mut c_char {
    let Some(provider) = cstr(provider) else {
        return json_cstring(&FfiOAuthResult {
            token: None,
            error: Some("invalid provider".to_string()),
        });
    };
    let Some(core_provider) = oauth_provider_from_str(&provider) else {
        return json_cstring(&FfiOAuthResult {
            token: None,
            error: Some(format!("unknown or unsupported provider: {provider}")),
        });
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            tracing::error!("bae_oauth_authorize: runtime: {e}");
            return json_cstring(&FfiOAuthResult {
                token: None,
                error: Some("couldn't start the sign-in runtime".to_string()),
            });
        }
    };
    // The cancel sender must outlive the flow: coven's authorize() treats the
    // watch channel closing as a cancellation, so dropping the sender early would
    // abort the flow immediately. Windows exposes no cancel button yet — the
    // sender stays untouched (value never set true) until the flow returns.
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let clock = bae_core::clock::SystemClock;
    let result = runtime.block_on(bae_core::oauth::authorize_provider(
        core_provider,
        cancel_rx,
        &clock,
    ));
    drop(cancel_tx);
    let out = match result {
        Ok(tokens) => match serde_json::to_string(&tokens) {
            Ok(token) => FfiOAuthResult {
                token: Some(token),
                error: None,
            },
            Err(e) => FfiOAuthResult {
                token: None,
                error: Some(format!("couldn't serialize tokens: {e}")),
            },
        },
        Err(e) => FfiOAuthResult {
            token: None,
            error: Some(e.to_string()),
        },
    };
    json_cstring(&out)
}

/// Restore a library from a restore code. For OAuth providers (Google Drive,
/// Dropbox, OneDrive) the caller first runs `bae_oauth_authorize` and passes
/// the resulting token JSON as `oauth_token_json`; for credential providers it
/// passes null. Pulls the library from the cloud, then returns `{library_id,
/// error}` as JSON for the caller to [`bae_init`]. No handle — runs before
/// [`bae_init`]. Free with [`bae_string_free`].
///
/// # Safety
/// `code` must be a valid NUL-terminated UTF-8 C string; `oauth_token_json` must
/// be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_restore_from_code(
    code: *const c_char,
    oauth_token_json: *const c_char,
) -> *mut c_char {
    let Some(code) = cstr(code) else {
        return json_cstring(&FfiRestoreResult {
            library_id: None,
            error: Some("invalid restore code".to_string()),
        });
    };
    let oauth_tokens = match cstr(oauth_token_json) {
        Some(json) => match serde_json::from_str::<bae_core::oauth::OAuthTokens>(&json) {
            Ok(tokens) => Some(tokens),
            Err(e) => {
                return json_cstring(&FfiRestoreResult {
                    library_id: None,
                    error: Some(format!("invalid oauth token json: {e}")),
                });
            }
        },
        None => None,
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            tracing::error!("bae_restore_from_code: runtime: {e}");
            return json_cstring(&FfiRestoreResult {
                library_id: None,
                error: Some("couldn't start the restore runtime".to_string()),
            });
        }
    };
    let result = runtime.block_on(bae_core::library::restore_from_code(
        &code,
        oauth_tokens,
        None,
        |status| tracing::info!("restore: {status}"),
    ));
    let out = match result {
        Ok(config) => FfiRestoreResult {
            library_id: Some(config.library_id.clone()),
            error: None,
        },
        Err(e) => FfiRestoreResult {
            library_id: None,
            error: Some(e),
        },
    };
    json_cstring(&out)
}

/// A manually-entered cloud source to restore from, when there's no restore code
/// — the user types every connection detail, including the secrets a shareable
/// code can't carry. Decoded from the `source_json` of [`bae_restore_from_cloud`].
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FfiRestoreSource {
    S3 {
        bucket: String,
        region: String,
        /// A custom endpoint for S3-compatible stores; `None`/empty for AWS.
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
    },
}

/// Restore a library by entering its cloud location and credentials directly,
/// rather than from a restore code. `library_id` and `encryption_key_hex`
/// identify and unlock the library; `library_name` names the local copy (empty
/// generates one); `source_json` is an `FfiRestoreSource`. Pulls the library
/// from the cloud, then returns `{library_id, error}` as JSON for the caller to
/// [`bae_init`]. Blocks on the pull — call off the UI thread. No handle — runs
/// before [`bae_init`]. Free with [`bae_string_free`].
///
/// # Safety
/// `library_id`, `encryption_key_hex`, and `source_json` must be valid
/// NUL-terminated UTF-8 C strings; `library_name` must be null or one.
#[no_mangle]
pub unsafe extern "C" fn bae_restore_from_cloud(
    library_id: *const c_char,
    encryption_key_hex: *const c_char,
    library_name: *const c_char,
    source_json: *const c_char,
) -> *mut c_char {
    let (Some(library_id), Some(encryption_key_hex), Some(source_json)) = (
        cstr(library_id),
        cstr(encryption_key_hex),
        cstr(source_json),
    ) else {
        return json_cstring(&FfiRestoreResult {
            library_id: None,
            error: Some("invalid restore argument".to_string()),
        });
    };
    let library_name = cstr(library_name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(bae_core::library_name::generate_library_name);
    let source = match serde_json::from_str::<FfiRestoreSource>(&source_json) {
        Ok(FfiRestoreSource::S3 {
            bucket,
            region,
            endpoint,
            access_key,
            secret_key,
        }) => bae_core::sync::restore::RestoreSource::S3 {
            bucket,
            region,
            endpoint: endpoint.filter(|e| !e.trim().is_empty()),
            access_key,
            secret_key,
        },
        Err(e) => {
            return json_cstring(&FfiRestoreResult {
                library_id: None,
                error: Some(format!("invalid restore source: {e}")),
            });
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            tracing::error!("bae_restore_from_cloud: runtime: {e}");
            return json_cstring(&FfiRestoreResult {
                library_id: None,
                error: Some("couldn't start the restore runtime".to_string()),
            });
        }
    };
    let result = runtime.block_on(bae_core::library::restore_from_cloud(
        &library_id,
        &encryption_key_hex,
        &library_name,
        source,
        |status| tracing::info!("restore: {status}"),
    ));
    let out = match result {
        Ok(config) => FfiRestoreResult {
            library_id: Some(config.library_id.clone()),
            error: None,
        },
        Err(e) => FfiRestoreResult {
            library_id: None,
            error: Some(e),
        },
    };
    json_cstring(&out)
}

/// Number of albums in the library, or -1 on error.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_album_count(handle: *const BaeHandle) -> i64 {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_album_count: null handle");
        return -1;
    };
    let app = &handle.0;
    let result = app
        .runtime
        .block_on(app.services.library_manager().get_album_count());
    match result {
        Ok(count) => count as i64,
        Err(e) => {
            tracing::error!("bae_album_count failed: {e}");
            -1
        }
    }
}

/// One album in a library page, as the WinUI grid renders it. `cover_path` is a
/// cache-bustable identifier (`<path>#v=<mtime_secs>`) the C# `CoverImage.Load`
/// resolves, or null when the album has no cover cached on disk.
#[derive(Serialize)]
struct FfiAlbum {
    id: String,
    title: String,
    artist: String,
    cover_path: Option<String>,
}

/// Serialize `value` to a JSON C string the caller frees with [`bae_string_free`],
/// or null on failure (logged).
fn json_cstring<T: Serialize>(value: &T) -> *mut c_char {
    let json = match serde_json::to_string(value) {
        Ok(json) => json,
        Err(e) => {
            tracing::error!("FFI JSON serialize failed: {e}");
            return std::ptr::null_mut();
        }
    };
    match CString::new(json) {
        Ok(cstring) => cstring.into_raw(),
        Err(e) => {
            tracing::error!("FFI JSON had an interior NUL: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Parse a wire sort tag into an [`AlbumSortField`].
fn album_sort_field_from_str(field: &str) -> Option<AlbumSortField> {
    match field {
        "title" => Some(AlbumSortField::Title),
        "artist" => Some(AlbumSortField::Artist),
        "year" => Some(AlbumSortField::Year),
        "date_added" => Some(AlbumSortField::DateAdded),
        _ => None,
    }
}

/// A page of albums sorted by `sort_field` (`title` / `artist` / `year` /
/// `date_added`) in the given direction, as a JSON array of
/// `{id, title, artist, cover_path}`. Returns null on error. Free the result with
/// [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `sort_field` must be null or a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_album_page(
    handle: *const BaeHandle,
    offset: u64,
    limit: u64,
    sort_field: *const c_char,
    ascending: bool,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_album_page: null handle");
        return std::ptr::null_mut();
    };
    let field = cstr(sort_field)
        .as_deref()
        .and_then(album_sort_field_from_str)
        .unwrap_or_else(|| {
            tracing::warn!(
                "bae_album_page: missing or unknown sort field, defaulting to date_added"
            );
            AlbumSortField::DateAdded
        });
    let direction = if ascending {
        SortDirection::Ascending
    } else {
        SortDirection::Descending
    };
    let app = &handle.0;
    let manager = app.services.library_manager();
    let sort = [AlbumSortCriterion { field, direction }];
    let albums = match app
        .runtime
        .block_on(manager.get_album_page(&sort, offset, limit))
    {
        Ok(albums) => albums,
        Err(e) => {
            tracing::error!("bae_album_page failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let page: Vec<FfiAlbum> = albums
        .into_iter()
        .map(|album| FfiAlbum {
            id: album.id,
            title: album.title,
            artist: album.artist_names,
            cover_path: album.cover_path,
        })
        .collect();
    json_cstring(&page)
}

/// Album results for `query` as the same JSON shape as [`bae_album_page`]
/// (`{id, title, artist, cover_path}`), so the grid renders them directly.
/// Returns null on error. Free the result with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `query` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_search(handle: *const BaeHandle, query: *const c_char) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_search: null handle");
        return std::ptr::null_mut();
    };
    let Some(query) = cstr(query) else {
        tracing::error!("bae_search: null or non-UTF-8 query");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let manager = app.services.library_manager();
    let results = match app.runtime.block_on(manager.search_library(&query, 50)) {
        Ok(results) => results,
        Err(e) => {
            tracing::error!("bae_search failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let albums: Vec<FfiAlbum> = results
        .albums
        .into_iter()
        .map(|album| {
            let cover_path = manager.image_path_if_exists(&album.primary_release_id);
            FfiAlbum {
                id: album.id,
                title: album.title,
                artist: album.artist_name,
                cover_path,
            }
        })
        .collect();
    json_cstring(&albums)
}

/// One image in a release's gallery (lightbox).
#[derive(Serialize)]
struct FfiGalleryItem {
    label: String,
    path: String,
}

/// A release's gallery images (cover + artwork) as a JSON array of
/// `{label, path}`, or null on error / not found. Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_gallery(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_gallery: null handle");
        return std::ptr::null_mut();
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_gallery: null or non-UTF-8 release_id");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let detail = match app.runtime.block_on(
        app.services
            .library_manager()
            .find_release_detail(&release_id),
    ) {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            tracing::warn!("bae_gallery: release {release_id} not found");
            return std::ptr::null_mut();
        }
        Err(e) => {
            tracing::error!("bae_gallery failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let items: Vec<FfiGalleryItem> = detail
        .gallery_items
        .into_iter()
        // Local images only on Windows (the desktop pins releases on view); a
        // cloud-only item has no local path here, so it's dropped rather than
        // shown as a blank entry.
        .filter_map(|item| {
            item.local_path.map(|path| FfiGalleryItem {
                label: item.label,
                path,
            })
        })
        .collect();
    json_cstring(&items)
}

/// The wire name for a storage action.
fn storage_action_name(action: &bae_core::album_detail::ReleaseStorageAction) -> &'static str {
    use bae_core::album_detail::ReleaseStorageAction;
    match action {
        ReleaseStorageAction::Pin => "pin",
        ReleaseStorageAction::Unpin => "unpin",
        ReleaseStorageAction::Manage => "manage",
        ReleaseStorageAction::Unmanage => "unmanage",
    }
}

/// One release row in the storage manager.
#[derive(Serialize)]
struct FfiStorageRow {
    release_id: String,
    album_title: String,
    artist: String,
    format: Option<String>,
    /// Raw total size in bytes; the C# formats it for the locale.
    total_size: i64,
    file_count: i64,
    /// Storage state wire tag: "unmanaged" / "pinned" / "cloud_only". The C#
    /// resolves a localized label per tag.
    state: &'static str,
    /// The storage transitions this release allows right now, gated on cloud
    /// home by the core. The in-flight-uploads gate lives in the UI: it reads
    /// `OutboxSnapshot.per_release[release_id]` and suppresses these actions
    /// when the release has uploads in flight.
    actions: Vec<String>,
}

/// Wire tag for a release's storage state. The C# resolves a localized label.
fn storage_state_tag(state: ReleaseStorageState) -> &'static str {
    match state {
        ReleaseStorageState::Unmanaged => "unmanaged",
        ReleaseStorageState::Pinned => "pinned",
        ReleaseStorageState::CloudOnly => "cloud_only",
    }
}

/// Every release's storage summary as a JSON array, or null on error. Free the
/// result with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_storage(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_storage: null handle");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let manager = app.services.library_manager();
    let summaries = match app
        .runtime
        .block_on(manager.get_release_storage_summaries())
    {
        Ok(summaries) => summaries,
        Err(e) => {
            tracing::error!("bae_storage failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let rows: Vec<FfiStorageRow> = summaries
        .into_iter()
        .map(|summary| {
            // Actions are computed by the core resolver (gated on the live
            // cloud-home + pending uploads); the FFI just maps them to wire names.
            let actions = summary
                .storage_actions
                .iter()
                .map(|action| storage_action_name(action).to_string())
                .collect();
            FfiStorageRow {
                release_id: summary.release_id,
                album_title: summary.album_title,
                artist: summary.artist_names,
                format: summary.format,
                total_size: summary.total_size,
                file_count: summary.file_count,
                state: storage_state_tag(summary.storage_state),
                actions,
            }
        })
        .collect();
    json_cstring(&rows)
}

/// Queue a cloud-only release to be pinned locally — the in-memory download
/// queue fetches it in the background. Returns null once enqueued, or an
/// error-message C string for a bad argument (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_pin_release(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(release_id) = cstr(release_id) else {
        return error_cstring("invalid release id");
    };
    let app = &handle.0;
    app.runtime.block_on(
        app.services
            .library_manager()
            .enqueue_pins(vec![release_id]),
    );
    std::ptr::null_mut()
}

/// Unpin a release (drop the local copy, keep it in the cloud). Returns null on
/// success, or an error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_unpin_release(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(release_id) = cstr(release_id) else {
        return error_cstring("invalid release id");
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.library_manager().unpin_release(&release_id))
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Bring an unmanaged release under management (copy its files into the library
/// and upload). `pin` keeps a local copy; `delete_source` removes the original
/// files after copying. Returns null on success, or an error-message C string
/// (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_manage_release(
    handle: *const BaeHandle,
    release_id: *const c_char,
    pin: bool,
    delete_source: bool,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(release_id) = cstr(release_id) else {
        return error_cstring("invalid release id");
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(
            app.services
                .library_manager()
                .manage_release(&release_id, pin, delete_source),
        ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Move a managed release's files out of the library to `new_path` (unmanage).
/// Returns null on success, or an error-message C string (free with
/// [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` and `new_path` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_unmanage_release(
    handle: *const BaeHandle,
    release_id: *const c_char,
    new_path: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (Some(release_id), Some(new_path)) = (cstr(release_id), cstr(new_path)) else {
        return error_cstring("invalid unmanage argument");
    };
    let app = &handle.0;
    match app.runtime.block_on(
        app.services
            .library_manager()
            .unmanage_release(&release_id, &new_path),
    ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// One queued cloud delete.
#[derive(Serialize)]
struct FfiDeleteOp {
    id: i64,
    cloud_key: String,
}

/// Per-state counts plus byte progress. Used both per-release and overall.
#[derive(Serialize)]
struct FfiUploadProgress {
    queued: u32,
    active: u32,
    failed: u32,
    bytes_done: u64,
    bytes_total: u64,
}

/// A release's pending uploads, grouped for the queue pane's per-release rows.
/// `release_id` is null for the orphaned-files bucket; `display_title` is the
/// row's label, resolved by core.
#[derive(Serialize)]
struct FfiUploadReleaseGroup {
    release_id: Option<String>,
    display_title: String,
    file_count: u32,
    progress: FfiUploadProgress,
}

/// The cloud outbox snapshot: per-item lists, per-release counts, overall
/// totals, raw throughput, and raw ETA. The C# composes the localized summary
/// band and formats throughput/ETA/bytes for the locale.
#[derive(Serialize)]
struct FfiOutboxSnapshot {
    upload_groups: Vec<FfiUploadReleaseGroup>,
    deletes: Vec<FfiDeleteOp>,
    per_release: std::collections::HashMap<String, FfiUploadProgress>,
    total: FfiUploadProgress,
    pending_deletes: u32,
    paused: bool,
    throughput_bps: u64,
    eta_seconds: Option<u64>,
}

fn upload_progress_to_ffi(p: &bae_core::library::UploadProgress) -> FfiUploadProgress {
    FfiUploadProgress {
        queued: p.queued,
        active: p.active,
        failed: p.failed,
        bytes_done: p.bytes_done,
        bytes_total: p.bytes_total,
    }
}

fn outbox_snapshot_to_ffi(snapshot: &bae_core::library::OutboxSnapshot) -> FfiOutboxSnapshot {
    let upload_groups = snapshot
        .upload_groups
        .iter()
        .map(|g| FfiUploadReleaseGroup {
            release_id: g.release_id.clone(),
            display_title: g.display_title.clone(),
            file_count: g.file_count,
            progress: upload_progress_to_ffi(&g.progress),
        })
        .collect();
    let deletes = snapshot
        .deletes
        .iter()
        .map(|op| FfiDeleteOp {
            id: op.id,
            cloud_key: op.cloud_key.clone(),
        })
        .collect();
    let per_release = snapshot
        .per_release
        .iter()
        .map(|(k, v)| (k.clone(), upload_progress_to_ffi(v)))
        .collect();
    FfiOutboxSnapshot {
        upload_groups,
        deletes,
        per_release,
        total: upload_progress_to_ffi(&snapshot.total),
        pending_deletes: snapshot.pending_deletes,
        paused: snapshot.paused,
        throughput_bps: snapshot.throughput_bps,
        eta_seconds: snapshot.eta_seconds,
    }
}

/// The cloud outbox snapshot as JSON, or null on error. Free with
/// [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_outbox_snapshot(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_outbox_snapshot: null handle");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.library_manager().outbox_snapshot())
    {
        Ok(snapshot) => json_cstring(&outbox_snapshot_to_ffi(&snapshot)),
        Err(e) => {
            tracing::error!("bae_outbox_snapshot failed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// One queued download (a whole release being pinned), as the Downloads pane
/// renders it: title, file count and size, and the state — "queued", "active"
/// (with `percent`), or "failed" (with `error`).
#[derive(Serialize)]
struct FfiDownloadOp {
    release_id: String,
    title: String,
    file_count: i64,
    total_size: i64,
    /// "queued", "active", or "failed".
    state: String,
    /// Overall release percent — present only while `state` is "active".
    percent: Option<u8>,
    /// The failure message when `state` is "failed".
    error: Option<String>,
}

/// Per-state counts for the download queue, for the pane header's summary and
/// the retry gate.
#[derive(Serialize)]
struct FfiDownloadProgress {
    queued: u32,
    active: u32,
    failed: u32,
}

/// The in-memory download (pin) queue snapshot the Downloads pane renders, and
/// which the storage row reads to detect a pinning release.
#[derive(Serialize)]
struct FfiDownloadSnapshot {
    downloads: Vec<FfiDownloadOp>,
    total: FfiDownloadProgress,
    /// True when the user paused the download queue.
    paused: bool,
}

/// The download (pin) queue snapshot as JSON, or null on error. Free with
/// [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_download_snapshot(handle: *const BaeHandle) -> *mut c_char {
    use bae_core::library::DownloadState;
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_download_snapshot: null handle");
        return std::ptr::null_mut();
    };
    let snapshot = handle.0.services.library_manager().download_snapshot();
    json_cstring(&FfiDownloadSnapshot {
        downloads: snapshot
            .downloads
            .iter()
            .map(|op| {
                let (state, percent, error) = match &op.state {
                    DownloadState::Queued => ("queued", None, None),
                    DownloadState::Active { percent } => ("active", Some(*percent), None),
                    DownloadState::Failed { error } => ("failed", None, Some(error.clone())),
                };
                FfiDownloadOp {
                    release_id: op.release_id.clone(),
                    title: op.title.clone(),
                    file_count: op.file_count,
                    total_size: op.total_size,
                    state: state.to_string(),
                    percent,
                    error,
                }
            })
            .collect(),
        total: FfiDownloadProgress {
            queued: snapshot.total.queued,
            active: snapshot.total.active,
            failed: snapshot.total.failed,
        },
        paused: snapshot.paused,
    })
}

/// Pause or resume the download (pin) queue. While paused, the worker waits.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_set_downloads_paused(handle: *const BaeHandle, paused: bool) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_set_downloads_paused: null handle");
        return;
    };
    handle
        .0
        .services
        .library_manager()
        .set_downloads_paused(paused);
}

/// Retry failed downloads now (clears their failure and re-queues them).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_retry_downloads(handle: *const BaeHandle) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_retry_downloads: null handle");
        return;
    };
    handle.0.services.library_manager().retry_downloads();
}

/// Retry the cloud outbox now (clears backoff and triggers a sync). Returns null
/// on success, or an error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_retry_outbox(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.library_manager().retry_outbox_now())
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// Pause or resume the cloud sync pipeline. While paused, changes still queue in
/// the outbox but the sync cycle won't drain them; the outbox snapshot's `paused`
/// field flips so the UI can render the toggle.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_set_sync_paused(handle: *const BaeHandle, paused: bool) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_set_sync_paused: null handle");
        return;
    };
    let app = &handle.0;
    app.runtime
        .block_on(app.services.library_manager().set_sync_paused(paused));
}

/// Cancel one queued outbox entry by id (dequeues it; the local file stays).
/// Returns null on success, or an error-message C string (free with
/// [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_cancel_outbox_item(handle: *const BaeHandle, id: i64) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.library_manager().cancel_outbox_item(id))
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// Cancel whatever transition a release is mid-flight — a pin (download), a
/// managed upload, or an unmanage — leaving it in its prior state. A no-op if
/// nothing is in progress. Returns null on success, else an owned error string
/// to free with `bae_free_string`.
///
/// # Safety
/// `handle` must be a live `BaeHandle`; `release_id` a valid C string.
#[no_mangle]
pub unsafe extern "C" fn bae_cancel_release_transition(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(release_id) = cstr(release_id) else {
        return error_cstring("invalid release id");
    };
    let app = &handle.0;
    match app.runtime.block_on(
        app.services
            .library_manager()
            .cancel_release_transition(&release_id),
    ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// One track in an album-detail track list.
#[derive(Serialize)]
struct FfiTrack {
    /// The track's id, used to play from this track or queue it individually.
    track_id: String,
    title: String,
    /// Structured position; the C# composes "A1"/"2-3"/"5" from the case and
    /// formats nothing locale-specific.
    position: loc::FfiTrackPosition,
    /// Raw track length in milliseconds, or null when unknown. The C# formats
    /// it for the locale (e.g. "3:07").
    duration_ms: Option<i64>,
    artist: String,
}

/// One file in a release, mirroring `BridgeFile`. `audio_format` is `None` for
/// non-audio files; for audio files the C# composes the one-line descriptor
/// ("FLAC · 44.1 kHz · 16-bit · stereo") from its structured parts.
#[derive(Serialize)]
struct FfiFile {
    id: String,
    original_filename: String,
    file_size: i64,
    content_type: String,
    is_image: bool,
    audio_format: Option<loc::FfiAudioFormat>,
}

/// One release in an album's detail. The picker lists these by `display_name`;
/// selecting one shows its `tracks`.
#[derive(Serialize)]
struct FfiRelease {
    release_id: String,
    /// Human-readable picker label, e.g. "2009 · CD" or "Release 2".
    display_name: String,
    tracks: Vec<FfiTrack>,
    /// The release's files (audio + images), each with its structured audio
    /// format where applicable. Mirrors `BridgeRelease.files`.
    files: Vec<FfiFile>,
}

/// An album's detail: header fields plus every release with its tracks. The UI
/// shows `primary_release_id` first and lets the user switch releases.
#[derive(Serialize)]
struct FfiAlbumDetail {
    id: String,
    title: String,
    artist: String,
    /// The release to show first — the user's primary, or the first.
    primary_release_id: String,
    cover_path: Option<String>,
    releases: Vec<FfiRelease>,
}

/// Full detail for one album as JSON, or null on error / not found. Free the
/// result with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `album_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_album_detail(
    handle: *const BaeHandle,
    album_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_album_detail: null handle");
        return std::ptr::null_mut();
    };
    let Some(album_id) = cstr(album_id) else {
        tracing::error!("bae_album_detail: null or non-UTF-8 album_id");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let manager = app.services.library_manager();
    let detail = match app.runtime.block_on(manager.find_album_detail(&album_id)) {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            tracing::warn!("bae_album_detail: album {album_id} not found");
            return std::ptr::null_mut();
        }
        Err(e) => {
            tracing::error!("bae_album_detail failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let releases: Vec<FfiRelease> = detail
        .releases
        .into_iter()
        .map(|release| FfiRelease {
            release_id: release.summary.id,
            display_name: release.display_name,
            tracks: release
                .tracks
                .iter()
                .map(|track| FfiTrack {
                    track_id: track.id.clone(),
                    title: track.title.clone(),
                    position: loc::FfiTrackPosition::from_core(&track.position),
                    duration_ms: track.duration_ms,
                    artist: track.artist_names.clone(),
                })
                .collect(),
            files: release
                .files
                .iter()
                .map(|file| FfiFile {
                    id: file.id.clone(),
                    original_filename: file.original_filename.clone(),
                    file_size: file.file_size,
                    content_type: file.content_type.clone(),
                    is_image: file.is_image,
                    audio_format: file
                        .audio_format
                        .as_ref()
                        .map(loc::FfiAudioFormat::from_core),
                })
                .collect(),
        })
        .collect();
    let out = FfiAlbumDetail {
        id: detail.album.id,
        title: detail.album.title,
        artist: detail.artist_names,
        primary_release_id: detail.primary_release_id,
        cover_path: detail.cover_path,
        releases,
    };
    json_cstring(&out)
}

/// One track in the play queue (carried by `QueueUpdated`).
#[derive(Serialize)]
struct FfiQueueItem {
    track_id: String,
    title: String,
    artist: String,
    /// Raw track length in milliseconds, or null when unknown. The C# formats it.
    duration_ms: Option<i64>,
    album_title: String,
    cover_image_id: Option<String>,
}

/// One signals-toolbar badge (carried by `CandidateIdentifyState`). A flat,
/// pre-shaped mirror of `bae_core::identify::ToolbarSignal`: the UI renders it
/// directly. `kind`/`role`/`origin` are the snake_case wire names; `state` is a
/// tagged shape (`kind` plus an optional `count`/`message`).
#[derive(Serialize)]
struct FfiSignal {
    /// `disc_id` / `barcode` / `catalog`.
    kind: &'static str,
    /// `identity` (finds releases) / `filter` (narrows the match).
    role: &'static str,
    /// The badge value (disc-ID hash, barcode digits, catalog number), or `None`
    /// when an identity signal had no value to show.
    value: Option<String>,
    /// Where the value was harvested from, e.g. `disc_toc` / `artwork`.
    origin: &'static str,
    state: FfiSignalState,
    /// Whether the user excluded this signal from triangulation.
    excluded: bool,
}

/// A badge's live lookup/match state, flattened to a tag plus the one payload
/// each variant carries. Mirrors `bae_core::identify::SignalState`: `count` is
/// set for `found`/`confirms`, `failure` for `failed`, both `None` otherwise.
/// `failure` is the structured lookup failure (no prose) — the C# resolves a
/// localized line per variant and renders `provider`'s status as the argument.
#[derive(Serialize)]
struct FfiSignalState {
    /// `looking_up` / `found` / `no_match` / `skipped` / `failed` / `confirms`.
    kind: &'static str,
    count: Option<u32>,
    failure: Option<loc::FfiLookupFailure>,
}

/// A UI event pushed to the native app, as JSON `{type, ...}`. Only the events
/// the WinUI app reacts to are mapped; the rest are dropped.
#[derive(Serialize)]
#[serde(tag = "type")]
enum FfiEvent {
    PlaybackPlaying {
        track_id: String,
        album_id: String,
        track_title: String,
        artist: String,
        album_title: String,
        cover_image_id: Option<String>,
        /// Raw track length in milliseconds; the C# formats it.
        duration_ms: u64,
    },
    PlaybackPaused {
        track_id: String,
        album_id: String,
        track_title: String,
        artist: String,
        album_title: String,
        cover_image_id: Option<String>,
        /// Raw track length in milliseconds; the C# formats it.
        duration_ms: u64,
    },
    PlaybackStopped,
    PlaybackProgress {
        progress: f64,
        /// Raw elapsed position in milliseconds; the C# formats it.
        position_ms: u64,
        /// Raw track length in milliseconds; the C# formats the remaining time
        /// (`duration_ms - position_ms`) for the locale.
        duration_ms: u64,
    },
    /// The library changed (album/release add/update/remove) — reload views.
    LibraryChanged,
    QueueUpdated {
        items: Vec<FfiQueueItem>,
        has_next: bool,
        has_previous: bool,
    },
    /// Tracks were appended/inserted into the queue — flash a `+N` badge.
    QueueItemsAdded {
        count: u32,
    },
    /// Output volume changed (0.0–1.0).
    VolumeChanged {
        volume: f32,
    },
    /// Mute toggled.
    MuteChanged {
        is_muted: bool,
    },
    /// Repeat mode changed: `none` / `track` / `album`.
    RepeatModeChanged {
        mode: String,
    },
    /// A sync-loop error, or `null` message when a prior error cleared.
    /// Sync-loop error state, or `null` when a prior error cleared. When set,
    /// the structured diagnostic the C# renders as a generic per-category line
    /// plus the opaque, log-only `detail`.
    SyncError {
        error: Option<loc::FfiError>,
    },
    /// The sync pipeline started or stopped a pass — drives the toolbar indicator.
    SyncingChanged {
        syncing: bool,
    },
    /// The last successful sync time changed (`null` when never synced). Unix
    /// epoch milliseconds the toolbar formats into a local time.
    SyncTimeChanged {
        sync_time: Option<i64>,
    },
    /// Playback failed for the current track. `reason` is the structured,
    /// locale-free reason: the actionable cloud-only cases the C# keys, and a
    /// `diagnostic` that renders through the error-category path.
    PlaybackError {
        reason: loc::FfiPlaybackErrorReason,
    },
    /// A general operation error (scan or library) with no event of its
    /// own — surfaced in the error banner. `error` is the structured diagnostic.
    Error {
        error: loc::FfiError,
    },
    /// A prior general error cleared — close the banner.
    ErrorCleared,
    /// The next track is loading (keep the prior now-playing visible).
    PlaybackLoading,
    /// Import-preview playback position. Raw elapsed milliseconds; the C# formats it.
    PreviewProgress {
        position_ms: u64,
    },
    /// Import-preview playback started or resumed. Raw track length in
    /// milliseconds, shown (formatted) next to the elapsed position.
    PreviewPlaying {
        duration_ms: u64,
    },
    /// Import-preview playback stopped/ended — clear its position display.
    PreviewIdle,
    /// A release candidate was found by a folder scan. `audio_paths` are its
    /// playable files, for pre-import preview.
    CandidateAdded {
        key: String,
        name: String,
        track_count: Option<u32>,
        format: String,
        audio_paths: Vec<String>,
    },
    /// A candidate left the scan list.
    CandidateRemoved {
        key: String,
    },
    /// The folder walk finished.
    ScanFinished,
    /// The cloud upload/delete outbox changed; the storage screen re-reads it.
    OutboxChanged,
    /// The download (pin) queue changed; the Downloads pane re-reads it.
    DownloadQueueChanged,
    /// Library config changed (cloud provider connected/disconnected, sync
    /// readiness, library rename, Discogs token). The settings screen re-reads
    /// `bae_settings` so its fields reflect the change without a reopen.
    ConfigChanged,
    /// Auto-identification progress/result for a candidate. `status` is one of
    /// `idle` / `identifying` / `found` / `conflict` / `not_found` / `manual` /
    /// `error`; `matches` is populated when `found`; `message` carries the reason
    /// when `error`.
    CandidateIdentifyState {
        key: String,
        status: String,
        matches: Vec<FfiCandidate>,
        message: Option<String>,
        /// The pre-shaped per-signal badge list projected from the same
        /// transition; the app replaces the candidate's badge row wholesale.
        signals: Vec<FfiSignal>,
    },
    /// Import progress for a candidate. `step` is the structured, locale-free
    /// step (or `null` before the first step is known); the C# resolves its
    /// localized verb from the step's catalog key.
    CandidateImportProgress {
        key: String,
        progress_percent: u32,
        step: Option<loc::FfiImportStep>,
    },
    /// A candidate's import finished; the album is in the library.
    CandidateImportComplete {
        key: String,
        /// The release the import created — the UI's join key for
        /// candidate-level invalidation (release deleted) and the
        /// per-release upload queue.
        release_id: String,
        album_id: String,
    },
    /// A candidate's import failed. `error` is the structured diagnostic the C#
    /// renders as a generic per-category line plus the opaque, log-only detail.
    CandidateImportError {
        key: String,
        error: loc::FfiError,
    },
}

/// Reduce an `IdentifyState` to the wire status the candidate row shows, the
/// match list (populated only when found), and any error message.
fn identify_state_to_ffi(
    state: &bae_core::identify::IdentifyState,
) -> (&'static str, Vec<FfiCandidate>, Option<String>) {
    use bae_core::identify::IdentifyState;
    match state {
        IdentifyState::Idle => ("idle", vec![], None),
        IdentifyState::Triangulating { .. } => ("identifying", vec![], None),
        IdentifyState::Found { matches, .. } => {
            ("found", matches.iter().map(metadata_to_ffi).collect(), None)
        }
        IdentifyState::Conflict { .. } => ("conflict", vec![], None),
        IdentifyState::NotFoundAnywhere { .. } => ("not_found", vec![], None),
        IdentifyState::ManualOnly { .. } => ("manual", vec![], None),
    }
}

/// Project one `ToolbarSignal` onto its flat wire mirror. Pure translation —
/// all per-signal state is already shaped by `bae_core::identify::toolbar`.
fn toolbar_signal_to_ffi(signal: &bae_core::identify::ToolbarSignal) -> FfiSignal {
    use bae_core::identify::{SignalKind, SignalRole, SignalState};
    use bae_core::signals::SignalOrigin;

    let kind = match signal.kind {
        SignalKind::DiscId => "disc_id",
        SignalKind::Barcode => "barcode",
        SignalKind::Catalog => "catalog",
    };
    let role = match signal.role {
        SignalRole::Identity => "identity",
        SignalRole::Filter => "filter",
    };
    let origin = match signal.origin {
        SignalOrigin::DiscToc => "disc_toc",
        SignalOrigin::CueSheet => "cue_sheet",
        SignalOrigin::Artwork => "artwork",
        SignalOrigin::FolderName => "folder_name",
        SignalOrigin::Filename => "filename",
        SignalOrigin::TextFile => "text_file",
    };
    let state = match &signal.state {
        SignalState::LookingUp => FfiSignalState {
            kind: "looking_up",
            count: None,
            failure: None,
        },
        SignalState::Found { count } => FfiSignalState {
            kind: "found",
            count: Some(*count),
            failure: None,
        },
        SignalState::NoMatch => FfiSignalState {
            kind: "no_match",
            count: None,
            failure: None,
        },
        SignalState::Skipped => FfiSignalState {
            kind: "skipped",
            count: None,
            failure: None,
        },
        SignalState::Failed { failure } => FfiSignalState {
            kind: "failed",
            count: None,
            failure: Some(loc::FfiLookupFailure::from_core(failure)),
        },
        SignalState::Confirms { count } => FfiSignalState {
            kind: "confirms",
            count: Some(*count),
            failure: None,
        },
    };

    FfiSignal {
        kind,
        role,
        value: signal.value.clone(),
        origin,
        state,
        excluded: signal.excluded,
    }
}

/// The playable audio file paths for a scanned candidate.
fn candidate_audio_paths(
    files: &bae_core::import::folder_scanner::CategorizedFiles,
) -> Vec<String> {
    use bae_core::import::folder_scanner::AudioContent;
    match &files.audio {
        AudioContent::CueFlacPairs { pairs, .. } => pairs
            .iter()
            .map(|pair| pair.audio_file.path.to_string_lossy().to_string())
            .collect(),
        AudioContent::TrackFiles { tracks, .. } => tracks
            .iter()
            .map(|track| track.path.to_string_lossy().to_string())
            .collect(),
    }
}

/// The wire name for a repeat mode.
fn repeat_mode_name(mode: &bae_core::playback::RepeatMode) -> &'static str {
    use bae_core::playback::RepeatMode;
    match mode {
        RepeatMode::None => "none",
        RepeatMode::Track => "track",
        RepeatMode::Album => "album",
    }
}

/// Map a core `UiBusEvent` to the subset the WinUI app handles, or `None`.
fn map_event(event: &UiBusEvent) -> Option<FfiEvent> {
    Some(match event {
        UiBusEvent::PlaybackPlaying {
            track_id,
            album_id,
            track_title,
            artist_names,
            album_title,
            cover_image_id,
            duration_ms,
            ..
        } => FfiEvent::PlaybackPlaying {
            track_id: track_id.clone(),
            album_id: album_id.clone(),
            track_title: track_title.clone(),
            artist: artist_names.clone(),
            album_title: album_title.clone(),
            cover_image_id: cover_image_id.clone(),
            duration_ms: *duration_ms,
        },
        UiBusEvent::PlaybackPaused {
            track_id,
            album_id,
            track_title,
            artist_names,
            album_title,
            cover_image_id,
            duration_ms,
            ..
        } => FfiEvent::PlaybackPaused {
            track_id: track_id.clone(),
            album_id: album_id.clone(),
            track_title: track_title.clone(),
            artist: artist_names.clone(),
            album_title: album_title.clone(),
            cover_image_id: cover_image_id.clone(),
            duration_ms: *duration_ms,
        },
        UiBusEvent::PlaybackStopped => FfiEvent::PlaybackStopped,
        UiBusEvent::PlaybackLoading { .. } => FfiEvent::PlaybackLoading,
        UiBusEvent::PreviewProgress { position_ms, .. } => FfiEvent::PreviewProgress {
            position_ms: *position_ms,
        },
        UiBusEvent::PreviewPlaying { duration_ms, .. } => FfiEvent::PreviewPlaying {
            duration_ms: *duration_ms,
        },
        UiBusEvent::PreviewIdle => FfiEvent::PreviewIdle,
        UiBusEvent::PlaybackError { reason } => FfiEvent::PlaybackError {
            reason: loc::FfiPlaybackErrorReason::from_core(reason),
        },
        UiBusEvent::Error { error } => FfiEvent::Error {
            error: loc::FfiError::from_core(error),
        },
        UiBusEvent::ErrorCleared => FfiEvent::ErrorCleared,
        UiBusEvent::SyncError { error } => FfiEvent::SyncError {
            error: error.as_ref().map(loc::FfiError::from_core),
        },
        UiBusEvent::SyncingChanged { syncing } => FfiEvent::SyncingChanged { syncing: *syncing },
        UiBusEvent::SyncTimeChanged { time } => FfiEvent::SyncTimeChanged { sync_time: *time },
        UiBusEvent::PlaybackProgress {
            progress,
            position_ms,
            duration_ms,
            ..
        } => FfiEvent::PlaybackProgress {
            progress: *progress,
            position_ms: *position_ms,
            duration_ms: *duration_ms,
        },
        UiBusEvent::AlbumAdded { .. }
        | UiBusEvent::AlbumUpdated { .. }
        | UiBusEvent::AlbumRemoved { .. }
        | UiBusEvent::ReleaseAdded { .. }
        | UiBusEvent::ReleaseUpdated { .. }
        | UiBusEvent::ReleaseRemoved { .. } => FfiEvent::LibraryChanged,
        UiBusEvent::QueueUpdated {
            items,
            has_next,
            has_previous,
        } => FfiEvent::QueueUpdated {
            items: items
                .iter()
                .map(|item| FfiQueueItem {
                    track_id: item.track_id.clone(),
                    title: item.title.clone(),
                    artist: item.artist_names.clone(),
                    duration_ms: item.duration_ms,
                    album_title: item.album_title.clone(),
                    cover_image_id: item.cover_image_id.clone(),
                })
                .collect(),
            has_next: *has_next,
            has_previous: *has_previous,
        },
        UiBusEvent::VolumeChanged { volume } => FfiEvent::VolumeChanged { volume: *volume },
        UiBusEvent::MuteChanged { is_muted } => FfiEvent::MuteChanged {
            is_muted: *is_muted,
        },
        UiBusEvent::RepeatModeChanged { mode } => FfiEvent::RepeatModeChanged {
            mode: repeat_mode_name(mode).to_string(),
        },
        UiBusEvent::FolderCandidateAdded { candidate } => FfiEvent::CandidateAdded {
            key: candidate.path.to_string_lossy().to_string(),
            name: candidate.name.clone(),
            track_count: candidate.files.audio.track_count(),
            format: candidate.files.audio.format_label().to_string(),
            audio_paths: candidate_audio_paths(&candidate.files),
        },
        UiBusEvent::ScanCandidateRemoved { key } => FfiEvent::CandidateRemoved { key: key.clone() },
        UiBusEvent::ScanFinished => FfiEvent::ScanFinished,
        UiBusEvent::OutboxChanged { .. } => FfiEvent::OutboxChanged,
        UiBusEvent::DownloadQueueChanged { .. } => FfiEvent::DownloadQueueChanged,
        UiBusEvent::ConfigChanged { .. } => FfiEvent::ConfigChanged,
        UiBusEvent::CandidateIdentifyStateChanged {
            key,
            state,
            toolbar,
        } => {
            let (status, matches, message) = identify_state_to_ffi(state);
            FfiEvent::CandidateIdentifyState {
                key: key.clone(),
                status: status.to_string(),
                matches,
                message,
                signals: toolbar.iter().map(toolbar_signal_to_ffi).collect(),
            }
        }
        UiBusEvent::CandidateImportImporting {
            key,
            progress_percent,
            step,
        } => FfiEvent::CandidateImportProgress {
            key: key.clone(),
            progress_percent: *progress_percent,
            step: step.as_ref().map(loc::FfiImportStep::from_core),
        },
        UiBusEvent::CandidateImportComplete {
            key,
            release_id,
            album_id,
        } => FfiEvent::CandidateImportComplete {
            key: key.clone(),
            release_id: release_id.clone(),
            album_id: album_id.clone(),
        },
        UiBusEvent::CandidateImportError { key, error } => FfiEvent::CandidateImportError {
            key: key.clone(),
            error: loc::FfiError::from_core(error),
        },
        UiBusEvent::QueueItemsAdded { count } => FfiEvent::QueueItemsAdded { count: *count },
        _ => return None,
    })
}

/// Callback the app passes to [`bae_subscribe`]; invoked with each event's JSON
/// (a NUL-terminated C string valid only for the duration of the call). It fires
/// on a background thread — the C# side marshals to its UI thread.
pub type EventCallback = unsafe extern "C" fn(*const c_char);

/// Subscribe to the core UI event bus. Spawns a task that forwards mapped events
/// to `callback` as JSON for the app's lifetime.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `callback` must remain a valid function pointer for the app's lifetime.
#[no_mangle]
pub unsafe extern "C" fn bae_subscribe(handle: *const BaeHandle, callback: EventCallback) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_subscribe: null handle");
        return;
    };
    let app = &handle.0;
    let mut rx = app.ui_event_bus.subscribe();
    app.runtime.spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let Some(ffi) = map_event(&event) else {
                        continue;
                    };
                    let json = match serde_json::to_string(&ffi) {
                        Ok(json) => json,
                        Err(e) => {
                            tracing::debug!("event serialize failed: {e}");
                            continue;
                        }
                    };
                    let Ok(cstring) = CString::new(json) else {
                        continue;
                    };
                    // SAFETY: the C# callback copies the string before returning;
                    // `cstring` stays alive across the call.
                    unsafe { callback(cstring.as_ptr()) };
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("bae_subscribe: UI event bus lagged, dropped {n} events");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Cache-bustable identifier for a library image id, or null if it isn't
/// cached: the on-disk path with the file's modification time appended as
/// `#v=<mtime_secs>`. The version changes when the cover does, so the WinUI
/// bitmap cache key changes; `CoverImage.Load` strips the `#v=…` suffix before
/// opening the file. Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `image_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_image_path(
    handle: *const BaeHandle,
    image_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_image_path: null handle");
        return std::ptr::null_mut();
    };
    let Some(image_id) = cstr(image_id) else {
        tracing::error!("bae_image_path: null or non-UTF-8 image_id");
        return std::ptr::null_mut();
    };
    let Some(identifier) = handle
        .0
        .services
        .library_manager()
        .image_path_if_exists(&image_id)
    else {
        return std::ptr::null_mut();
    };
    match CString::new(identifier) {
        Ok(cstring) => cstring.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Start playing a release (optionally shuffled). `start_track_index` selects
/// the track to start from; a negative value starts from the first track (the
/// C-ABI form of the core's `Option<usize>` start index).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_play_release(
    handle: *const BaeHandle,
    release_id: *const c_char,
    start_track_index: i64,
    shuffle: bool,
) {
    let Some(handle) = handle.as_ref() else {
        return;
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_play_release: null or non-UTF-8 release_id");
        return;
    };
    let start = usize::try_from(start_track_index).ok();
    handle
        .0
        .services
        .playback()
        .play_release(release_id, start, shuffle);
}

/// Toggle play/pause.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_play_pause(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().toggle_play_pause();
    }
}

/// Seek the current track to `ratio` (0.0–1.0) of its duration.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_seek_by_ratio(handle: *const BaeHandle, ratio: f64) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().seek_by_ratio(ratio);
    }
}

/// Set the output volume (0.0–1.0).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_set_volume(handle: *const BaeHandle, volume: f32) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().set_volume(volume);
    }
}

/// Toggle mute.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_toggle_mute(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().toggle_mute();
    }
}

/// The current output volume (0.0–1.0), or 1.0 if the handle is null.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_get_volume(handle: *const BaeHandle) -> f32 {
    let Some(handle) = handle.as_ref() else {
        return 1.0;
    };
    let app = &handle.0;
    app.runtime.block_on(app.services.playback().get_volume())
}

/// Cycle the repeat mode (off → repeat-track → repeat-album → off). The new mode
/// arrives as a `RepeatModeChanged` event.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_cycle_repeat_mode(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().cycle_repeat_mode();
    }
}

/// Preview-play an audio file by path (auditioning a candidate before import).
/// Independent of library playback; progress arrives as `PreviewProgress` events.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `path` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_preview_play(handle: *const BaeHandle, path: *const c_char) {
    let Some(handle) = handle.as_ref() else {
        return;
    };
    let Some(path) = cstr(path) else {
        tracing::error!("bae_preview_play: null or non-UTF-8 path");
        return;
    };
    handle.0.services.playback().preview_play(path);
}

/// Stop preview playback.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_preview_stop(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().preview_stop();
    }
}

/// Toggle preview play/pause.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_preview_toggle_pause(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().preview_toggle_pause();
    }
}

/// Skip to the next track.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_next(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().next();
    }
}

/// Skip to the previous track.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_previous(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().previous();
    }
}

/// Jump to the queue entry at `index`.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_queue_skip_to(handle: *const BaeHandle, index: u32) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().skip_to(index as usize);
    }
}

/// Remove the queue entry at `index`.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_queue_remove(handle: *const BaeHandle, index: u32) {
    if let Some(handle) = handle.as_ref() {
        handle
            .0
            .services
            .playback()
            .remove_from_queue(index as usize);
    }
}

/// Move the queue entry at `from` to `to`.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_queue_reorder(handle: *const BaeHandle, from: u32, to: u32) {
    if let Some(handle) = handle.as_ref() {
        handle
            .0
            .services
            .playback()
            .reorder_queue(from as usize, to as usize);
    }
}

/// Clear the play queue.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_queue_clear(handle: *const BaeHandle) {
    if let Some(handle) = handle.as_ref() {
        handle.0.services.playback().clear_queue();
    }
}

/// Append a release's tracks to the end of the play queue.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_add_release_to_queue(
    handle: *const BaeHandle,
    release_id: *const c_char,
) {
    let Some(handle) = handle.as_ref() else {
        return;
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_add_release_to_queue: null or non-UTF-8 release_id");
        return;
    };
    handle
        .0
        .services
        .playback()
        .add_release_to_queue(release_id);
}

/// Queue a release's tracks to play next (after the current track).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_add_release_next(handle: *const BaeHandle, release_id: *const c_char) {
    let Some(handle) = handle.as_ref() else {
        return;
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_add_release_next: null or non-UTF-8 release_id");
        return;
    };
    handle.0.services.playback().add_release_next(release_id);
}

/// Parse a JSON array of track ids passed across the C ABI.
fn track_ids_from_json(track_ids_json: *const c_char) -> Result<Vec<String>, *mut c_char> {
    let Some(json) = (unsafe { cstr(track_ids_json) }) else {
        return Err(error_cstring("invalid track ids payload"));
    };
    serde_json::from_str(&json).map_err(|e| error_cstring(&format!("malformed track ids: {e}")))
}

/// Append specific tracks to the end of the play queue. `track_ids_json` is a
/// JSON array of track ids. Returns null on success, or an error-message C
/// string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `track_ids_json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_add_to_queue(
    handle: *const BaeHandle,
    track_ids_json: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let track_ids = match track_ids_from_json(track_ids_json) {
        Ok(ids) => ids,
        Err(err) => return err,
    };
    handle.0.services.playback().add_to_queue(track_ids);
    std::ptr::null_mut()
}

/// Queue specific tracks to play next (after the current track).
/// `track_ids_json` is a JSON array of track ids. Returns null on success, or
/// an error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `track_ids_json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_add_next(
    handle: *const BaeHandle,
    track_ids_json: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let track_ids = match track_ids_from_json(track_ids_json) {
        Ok(ids) => ids,
        Err(err) => return err,
    };
    handle.0.services.playback().add_next(track_ids);
    std::ptr::null_mut()
}

/// Delete a release from the library. Returns null on success, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_delete_release(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(release_id) = cstr(release_id) else {
        return error_cstring("invalid release id");
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.library_manager().delete_release(&release_id))
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// Allocate an owned error-message C string (for command results: null = success,
/// non-null = the error). Null if allocation fails.
fn error_cstring(msg: &str) -> *mut c_char {
    CString::new(msg)
        .map(CString::into_raw)
        .unwrap_or_else(|e| {
            tracing::error!("FFI string had an interior NUL: {e}");
            std::ptr::null_mut()
        })
}

/// The wire name for a connected cloud provider.
fn cloud_provider_name(provider: &bae_core::config::CloudProvider) -> &'static str {
    use bae_core::config::CloudProvider;
    match provider {
        CloudProvider::S3 => "s3",
        CloudProvider::GoogleDrive => "google_drive",
        CloudProvider::Dropbox => "dropbox",
        CloudProvider::OneDrive => "onedrive",
        CloudProvider::CloudKit => "cloudkit",
    }
}

/// Parse a wire tag into an OAuth cloud provider. Only the browser-OAuth
/// providers are accepted here — S3 connects via its own entry point, and
/// CloudKit is Apple-only.
#[cfg(feature = "oauth-providers")]
fn oauth_provider_from_str(provider: &str) -> Option<bae_core::config::CloudProvider> {
    use bae_core::config::CloudProvider;
    match provider {
        "google_drive" => Some(CloudProvider::GoogleDrive),
        "dropbox" => Some(CloudProvider::Dropbox),
        "onedrive" => Some(CloudProvider::OneDrive),
        _ => None,
    }
}

/// The cloud providers this build supports, as a JSON array of wire tags
/// ("s3", "google_drive", ...) in display order. S3 is always present; the
/// OAuth providers only when compiled in. The WinUI picker renders from this
/// instead of a hardcoded list, so a baeium (S3-only) build offers only S3.
/// Free the result with [`bae_string_free`].
#[no_mangle]
pub extern "C" fn bae_available_cloud_providers() -> *mut c_char {
    let providers = [
        "s3",
        #[cfg(feature = "oauth-providers")]
        "google_drive",
        #[cfg(feature = "oauth-providers")]
        "dropbox",
        #[cfg(feature = "oauth-providers")]
        "onedrive",
    ];
    json_cstring(&providers)
}

/// App settings the WinUI settings screen displays.
#[derive(Serialize)]
struct FfiSettings {
    library_name: String,
    library_id: String,
    has_discogs_token: bool,
    /// `not_configured` / `valid` / `unvalidated` / `rejected`.
    discogs_status: String,
    /// Whether Discogs can be used as a metadata source (a stored key that
    /// isn't rejected). Core decides the policy; the C# reads this flag rather
    /// than re-deriving it from `discogs_status`.
    discogs_usable: bool,
    /// The connected cloud provider's wire name, or null when not syncing.
    sync_provider: Option<String>,
    /// A human label for the connected account, or null.
    sync_account: Option<String>,
    /// Whether the sync layer is initialized and ready to push/pull.
    sync_ready: bool,
}

/// Current settings as JSON, or null on error. Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_settings(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_settings: null handle");
        return std::ptr::null_mut();
    };
    let manager = handle.0.services.library_manager();
    let config = manager.get_config();
    let discogs_status = config.discogs_token_status();
    let out = FfiSettings {
        library_name: config.library_name.clone(),
        library_id: config.library_id.clone(),
        has_discogs_token: manager.has_discogs_token(),
        discogs_usable: discogs_status.is_usable(),
        discogs_status: match discogs_status {
            bae_core::config::DiscogsTokenStatus::NotConfigured => "not_configured",
            bae_core::config::DiscogsTokenStatus::Valid => "valid",
            bae_core::config::DiscogsTokenStatus::Unvalidated => "unvalidated",
            bae_core::config::DiscogsTokenStatus::Rejected => "rejected",
        }
        .to_string(),
        sync_provider: config
            .cloud_home
            .provider
            .as_ref()
            .map(|provider| cloud_provider_name(provider).to_string()),
        sync_account: config.cloud_account_display(),
        sync_ready: manager.is_sync_ready(),
    };
    json_cstring(&out)
}

/// Validate and save the Discogs API token. Validates against Discogs first,
/// then persists only an accepted or unreachable key. Returns the outcome as a
/// C string — `"valid"` (validated and stored), `"unvalidated"` (couldn't reach
/// Discogs/rate-limited, stored anyway and re-validated later), or `"rejected"`
/// (Discogs rejected it, nothing stored). Returns null on an internal error
/// (logged). Free the result with [`bae_string_free`].
///
/// On `"valid"`/`"unvalidated"` the config changes, so a `ConfigChanged` event
/// follows and the settings screen re-reads the authoritative status; `"rejected"`
/// persists nothing, so the caller must surface it from this return value.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `token` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_save_discogs_token(
    handle: *const BaeHandle,
    token: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_save_discogs_token: null handle");
        return std::ptr::null_mut();
    };
    let Some(token) = cstr(token) else {
        tracing::error!("bae_save_discogs_token: null or non-UTF-8 token");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let outcome = app
        .runtime
        .block_on(app.services.import().save_discogs_token(&token));
    match outcome {
        Ok(bae_core::import::DiscogsSaveOutcome::Valid) => error_cstring("valid"),
        Ok(bae_core::import::DiscogsSaveOutcome::Unvalidated) => error_cstring("unvalidated"),
        Ok(bae_core::import::DiscogsSaveOutcome::Rejected) => error_cstring("rejected"),
        Err(e) => {
            tracing::error!("bae_save_discogs_token failed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Remove the stored Discogs token. Returns null on success, or an error-message
/// C string on failure (free it with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_delete_discogs_token(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    // Goes through the import handle (not the raw keyring delete) so the config
    // flag is cleared in the same step, firing ConfigChanged for the UI.
    match handle.0.services.import().remove_discogs_token() {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Re-validate a stored-but-unvalidated Discogs token against Discogs (e.g. one
/// saved while offline). No-op unless a key is stored with `unvalidated` status.
/// On a result the config status changes, so a `ConfigChanged` event follows and
/// the settings screen re-reads. Returns null on success, or an error-message C
/// string on failure (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_revalidate_discogs_token(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.import().revalidate_discogs_token())
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Read a C string, mapping null/empty to `None` (for optional fields).
unsafe fn opt_cstr(ptr: *const c_char) -> Option<String> {
    cstr(ptr).filter(|s| !s.is_empty())
}

/// Connect cloud sync to an S3-compatible bucket. `endpoint` and `key_prefix`
/// are optional (empty = unset). `storage` is `"opaque"` (encrypted at rest,
/// obfuscated blob paths) or `"browsable"` (stored in the clear at readable
/// paths). Probes the bucket before saving. Returns null on success, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed; all
/// string arguments must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bae_save_sync_config(
    handle: *const BaeHandle,
    bucket: *const c_char,
    region: *const c_char,
    endpoint: *const c_char,
    key_prefix: *const c_char,
    access_key: *const c_char,
    secret_key: *const c_char,
    storage: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (Some(bucket), Some(region), Some(access_key), Some(secret_key)) = (
        cstr(bucket),
        cstr(region),
        cstr(access_key),
        cstr(secret_key),
    ) else {
        return error_cstring("bucket, region, access key, and secret key are required");
    };
    let Some(storage) = cstr(storage).as_deref().and_then(home_storage_from_str) else {
        return error_cstring("storage must be \"opaque\" or \"browsable\"");
    };
    let config = bae_core::sync::sync_manager::S3ConfigData {
        bucket,
        region,
        endpoint: opt_cstr(endpoint),
        key_prefix: opt_cstr(key_prefix),
        access_key,
        secret_key,
        storage,
    };
    match handle
        .0
        .runtime
        .block_on(handle.0.services.library_manager().save_s3_config(config))
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// The data-loss warning shown before disconnecting sync when releases live only
/// in the cloud, or null when there's nothing at risk (or on error). Free with
/// [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_disconnect_warning(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_disconnect_warning: null handle");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    match app
        .runtime
        .block_on(app.services.library_manager().disconnect_warning_message())
    {
        Ok(Some(message)) => error_cstring(&message),
        Ok(None) => std::ptr::null_mut(),
        Err(e) => {
            tracing::error!("bae_disconnect_warning failed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Connect cloud sync to an OAuth provider (`"google_drive"` / `"dropbox"` /
/// `"onedrive"`). `storage` is `"opaque"` (encrypted at rest, obfuscated blob
/// paths) or `"browsable"` (stored in the clear at readable paths). Runs the
/// browser authorization (the core opens the system browser and waits for the
/// redirect), then saves the tokens. Blocks until the user finishes — call off
/// the UI thread. Returns null on success, or an error-message C string (free
/// with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `provider` and `storage` must be valid NUL-terminated UTF-8 C strings.
#[cfg(feature = "oauth-providers")]
#[no_mangle]
pub unsafe extern "C" fn bae_sign_in_cloud(
    handle: *const BaeHandle,
    provider: *const c_char,
    storage: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(provider) = cstr(provider) else {
        return error_cstring("invalid provider");
    };
    let Some(provider) = oauth_provider_from_str(&provider) else {
        return error_cstring("unknown or unsupported provider");
    };
    let Some(storage) = cstr(storage).as_deref().and_then(home_storage_from_str) else {
        return error_cstring("storage must be \"opaque\" or \"browsable\"");
    };
    let app = &handle.0;
    match app.runtime.block_on(
        app.services
            .library_manager()
            .sign_in_cloud_provider(provider, storage),
    ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Disconnect cloud sync. Returns null on success, or an error-message C string
/// (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_disconnect_cloud(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    match handle
        .0
        .services
        .library_manager()
        .disconnect_cloud_provider()
    {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Trigger a sync pass now. No-op when sync isn't connected.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_trigger_sync(handle: *const BaeHandle) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_trigger_sync: null handle");
        return;
    };
    handle.0.services.library_manager().trigger_sync();
}

/// A restore code for this library — enter it on another device to restore the
/// library from the cloud. Returns the code as a C string, or null on error
/// (e.g. sync isn't connected; logged). Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_generate_restore_code(handle: *const BaeHandle) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_generate_restore_code: null handle");
        return std::ptr::null_mut();
    };
    match handle.0.services.library_manager().generate_restore_code() {
        Ok(code) => error_cstring(&code),
        Err(e) => {
            tracing::error!("bae_generate_restore_code failed: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Seed the metadata editor's raw form from a release's current metadata, as a
/// JSON [`bae_core::import::RawReleaseEdit`] (`{album_title, album_artist_text,
/// pressing, tracks}`). Returns null on error. Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_release_edit_seed(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_release_edit_seed: null handle");
        return std::ptr::null_mut();
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_release_edit_seed: null or non-UTF-8 release_id");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let raw = match app.runtime.block_on(
        app.services
            .library_manager()
            .release_edit_seed(&release_id),
    ) {
        Ok(raw) => raw,
        Err(e) => {
            tracing::error!("bae_release_edit_seed failed: {e}");
            return std::ptr::null_mut();
        }
    };
    json_cstring(&raw)
}

/// Re-seed the metadata editor's raw form from a release's stored metadata
/// source (its original identity), discarding the user's in-progress edits
/// without writing the DB. Returns the same JSON
/// [`bae_core::import::RawReleaseEdit`] shape as [`bae_release_edit_seed`], so
/// the editor repopulates its form from the projected source values. Returns
/// null on error. Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_reset_metadata_to_source(
    handle: *const BaeHandle,
    release_id: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_reset_metadata_to_source: null handle");
        return std::ptr::null_mut();
    };
    let Some(release_id) = cstr(release_id) else {
        tracing::error!("bae_reset_metadata_to_source: null or non-UTF-8 release_id");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let user_edit = match app.runtime.block_on(
        app.services
            .library_manager()
            .reset_metadata_to_source(&release_id),
    ) {
        Ok(edit) => edit,
        Err(e) => {
            tracing::error!("bae_reset_metadata_to_source failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let raw = bae_core::import::RawReleaseEdit::from_user_edit(user_edit, "reset-track");
    json_cstring(&raw)
}

/// Apply an edited raw form (JSON [`bae_core::import::RawReleaseEdit`]) to a
/// release: validate (shape) then write the user's values without touching
/// identity or metadata source. Returns null on success, or an error-message C
/// string on failure (free it with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` and `raw_json` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_apply_release_edit(
    handle: *const BaeHandle,
    release_id: *const c_char,
    raw_json: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(release_id) = cstr(release_id) else {
        return error_cstring("invalid release id");
    };
    let Some(raw_json) = cstr(raw_json) else {
        return error_cstring("invalid edit payload");
    };
    let raw: bae_core::import::RawReleaseEdit = match serde_json::from_str(&raw_json) {
        Ok(raw) => raw,
        Err(e) => return error_cstring(&format!("malformed edit: {e}")),
    };
    let edit = match raw.shape() {
        Ok(edit) => edit,
        Err(e) => return error_cstring(&e.to_string()),
    };
    let app = &handle.0;
    match app.runtime.block_on(
        app.services
            .library_manager()
            .apply_release_metadata_user_edit(&release_id, &edit),
    ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// One candidate identity from a metadata search, as the re-identify picker
/// renders it. The picker passes `release_id` + `source` back to
/// [`bae_reidentify_release`] to commit the choice.
#[derive(Serialize)]
struct FfiCandidate {
    source: String,
    release_id: String,
    title: String,
    artist: Option<String>,
    year: Option<i32>,
    format: Option<String>,
    label: Option<String>,
    catalog_number: Option<String>,
    country: Option<String>,
}

/// Parse the wire source tag into a [`bae_core::import::MetadataSource`].
fn metadata_source_from_str(source: &str) -> Option<bae_core::import::MetadataSource> {
    match source {
        "discogs" => Some(bae_core::import::MetadataSource::Discogs),
        "musicbrainz" => Some(bae_core::import::MetadataSource::MusicBrainz),
        _ => None,
    }
}

/// Parse the wire storage-mode tag into a [`bae_core::import::StorageMode`].
/// `managed_pinned` keeps a local managed copy; `managed_unpinned` uploads to
/// cloud without keeping one; `unmanaged` leaves the files in place.
fn storage_mode_from_str(mode: &str) -> Option<bae_core::import::StorageMode> {
    match mode {
        "managed_unpinned" => Some(bae_core::import::StorageMode::Managed { pin: false }),
        "managed_pinned" => Some(bae_core::import::StorageMode::Managed { pin: true }),
        "unmanaged" => Some(bae_core::import::StorageMode::Unmanaged),
        _ => None,
    }
}

/// Parse the wire cloud-home storage tag into a [`bae_core::config::HomeStorage`].
/// `opaque` encrypts every object at rest under the library key; `browsable`
/// stores objects in the clear at readable paths. Not access control — the
/// bucket's own credentials gate it either way.
fn home_storage_from_str(storage: &str) -> Option<bae_core::config::HomeStorage> {
    match storage {
        "opaque" => Some(bae_core::config::HomeStorage::Opaque),
        "browsable" => Some(bae_core::config::HomeStorage::Browsable),
        _ => None,
    }
}

/// Build a wire candidate from a core search/identify result.
fn metadata_to_ffi(result: &bae_core::import::search::MetadataResult) -> FfiCandidate {
    FfiCandidate {
        source: result.source.as_str().to_string(),
        release_id: result.release_id.clone(),
        title: result.title.clone(),
        artist: result.artist.clone(),
        year: result.year,
        format: result.format.clone(),
        label: result.label.clone(),
        catalog_number: result.catalog_number.clone(),
        country: result.country.clone(),
    }
}

/// Search a metadata source (`"discogs"` or `"musicbrainz"`) for releases
/// matching `artist` + `album`, as a JSON array of candidates. Returns null on
/// error. Free with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `source`, `artist`, and `album` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_search_releases(
    handle: *const BaeHandle,
    source: *const c_char,
    artist: *const c_char,
    album: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_search_releases: null handle");
        return std::ptr::null_mut();
    };
    let (Some(source), Some(artist), Some(album)) = (cstr(source), cstr(artist), cstr(album))
    else {
        tracing::error!("bae_search_releases: null or non-UTF-8 argument");
        return std::ptr::null_mut();
    };
    let Some(source) = metadata_source_from_str(&source) else {
        tracing::error!("bae_search_releases: unknown source");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let results = match source {
        bae_core::import::MetadataSource::Discogs => app.runtime.block_on(
            app.services
                .import()
                .search_discogs(artist, album, None, None),
        ),
        bae_core::import::MetadataSource::MusicBrainz => app.runtime.block_on(
            app.services
                .import()
                .search_musicbrainz(artist, album, None, None),
        ),
    };
    let results = match results {
        Ok(results) => results,
        Err(e) => {
            tracing::error!("bae_search_releases failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let candidates: Vec<FfiCandidate> = results.iter().map(metadata_to_ffi).collect();
    json_cstring(&candidates)
}

/// Re-identify a release as the chosen candidate (an exact match from
/// [`bae_search_releases`]). Rewrites identity + the metadata pointer the way a
/// re-import with the same choice would. Returns null on success, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id`, `chosen_release_id`, and `source` must be valid NUL-terminated
/// UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_reidentify_release(
    handle: *const BaeHandle,
    release_id: *const c_char,
    chosen_release_id: *const c_char,
    source: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (Some(release_id), Some(chosen_release_id), Some(source)) =
        (cstr(release_id), cstr(chosen_release_id), cstr(source))
    else {
        return error_cstring("invalid re-identify argument");
    };
    let Some(source) = metadata_source_from_str(&source) else {
        return error_cstring("unknown metadata source");
    };
    let choice = bae_core::import::IdentityChoice::Exact {
        release_ref: bae_core::import::MetadataRef::new(chosen_release_id, source),
    };
    let app = &handle.0;
    match app.runtime.block_on(
        app.services
            .library_manager()
            .re_identify_release(&release_id, choice),
    ) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e.to_string()),
    }
}

/// Add a folder to import from. The folder joins the watched-folder set, which
/// scans it and reconciles its release candidates as the library changes on
/// disk; discovered candidates arrive as `CandidateAdded` events and
/// `ScanFinished` fires when the walk completes. `clear_first` is retained for
/// ABI compatibility but ignored — the watched-folder model reconciles
/// candidates rather than clearing them. Returns null on success, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `path` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_scan_folder(
    handle: *const BaeHandle,
    path: *const c_char,
    _clear_first: bool,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let Some(path) = cstr(path) else {
        return error_cstring("invalid folder path");
    };
    match handle.0.services.import().add_watched_folder(path) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Start auto-identifying a scanned folder candidate. Identification progress
/// and results arrive as `CandidateIdentifyState` events keyed by
/// `candidate_key` (which, for a folder candidate, is its folder path). Fire and
/// forget — the result is delivered through the event stream.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `candidate_key` and `folder_path` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_auto_identify_folder(
    handle: *const BaeHandle,
    candidate_key: *const c_char,
    folder_path: *const c_char,
) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_auto_identify_folder: null handle");
        return;
    };
    let (Some(candidate_key), Some(folder_path)) = (cstr(candidate_key), cstr(folder_path)) else {
        tracing::error!("bae_auto_identify_folder: null or non-UTF-8 argument");
        return;
    };
    let app = &handle.0;
    app.services.identify().start(candidate_key.clone());
    app.services.extraction().start(
        candidate_key,
        bae_core::signals::ExtractionSource::Folder(std::path::PathBuf::from(folder_path)),
    );
}

/// Toggle a signal in a candidate's identification toolbar — exclude it from
/// triangulation (or re-include an already-excluded one). `kind` is the badge's
/// wire kind name (`"disc_id"` / `"barcode"` / `"catalog"`); for `"catalog"`,
/// `value` is the catalog number that names which candidate to toggle (it is
/// ignored for disc ID and barcode, which are singletons). The candidate
/// re-derives its outcome and re-emits its `CandidateIdentifyState` event, so
/// the UI updates through the event stream. No-op when the candidate isn't
/// running or `kind` is unrecognized. Fire and forget.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `candidate_key`, `kind`, and `value` must be valid NUL-terminated UTF-8 C
/// strings.
#[no_mangle]
pub unsafe extern "C" fn bae_toggle_signal_for_candidate(
    handle: *const BaeHandle,
    candidate_key: *const c_char,
    kind: *const c_char,
    value: *const c_char,
) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_toggle_signal_for_candidate: null handle");
        return;
    };
    let (Some(candidate_key), Some(kind), Some(value)) =
        (cstr(candidate_key), cstr(kind), cstr(value))
    else {
        tracing::error!("bae_toggle_signal_for_candidate: null or non-UTF-8 argument");
        return;
    };
    let signal = match kind.as_str() {
        "disc_id" => bae_core::identify::ExcludedSignal::Disc,
        "barcode" => bae_core::identify::ExcludedSignal::Barcode,
        "catalog" => bae_core::identify::ExcludedSignal::Catalog(value),
        other => {
            tracing::error!("bae_toggle_signal_for_candidate: unknown signal kind {other:?}");
            return;
        }
    };
    handle
        .0
        .services
        .identify()
        .toggle_signal(&candidate_key, signal);
}

/// Re-run a candidate's identification lookups, preserving the user's signal
/// exclusions. Progress and the re-derived outcome arrive as
/// `CandidateIdentifyState` events keyed by
/// `candidate_key`. No-op when the candidate isn't running. Fire and forget.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `candidate_key` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn bae_rerun_identify_for_candidate(
    handle: *const BaeHandle,
    candidate_key: *const c_char,
) {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_rerun_identify_for_candidate: null handle");
        return;
    };
    let Some(candidate_key) = cstr(candidate_key) else {
        tracing::error!("bae_rerun_identify_for_candidate: null or non-UTF-8 candidate_key");
        return;
    };
    handle.0.services.identify().rerun(&candidate_key);
}

/// Seed the import confirmation pane from a candidate release the user has
/// chosen to import, before any DB row exists. Fetches the release from its
/// metadata source (`"discogs"` or `"musicbrainz"`) and returns a JSON
/// `FfiPrefetchedEdit` `{edit, remote_covers, local_artwork}`:
///
/// - `edit` is the raw editor form ([`bae_core::import::RawReleaseEdit`],
///   `{album_title, album_artist_text, pressing, tracks}`) — the same shape as
///   [`bae_release_edit_seed`], which the editor repopulates from and whose
///   user-mutated value rides back as `user_edit_json` in
///   [`bae_import_candidate`].
/// - `remote_covers` are the cover-art options the prefetched release detail
///   carries (Cover Art Archive / Discogs); the pre-import release isn't in the
///   DB yet, so [`bae_fetch_remote_covers`] can't supply these.
/// - `local_artwork` are the image files in the candidate's import `folder_path`
///   (relative `file_id` + absolute thumbnail `path`). A folder-scan failure
///   yields no local artwork without failing the prefetch — the remote covers
///   and edit form are independent of it.
///
/// The picked cover rides back as `selected_cover_json` in
/// [`bae_import_candidate`]. Returns null on a metadata-source fetch error. Free
/// with [`bae_string_free`].
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id`, `source`, and `folder_path` must be valid NUL-terminated UTF-8
/// C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_prefetch_candidate_edit(
    handle: *const BaeHandle,
    release_id: *const c_char,
    source: *const c_char,
    folder_path: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_prefetch_candidate_edit: null handle");
        return std::ptr::null_mut();
    };
    let (Some(release_id), Some(source), Some(folder_path)) =
        (cstr(release_id), cstr(source), cstr(folder_path))
    else {
        tracing::error!("bae_prefetch_candidate_edit: null or non-UTF-8 argument");
        return std::ptr::null_mut();
    };
    let Some(source) = metadata_source_from_str(&source) else {
        tracing::error!("bae_prefetch_candidate_edit: unknown source");
        return std::ptr::null_mut();
    };
    let app = &handle.0;
    let detail = match app
        .runtime
        .block_on(app.services.import().prefetch_release(&release_id, source))
    {
        Ok(detail) => detail,
        Err(e) => {
            tracing::error!("bae_prefetch_candidate_edit failed: {e}");
            return std::ptr::null_mut();
        }
    };

    let remote_covers: Vec<FfiRemoteCover> =
        detail.cover_art.iter().map(remote_cover_to_ffi).collect();

    // Image files in the candidate's folder. The commit worker re-walks the
    // folder, so this scan is purely to populate the picker; a failure here just
    // means no local choices, not a failed prefetch.
    let local_artwork: Vec<FfiLocalArtwork> =
        match bae_core::import::folder_scanner::collect_release_candidate_files(
            std::path::Path::new(&folder_path),
        ) {
            Ok(files) => files
                .artwork
                .into_iter()
                .map(|file| FfiLocalArtwork {
                    file_id: file.relative_path,
                    path: file.path.to_string_lossy().to_string(),
                })
                .collect(),
            Err(e) => {
                tracing::warn!(
                    "bae_prefetch_candidate_edit: scan of {folder_path} for artwork: {e}"
                );
                Vec::new()
            }
        };

    let choice = bae_core::import::IdentityChoice::Exact {
        release_ref: bae_core::import::MetadataRef::new(release_id, source),
    };
    let user_edit = bae_core::import::shape_user_edit_from_search_detail(&detail, &choice);
    let edit = bae_core::import::RawReleaseEdit::from_user_edit(user_edit, "import-track");
    json_cstring(&FfiPrefetchedEdit {
        edit,
        remote_covers,
        local_artwork,
    })
}

/// Whether a candidate release is already represented in the library, returned
/// by [`bae_check_release_in_library`]. `release_in_library` is the exact
/// pressing (same source + release id); `album_in_library` is true when that
/// pressing matches (the album it belongs to is in the library). When present,
/// `album_id` is the library album to open and `album_title` names it.
#[derive(Serialize)]
struct FfiLibraryStatus {
    release_in_library: bool,
    album_in_library: bool,
    album_id: Option<String>,
    album_title: Option<String>,
}

/// Check whether the chosen candidate (source `"discogs"` or `"musicbrainz"`
/// plus its release id) is already in the library, as a JSON
/// `FfiLibraryStatus` `{release_in_library, album_in_library, album_id,
/// album_title}`. The import confirmation shows a banner when
/// `release_in_library` is set, linking to `album_id`. Returns null on error.
/// Free with [`bae_string_free`].
///
/// The check is by release identity (source + release id); the confirm flow
/// has no group id, so the album-level lookup for a *different* pressing of the
/// same album is not run here — `album_in_library` therefore tracks the exact
/// pressing match.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed;
/// `release_id` and `source` must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_check_release_in_library(
    handle: *const BaeHandle,
    release_id: *const c_char,
    source: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        tracing::error!("bae_check_release_in_library: null handle");
        return std::ptr::null_mut();
    };
    let (Some(release_id), Some(source)) = (cstr(release_id), cstr(source)) else {
        tracing::error!("bae_check_release_in_library: null or non-UTF-8 argument");
        return std::ptr::null_mut();
    };
    let Some(source) = metadata_source_from_str(&source) else {
        tracing::error!("bae_check_release_in_library: unknown source");
        return std::ptr::null_mut();
    };
    let check = bae_core::db::LibraryCheck {
        release_id,
        source,
        source_group_id: None,
    };
    let app = &handle.0;
    let statuses = match app.runtime.block_on(
        app.services
            .library_manager()
            .check_releases_in_library(&[check]),
    ) {
        Ok(statuses) => statuses,
        Err(e) => {
            tracing::error!("bae_check_release_in_library failed: {e}");
            return std::ptr::null_mut();
        }
    };
    let Some(status) = statuses.into_iter().next() else {
        tracing::error!("bae_check_release_in_library: no status for single check");
        return std::ptr::null_mut();
    };
    json_cstring(&FfiLibraryStatus {
        release_in_library: status.release_in_library,
        album_in_library: status.album_in_library,
        album_id: status.album_id,
        album_title: status.album_title,
    })
}

/// Import a scanned candidate as the chosen identity (a match from a
/// `CandidateIdentifyState` event, or a manual search). `storage_mode` is
/// `"unmanaged"`, `"managed_pinned"`, or `"managed_unpinned"`. `user_edit_json`
/// overlays the user's confirmed metadata edits onto the committed release:
/// when null or empty the release seeds straight from the source (no edit),
/// otherwise it is a JSON [`bae_core::import::RawReleaseEdit`] (the shape
/// [`bae_prefetch_candidate_edit`] returns and the editor mutated).
/// `selected_cover_json` is the cover the user picked in the confirm pane: when
/// null or empty the import uses its default cover (the source's first cover
/// art, else a folder image, else embedded art); otherwise it is an
/// `FfiCoverSelection` JSON (`{"type":"release_image","file_id":"…"}` for a
/// folder image, `{"type":"remote_cover","url":"…","source":"musicbrainz"}` for
/// a remote one). The import runs in the background; progress and the result
/// arrive as `CandidateImportProgress` / `CandidateImportComplete` /
/// `CandidateImportError` events. Returns null on a successful enqueue, or an
/// error-message C string (free with [`bae_string_free`]).
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed; all
/// non-null string arguments must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn bae_import_candidate(
    handle: *const BaeHandle,
    candidate_key: *const c_char,
    folder_path: *const c_char,
    chosen_release_id: *const c_char,
    source: *const c_char,
    storage_mode: *const c_char,
    user_edit_json: *const c_char,
    selected_cover_json: *const c_char,
) -> *mut c_char {
    let Some(handle) = handle.as_ref() else {
        return error_cstring("no app handle");
    };
    let (
        Some(candidate_key),
        Some(folder_path),
        Some(chosen_release_id),
        Some(source),
        Some(storage_mode),
    ) = (
        cstr(candidate_key),
        cstr(folder_path),
        cstr(chosen_release_id),
        cstr(source),
        cstr(storage_mode),
    )
    else {
        return error_cstring("invalid import argument");
    };
    let Some(source) = metadata_source_from_str(&source) else {
        return error_cstring("unknown metadata source");
    };
    let Some(storage_mode) = storage_mode_from_str(&storage_mode) else {
        return error_cstring("unknown storage mode");
    };
    let user_edit = match cstr(user_edit_json) {
        Some(json) if !json.is_empty() => {
            let raw: bae_core::import::RawReleaseEdit = match serde_json::from_str(&json) {
                Ok(raw) => raw,
                Err(e) => return error_cstring(&format!("malformed edit: {e}")),
            };
            match raw.shape() {
                Ok(edit) => Some(edit),
                Err(e) => return error_cstring(&e.to_string()),
            }
        }
        _ => None,
    };
    let selected_cover = match cstr(selected_cover_json) {
        Some(json) if !json.is_empty() => {
            let selection = match serde_json::from_str::<FfiCoverSelection>(&json) {
                Ok(selection) => selection,
                Err(e) => return error_cstring(&format!("invalid cover selection: {e}")),
            };
            match ffi_cover_to_import(selection) {
                Ok(cover) => Some(cover),
                Err(e) => return error_cstring(&e),
            }
        }
        _ => None,
    };
    let identity_choice = bae_core::import::IdentityChoice::Exact {
        release_ref: bae_core::import::MetadataRef::new(chosen_release_id, source),
    };
    let app = &handle.0;
    match app.services.import().start_import(
        &candidate_key,
        std::path::PathBuf::from(folder_path),
        selected_cover,
        storage_mode,
        identity_choice,
        user_edit,
    ) {
        Ok(_import_id) => std::ptr::null_mut(),
        Err(e) => error_cstring(&e),
    }
}

/// Persist playback state and stop playback before exit. Blocks until the
/// snapshot (queue, current track, position) is durable. Call before
/// [`bae_handle_free`] so a later launch can restore where playback left off.
///
/// # Safety
/// `handle` must be a pointer returned by [`bae_init`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn bae_shutdown(handle: *const BaeHandle) {
    let Some(handle) = handle.as_ref() else {
        return;
    };
    let app = &handle.0;
    app.runtime.block_on(app.services.playback().shutdown());
}

/// Release a handle created by [`bae_init`].
///
/// # Safety
/// `handle` must be a pointer from [`bae_init`] that has not already been freed;
/// it must not be used afterward.
#[no_mangle]
pub unsafe extern "C" fn bae_handle_free(handle: *mut BaeHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Release a string returned by this library.
///
/// # Safety
/// `ptr` must be a string returned by one of this library's functions, not freed
/// before, and not used afterward.
#[no_mangle]
pub unsafe extern "C" fn bae_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_mode_tags_carry_pin_choice() {
        match storage_mode_from_str("unmanaged") {
            Some(bae_core::import::StorageMode::Unmanaged) => {}
            other => panic!("unexpected unmanaged mode: {other:?}"),
        }
        match storage_mode_from_str("managed_pinned") {
            Some(bae_core::import::StorageMode::Managed { pin: true }) => {}
            other => panic!("unexpected managed_pinned mode: {other:?}"),
        }
        match storage_mode_from_str("managed_unpinned") {
            Some(bae_core::import::StorageMode::Managed { pin: false }) => {}
            other => panic!("unexpected managed_unpinned mode: {other:?}"),
        }
        assert!(storage_mode_from_str("managed").is_none());
    }

    /// The WinUI "go to now playing" navigation needs the album and track ids of
    /// the playing track, so the playback events must carry them through to the
    /// app. Drop them from the mapping and the app has nothing to navigate to.
    #[test]
    fn playback_events_carry_track_and_album_id() {
        let playing = UiBusEvent::PlaybackPlaying {
            track_id: "trk-1".to_string(),
            track_title: "Track Title".to_string(),
            artist_names: "Artist Name".to_string(),
            artist_id: "art-1".to_string(),
            album_id: "alb-1".to_string(),
            album_title: "Album Title".to_string(),
            cover_image_id: None,
            duration_ms: 1_000,
        };
        let paused = UiBusEvent::PlaybackPaused {
            track_id: "trk-1".to_string(),
            track_title: "Track Title".to_string(),
            artist_names: "Artist Name".to_string(),
            artist_id: "art-1".to_string(),
            album_id: "alb-1".to_string(),
            album_title: "Album Title".to_string(),
            cover_image_id: None,
            duration_ms: 1_000,
        };

        for event in [playing, paused] {
            let ffi = map_event(&event).expect("playback event maps to an FFI event");
            let json = serde_json::to_value(&ffi).expect("FFI event serializes");
            assert_eq!(json["track_id"], "trk-1");
            assert_eq!(json["album_id"], "alb-1");
        }
    }

    #[test]
    fn remote_cover_ffi_preserves_thumbnail_url() {
        let cover = bae_core::import::cover_art::RemoteCover {
            url: "https://cover.example/full.jpg".to_string(),
            thumbnail_url: "https://cover.example/thumb.jpg".to_string(),
            label: "Cover Source".to_string(),
            source: bae_core::import::MetadataSource::MusicBrainz,
        };

        let ffi = remote_cover_to_ffi(&cover);

        assert_eq!(ffi.url, "https://cover.example/full.jpg");
        assert_eq!(ffi.thumbnail_url, "https://cover.example/thumb.jpg");
        assert_eq!(ffi.label, "Cover Source");
        assert_eq!(ffi.source, "musicbrainz");
    }

    #[test]
    fn diagnostics_level_tags_map_to_core_levels() {
        assert_eq!(
            diagnostic_level_from_str("trace"),
            Some(DiagnosticLevel::Trace)
        );
        assert_eq!(
            diagnostic_level_from_str("debug"),
            Some(DiagnosticLevel::Debug)
        );
        assert_eq!(
            diagnostic_level_from_str("info"),
            Some(DiagnosticLevel::Info)
        );
        assert_eq!(
            diagnostic_level_from_str("warn"),
            Some(DiagnosticLevel::Warn)
        );
        assert_eq!(
            diagnostic_level_from_str("error"),
            Some(DiagnosticLevel::Error)
        );
        assert_eq!(diagnostic_level_from_str("warning"), None);
    }

    #[test]
    fn diagnostics_fields_json_maps_key_value_pairs() {
        let fields = diagnostic_fields_from_json(
            r#"[{"key":"action","value":"startup"},{"key":"count","value":"1"}]"#,
        )
        .expect("diagnostic fields parse");

        assert_eq!(
            fields,
            vec![
                ("action".to_string(), "startup".to_string()),
                ("count".to_string(), "1".to_string()),
            ]
        );
    }

    #[test]
    fn diagnostics_fields_json_rejects_bad_payloads() {
        assert!(diagnostic_fields_from_json(r#"{"key":"action","value":"startup"}"#).is_err());
        assert!(diagnostic_fields_from_json(r#"[{"key":"action"}]"#).is_err());
    }

    #[test]
    fn windows_diagnostics_config_disables_baeium_and_missing_datadog_config() {
        assert_eq!(
            diagnostics_config_from_parts(
                Some("datadoghq.com".to_string()),
                Some("token".to_string()),
                "windows".to_string(),
                Some(AppDiagnosticMetadata {
                    service: "bae".to_string(),
                    environment: "dev".to_string(),
                    app_version: "0.0-dev".to_string(),
                    edition: "baeium".to_string(),
                    git_commit: "commit".to_string(),
                }),
            ),
            DiagnosticsConfig::Disabled
        );
        assert_eq!(
            diagnostics_config_from_parts(
                Some("datadoghq.com".to_string()),
                None,
                "windows".to_string(),
                Some(AppDiagnosticMetadata {
                    service: "bae".to_string(),
                    environment: "dev".to_string(),
                    app_version: "0.0-dev".to_string(),
                    edition: "bae".to_string(),
                    git_commit: "commit".to_string(),
                }),
            ),
            DiagnosticsConfig::Disabled
        );
    }

    #[test]
    fn windows_diagnostics_config_enables_complete_bae_config() {
        let config = diagnostics_config_from_parts(
            Some("datadoghq.com".to_string()),
            Some("token".to_string()),
            "windows".to_string(),
            Some(AppDiagnosticMetadata {
                service: "bae".to_string(),
                environment: "dev".to_string(),
                app_version: "0.0-dev".to_string(),
                edition: "bae".to_string(),
                git_commit: "commit".to_string(),
            }),
        );

        let DiagnosticsConfig::Enabled(config) = config else {
            panic!("complete bae diagnostics config should be enabled");
        };
        assert_eq!(config.datadog_site, "datadoghq.com");
        assert_eq!(config.client_token, "token");
        assert_eq!(config.source, "windows");
        assert_eq!(config.app.environment, "dev");
    }
}

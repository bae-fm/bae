using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Bae.Windows;

/// <summary>
/// P/Invoke surface over bae_windows_ffi.dll — the hand-written C ABI that the
/// Rust crate bae-windows-ffi exposes over bae-core. The handle is opaque
/// (<see cref="IntPtr"/>); strings the library returns are owned by Rust and must
/// be released with <see cref="StringFree"/>.
///
/// Rust's <c>extern "C"</c> uses the cdecl calling convention, so every entry
/// point is declared <see cref="CallingConvention.Cdecl"/>. The DLL is resolved
/// by name from the application directory, so the CI build copies it next to the
/// WinUI output.
/// </summary>
internal static class NativeBae
{
    private const string Dll = "bae_windows_ffi.dll";

    /// <summary>One-time startup: register the OS credential store.</summary>
    [DllImport(Dll, EntryPoint = "bae_startup", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void Startup();

    [DllImport(Dll, EntryPoint = "bae_configure_diagnostics", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ConfigureDiagnosticsPtr(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? datadogSite,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? clientToken,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string source,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string service,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? environment,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string appVersion,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string edition,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? gitCommit);

    internal static string? ConfigureDiagnostics(
        string? datadogSite,
        string? clientToken,
        string source,
        string service,
        string? environment,
        string appVersion,
        string edition,
        string? gitCommit) =>
        ResultMessage(ConfigureDiagnosticsPtr(datadogSite, clientToken, source, service, environment, appVersion, edition, gitCommit));

    [DllImport(Dll, EntryPoint = "bae_diagnostics_log", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DiagnosticsLogPtr(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string level,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string target,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string message,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string fieldsJson);

    internal static string? DiagnosticsLog(
        string level,
        string target,
        string message,
        IEnumerable<KeyValuePair<string, string>>? fields = null) =>
        ResultMessage(DiagnosticsLogPtr(level, target, message, DiagnosticFieldsJson(fields)));

    [DllImport(Dll, EntryPoint = "bae_diagnostics_event", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DiagnosticsEventPtr(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string name,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string fieldsJson);

    internal static string? DiagnosticsEvent(
        string name,
        IEnumerable<KeyValuePair<string, string>>? fields = null) =>
        ResultMessage(DiagnosticsEventPtr(name, DiagnosticFieldsJson(fields)));

    [DllImport(Dll, EntryPoint = "bae_flush_diagnostics", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr FlushDiagnosticsPtr();

    internal static string? FlushDiagnostics() => ResultMessage(FlushDiagnosticsPtr());

    [DllImport(Dll, EntryPoint = "bae_set_oauth_client_creds", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SetOauthClientCredsPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string credsJson);

    /// <summary>
    /// Register the OAuth client credentials JSON so coven can build authorization
    /// URLs and refresh tokens. Call once at startup before any OAuth flow. Returns
    /// null on success, else an error message.
    /// </summary>
    internal static string? SetOauthClientCreds(string credsJson) =>
        ResultMessage(SetOauthClientCredsPtr(credsJson));

    [DllImport(Dll, EntryPoint = "bae_available_cloud_providers", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr AvailableCloudProvidersPtr();

    /// <summary>
    /// The cloud-provider wire tags this build's native library supports. Always
    /// includes <c>"s3"</c>; <c>"google_drive"</c>/<c>"dropbox"</c>/<c>"onedrive"</c>
    /// are present only when bae_windows_ffi.dll was built with the oauth-providers
    /// feature (the baeium build omits them, and with it the OAuth entry points). The
    /// UI offers only these providers, so it never P/Invokes an OAuth entry point a
    /// baeium DLL doesn't export. This entry point is always exported. Copies and frees.
    /// </summary>
    internal static string[] AvailableCloudProviders()
    {
        var json = CopyAndFree(AvailableCloudProvidersPtr())
            ?? throw new InvalidOperationException("bae_available_cloud_providers returned null");
        return JsonSerializer.Deserialize<string[]>(json)
            ?? throw new InvalidOperationException($"bae_available_cloud_providers returned invalid JSON: {json}");
    }

    /// <summary>
    /// Whether this build's native library supports any OAuth cloud provider (i.e. it
    /// was built with the oauth-providers feature). When false, the OAuth entry points
    /// are absent and no OAuth flow — including credential registration — must run.
    /// </summary>
    internal static bool SupportsOAuthProviders() =>
        AvailableCloudProviders().Any(provider => provider is not "s3");

    // ── Catalog-key selection ────────────────────────────────────────────────
    //
    // The enum→key mapping macOS gets free from uniffi's bridge_*_key() functions
    // is hand-mirrored in bae-windows-ffi (src/loc.rs) and exported as these
    // entry points, so the key strings have one source (Rust), not a duplicate in
    // C#. Each returns the core.* catalog key for the value, or null when the
    // value has no key (the caller falls back to a passthrough / generic line).
    // The C# resolves the returned key through Loc.Core.

    [DllImport(Dll, EntryPoint = "bae_cloud_provider_label_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr CloudProviderLabelKeyPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string? provider);

    /// <summary>The catalog key for a cloud provider's display name, or null for
    /// the brand-name providers the UI passes through verbatim. <paramref name="provider"/>
    /// is the wire tag ("s3"/"google_drive"/…) or null/"" for local-only.</summary>
    internal static string? CloudProviderLabelKey(string? provider) =>
        CopyAndFree(CloudProviderLabelKeyPtr(provider));

    [DllImport(Dll, EntryPoint = "bae_audio_channels_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr AudioChannelsKeyPtr(long channels);

    /// <summary>The catalog key for a channel count's word ("mono"/"stereo"), or
    /// null for counts the UI renders as "{n}ch".</summary>
    internal static string? AudioChannelsKey(long channels) =>
        CopyAndFree(AudioChannelsKeyPtr(channels));

    [DllImport(Dll, EntryPoint = "bae_error_category_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ErrorCategoryKeyPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string category);

    /// <summary>The catalog key for a diagnostic error category's generic line
    /// (the wire tag an FfiError carries), or null for an unknown tag.</summary>
    internal static string? ErrorCategoryKey(string category) =>
        CopyAndFree(ErrorCategoryKeyPtr(category));

    [DllImport(Dll, EntryPoint = "bae_entity_not_found_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr EntityNotFoundKeyPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string entity);

    /// <summary>The catalog key for a missing entity's "… not found" line (the
    /// wire tag an FfiError carries), or null for an unknown tag.</summary>
    internal static string? EntityNotFoundKey(string entity) =>
        CopyAndFree(EntityNotFoundKeyPtr(entity));

    [DllImport(Dll, EntryPoint = "bae_playback_error_reason_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr PlaybackErrorReasonKeyPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string kind);

    /// <summary>The catalog key for an actionable playback-error reason (the wire
    /// tag the reason carries), or null for the "diagnostic" reason (rendered
    /// through the error-category path) and unknown tags.</summary>
    internal static string? PlaybackErrorReasonKey(string kind) =>
        CopyAndFree(PlaybackErrorReasonKeyPtr(kind));

    [DllImport(Dll, EntryPoint = "bae_prepare_step_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr PrepareStepKeyPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string step);

    /// <summary>The catalog key for an import prepare-step wire tag, or null for
    /// an unknown tag.</summary>
    internal static string? PrepareStepKey(string step) =>
        CopyAndFree(PrepareStepKeyPtr(step));

    [DllImport(Dll, EntryPoint = "bae_import_phase_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ImportPhaseKeyPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string phase);

    /// <summary>The catalog key for an import-phase wire tag, or null for an
    /// unknown tag.</summary>
    internal static string? ImportPhaseKey(string phase) =>
        CopyAndFree(ImportPhaseKeyPtr(phase));

    [DllImport(Dll, EntryPoint = "bae_transfer_action_key", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr TransferActionKeyPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string action);

    /// <summary>The catalog key for a transfer action's progress verb (a wire tag
    /// from a storage row's actions), or null for an unknown tag.</summary>
    internal static string? TransferActionKey(string action) =>
        CopyAndFree(TransferActionKeyPtr(action));

    /// <summary>Discovered libraries as JSON, or null. Copies and frees.</summary>
    [DllImport(Dll, EntryPoint = "bae_libraries", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr LibrariesPtr();

    /// <summary>Create a library; returns its id string, or null. Copies and frees.</summary>
    [DllImport(Dll, EntryPoint = "bae_create_library", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr CreateLibraryPtr();

    /// <summary>The discovered libraries as a JSON string, or null.</summary>
    internal static string? LibrariesJson() => CopyAndFree(LibrariesPtr());

    /// <summary>Create a new library; returns its id, or null on error.</summary>
    internal static string? CreateLibrary() => CopyAndFree(CreateLibraryPtr());

    [DllImport(Dll, EntryPoint = "bae_decode_restore_code", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DecodeRestoreCodePtr([MarshalAs(UnmanagedType.LPUTF8Str)] string code);

    [DllImport(Dll, EntryPoint = "bae_oauth_authorize", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr OAuthAuthorizePtr([MarshalAs(UnmanagedType.LPUTF8Str)] string provider);

    [DllImport(Dll, EntryPoint = "bae_restore_from_code", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr RestoreFromCodePtr(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string code,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? oauthTokenJson);

    /// <summary>
    /// Run the desktop OAuth flow for a provider (google_drive / dropbox / onedrive)
    /// and return a result JSON (<c>{token, error}</c>): <c>token</c> is the provider's
    /// token JSON for <see cref="RestoreFromCode"/>, <c>error</c> the reason it failed.
    /// The core opens the system browser and runs the 127.0.0.1 callback listener —
    /// blocks until the user finishes, so call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? OAuthAuthorize(string provider) => CopyAndFree(OAuthAuthorizePtr(provider));

    /// <summary>Decode a restore code to its info JSON, or null if malformed.</summary>
    internal static string? DecodeRestoreCode(string code) => CopyAndFree(DecodeRestoreCodePtr(code));

    /// <summary>
    /// Restore a library from a code; returns a result JSON (<c>{library_id,
    /// error}</c>). For OAuth providers pass the token JSON from
    /// <see cref="OAuthAuthorize"/>; for credential providers pass null. Blocks on a
    /// cloud pull — call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? RestoreFromCode(string code, string? oauthTokenJson) =>
        CopyAndFree(RestoreFromCodePtr(code, oauthTokenJson));

    [DllImport(Dll, EntryPoint = "bae_generate_join_request", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr GenerateJoinRequestPtr();

    /// <summary>
    /// This device's join-request code and the fingerprint it encodes, as JSON
    /// (<c>{code, fingerprint}</c>), to hand to an existing member for approval,
    /// or null on error. The joining device has no library yet, so this needs no
    /// handle; it only requires <see cref="Startup"/>. Copies and frees.
    /// </summary>
    internal static string? GenerateJoinRequest() => CopyAndFree(GenerateJoinRequestPtr());

    [DllImport(Dll, EntryPoint = "bae_decode_join_request", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DecodeJoinRequestPtr([MarshalAs(UnmanagedType.LPUTF8Str)] string code);

    /// <summary>
    /// Decode a join-request code to its info JSON (<c>{pubkey, fingerprint,
    /// email}</c>), or null if malformed. Copies and frees.
    /// </summary>
    internal static string? DecodeJoinRequest(string code) => CopyAndFree(DecodeJoinRequestPtr(code));

    [DllImport(Dll, EntryPoint = "bae_decode_invite_code", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DecodeInviteCodePtr([MarshalAs(UnmanagedType.LPUTF8Str)] string code);

    /// <summary>
    /// Decode an invite code to its info JSON (<c>{library_id, library_name,
    /// owner_pubkey, owner_fingerprint, provider, needs_oauth}</c>), or null if
    /// malformed. Copies and frees.
    /// </summary>
    internal static string? DecodeInviteCode(string code) => CopyAndFree(DecodeInviteCodePtr(code));

    [DllImport(Dll, EntryPoint = "bae_join_from_code", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr JoinFromCodePtr(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string code,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? oauthTokenJson);

    /// <summary>
    /// Join a shared library from an invite code; returns a result JSON
    /// (<c>{library_id, error}</c>). For OAuth providers pass the token JSON from
    /// <see cref="OAuthAuthorize"/>; for credential providers pass null. Blocks on
    /// a cloud pull — call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? JoinFromCode(string code, string? oauthTokenJson) =>
        CopyAndFree(JoinFromCodePtr(code, oauthTokenJson));

    [DllImport(Dll, EntryPoint = "bae_restore_from_cloud", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr RestoreFromCloudPtr(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string libraryId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string encryptionKeyHex,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? libraryName,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string sourceJson);

    /// <summary>
    /// Restore a library by entering its cloud location and credentials directly
    /// (no restore code); returns a result JSON (<c>{library_id, error}</c>).
    /// <paramref name="sourceJson"/> is a tagged source (<c>{"type":"s3",…}</c>);
    /// an empty <paramref name="libraryName"/> generates one. Blocks on a cloud
    /// pull — call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? RestoreFromCloud(
        string libraryId, string encryptionKeyHex, string? libraryName, string sourceJson) =>
        CopyAndFree(RestoreFromCloudPtr(libraryId, encryptionKeyHex, libraryName, sourceJson));

    /// <summary>
    /// Initialize the app for <paramref name="libraryId"/>. Returns an opaque
    /// handle, or <see cref="IntPtr.Zero"/> on failure (the Rust side logs the
    /// error). Free the result with <see cref="HandleFree"/>.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_init", CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr Init(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string libraryId,
        uint positionUpdateIntervalMs);

    /// <summary>Whether this library's encryption key is loaded. False means the
    /// library is locked (key not on this device) and needs <see cref="UnlockLibrary"/>.</summary>
    [DllImport(Dll, EntryPoint = "bae_has_encryption_key", CallingConvention = CallingConvention.Cdecl)]
    [return: MarshalAs(UnmanagedType.I1)]
    internal static extern bool HasEncryptionKey(IntPtr handle);

    [DllImport(Dll, EntryPoint = "bae_unlock_library", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr UnlockLibraryPtr(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string libraryId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string keyHex);

    /// <summary>Store a library's hex key so it can be opened; null on success, else the error.</summary>
    internal static string? UnlockLibrary(string libraryId, string keyHex) =>
        ResultMessage(UnlockLibraryPtr(libraryId, keyHex));

    [DllImport(Dll, EntryPoint = "bae_lock_active_library", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr LockActiveLibraryPtr(IntPtr handle);

    /// <summary>Forget the active library's key (lock it); null on success, else the error.</summary>
    internal static string? LockActiveLibrary(IntPtr handle) =>
        ResultMessage(LockActiveLibraryPtr(handle));

    [DllImport(Dll, EntryPoint = "bae_rename_library", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr RenameLibraryPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string libraryId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string name);

    /// <summary>Rename a library (active or inactive); null on success, else the error.</summary>
    internal static string? RenameLibrary(IntPtr handle, string libraryId, string name) =>
        ResultMessage(RenameLibraryPtr(handle, libraryId, name));

    [DllImport(Dll, EntryPoint = "bae_set_primary_release", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SetPrimaryReleasePtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string albumId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>Set an album's primary release; null on success, else the error.</summary>
    internal static string? SetPrimaryRelease(IntPtr handle, string albumId, string releaseId) =>
        ResultMessage(SetPrimaryReleasePtr(handle, albumId, releaseId));

    [DllImport(Dll, EntryPoint = "bae_export_track", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ExportTrackPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string trackId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string outputPath,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string format);

    /// <summary>Export one track to outputPath as "flac" or "mp3"; null on success, else the error.</summary>
    internal static string? ExportTrack(IntPtr handle, string trackId, string outputPath, string format) =>
        ResultMessage(ExportTrackPtr(handle, trackId, outputPath, format));

    [DllImport(Dll, EntryPoint = "bae_get_release_images", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr GetReleaseImagesPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>
    /// A release's local image files (cover-art candidates) as a JSON array of
    /// <c>{id, original_filename}</c>, or null on error. Copies and frees.
    /// </summary>
    internal static string? GetReleaseImagesJson(IntPtr handle, string releaseId) =>
        CopyAndFree(GetReleaseImagesPtr(handle, releaseId));

    [DllImport(Dll, EntryPoint = "bae_fetch_remote_covers", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr FetchRemoteCoversPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>
    /// Remote cover-art candidates for a release (MusicBrainz / Discogs) as a JSON
    /// array of <c>{url, thumbnail_url, label, source}</c>, or null on error.
    /// Performs network I/O — call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? FetchRemoteCoversJson(IntPtr handle, string releaseId) =>
        CopyAndFree(FetchRemoteCoversPtr(handle, releaseId));

    [DllImport(Dll, EntryPoint = "bae_change_cover", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ChangeCoverPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string albumId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string selectionJson);

    /// <summary>
    /// Change a release's cover art from a selection JSON (a release image file or
    /// a remote URL); null on success, else the error. Performs network I/O for a
    /// remote cover — call off the UI thread.
    /// </summary>
    internal static string? ChangeCover(IntPtr handle, string albumId, string releaseId, string selectionJson) =>
        ResultMessage(ChangeCoverPtr(handle, albumId, releaseId, selectionJson));

    /// <summary>
    /// A page of albums (newest first) as a JSON array, or <see cref="IntPtr.Zero"/>
    /// on error. The returned string is owned by Rust; prefer
    /// <see cref="AlbumPageJson"/>, which copies and frees it.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_album_page", CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr AlbumPage(
        IntPtr handle,
        ulong offset,
        ulong limit,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string sortField,
        [MarshalAs(UnmanagedType.I1)] bool ascending);

    /// <summary>
    /// A page of albums sorted by <paramref name="sortField"/> as a JSON string, or
    /// null on error. Copies the string into managed memory and frees the native one.
    /// </summary>
    internal static string? AlbumPageJson(IntPtr handle, ulong offset, ulong limit, string sortField, bool ascending) =>
        CopyAndFree(AlbumPage(handle, offset, limit, sortField, ascending));

    /// <summary>
    /// A release's gallery images as JSON, or <see cref="IntPtr.Zero"/> on error.
    /// Prefer <see cref="GalleryJson"/>.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_gallery", CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr GalleryPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>A release's gallery as a JSON string, or null. Copies and frees.</summary>
    internal static string? GalleryJson(IntPtr handle, string releaseId) =>
        CopyAndFree(GalleryPtr(handle, releaseId));

    /// <summary>
    /// Every release's storage summary as JSON, or <see cref="IntPtr.Zero"/> on
    /// error. Prefer <see cref="StorageJson"/>.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_storage", CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr StoragePtr(IntPtr handle);

    /// <summary>
    /// Every release's storage summary as a JSON string, or null on error. Copies
    /// the string into managed memory and frees the native one.
    /// </summary>
    internal static string? StorageJson(IntPtr handle) => CopyAndFree(StoragePtr(handle));

    [DllImport(Dll, EntryPoint = "bae_pin_release", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr PinReleasePtr(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    [DllImport(Dll, EntryPoint = "bae_unpin_release", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr UnpinReleasePtr(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    [DllImport(Dll, EntryPoint = "bae_make_release_remote", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr MakeReleaseRemotePtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.I1)] bool pin);

    /// <summary>Pin a cloud-only release locally; null on success, else the error.</summary>
    internal static string? PinRelease(IntPtr handle, string releaseId) =>
        ResultMessage(PinReleasePtr(handle, releaseId));

    /// <summary>Unpin a release (drop the local copy); null on success, else the error.</summary>
    internal static string? UnpinRelease(IntPtr handle, string releaseId) =>
        ResultMessage(UnpinReleasePtr(handle, releaseId));

    /// <summary>
    /// Make a local release remote: upload it to the cloud and drop the in-place
    /// source (a remote release has no local path). <paramref name="pin"/> keeps
    /// coven's blobs offline — the orthogonal "keep local" choice. Null on success,
    /// else the error.
    /// </summary>
    internal static string? MakeReleaseRemote(IntPtr handle, string releaseId, bool pin) =>
        ResultMessage(MakeReleaseRemotePtr(handle, releaseId, pin));

    [DllImport(Dll, EntryPoint = "bae_make_release_local", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr MakeReleaseLocalPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string newPath);

    /// <summary>Move a release's files out of the library; null on success, else the error.</summary>
    internal static string? MakeReleaseLocal(IntPtr handle, string releaseId, string newPath) =>
        ResultMessage(MakeReleaseLocalPtr(handle, releaseId, newPath));

    [DllImport(Dll, EntryPoint = "bae_outbox_snapshot", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr OutboxSnapshotPtr(IntPtr handle);

    /// <summary>The cloud outbox snapshot as a JSON string, or null on error.
    /// Copies into managed memory and frees the native string.</summary>
    internal static string? OutboxSnapshotJson(IntPtr handle) => CopyAndFree(OutboxSnapshotPtr(handle));

    [DllImport(Dll, EntryPoint = "bae_download_snapshot", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DownloadSnapshotPtr(IntPtr handle);

    /// <summary>The download (pin) queue snapshot as a JSON string, or null on
    /// error. Copies into managed memory and frees the native string.</summary>
    internal static string? DownloadSnapshotJson(IntPtr handle) => CopyAndFree(DownloadSnapshotPtr(handle));

    [DllImport(Dll, EntryPoint = "bae_set_downloads_paused", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SetDownloadsPausedNative(IntPtr handle, [MarshalAs(UnmanagedType.I1)] bool paused);

    /// <summary>Pause or resume the download (pin) queue.</summary>
    internal static void SetDownloadsPaused(IntPtr handle, bool paused) => SetDownloadsPausedNative(handle, paused);

    [DllImport(Dll, EntryPoint = "bae_retry_downloads", CallingConvention = CallingConvention.Cdecl)]
    private static extern void RetryDownloadsNative(IntPtr handle);

    /// <summary>Retry failed downloads now (re-queues them).</summary>
    internal static void RetryDownloads(IntPtr handle) => RetryDownloadsNative(handle);

    [DllImport(Dll, EntryPoint = "bae_retry_outbox", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr RetryOutboxPtr(IntPtr handle);

    /// <summary>Retry the cloud outbox now; null on success, else the error.</summary>
    internal static string? RetryOutbox(IntPtr handle) => ResultMessage(RetryOutboxPtr(handle));

    /// <summary>Pause or resume the cloud sync pipeline (drains the outbox when resumed).</summary>
    [DllImport(Dll, EntryPoint = "bae_set_sync_paused", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void SetSyncPaused(IntPtr handle, [MarshalAs(UnmanagedType.I1)] bool paused);

    [DllImport(Dll, EntryPoint = "bae_cancel_outbox_item", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr CancelOutboxItemPtr(IntPtr handle, long id);

    /// <summary>Cancel one queued outbox entry by id; null on success, else the error.</summary>
    internal static string? CancelOutboxItem(IntPtr handle, long id) =>
        ResultMessage(CancelOutboxItemPtr(handle, id));

    [DllImport(Dll, EntryPoint = "bae_cancel_release_transition", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr CancelReleaseTransitionPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>Cancel whatever transition a release is mid-flight (pin / upload /
    /// unmanage), leaving it in its prior state; null on success, else the error.</summary>
    internal static string? CancelReleaseTransition(IntPtr handle, string releaseId) =>
        ResultMessage(CancelReleaseTransitionPtr(handle, releaseId));

    /// <summary>
    /// Album results for a query as JSON (same shape as a page), or
    /// <see cref="IntPtr.Zero"/> on error. Prefer <see cref="SearchJson"/>.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_search", CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr SearchPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string query);

    /// <summary>
    /// Album results for a query as a JSON string, or null on error. Copies the
    /// string into managed memory and frees the native one.
    /// </summary>
    internal static string? SearchJson(IntPtr handle, string query) => CopyAndFree(SearchPtr(handle, query));

    /// <summary>
    /// Full detail for one album as JSON, or <see cref="IntPtr.Zero"/> on error /
    /// not found. Prefer <see cref="AlbumDetailJson"/>, which copies and frees it.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_album_detail", CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr AlbumDetailPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string albumId);

    /// <summary>
    /// One album's detail as a JSON string, or null on error / not found. Copies
    /// the string into managed memory and frees the native one.
    /// </summary>
    internal static string? AlbumDetailJson(IntPtr handle, string albumId) =>
        CopyAndFree(AlbumDetailPtr(handle, albumId));

    /// <summary>Current settings as JSON, or null on error. Copies and frees.</summary>
    [DllImport(Dll, EntryPoint = "bae_settings", CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr SettingsPtr(IntPtr handle);

    /// <summary>Current settings as a JSON string, or null on error.</summary>
    internal static string? SettingsJson(IntPtr handle) => CopyAndFree(SettingsPtr(handle));

    [DllImport(Dll, EntryPoint = "bae_set_pause_between_sides", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SetPauseBetweenSidesPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.I1)] bool enabled);

    /// <summary>Set physical-side playback pauses; null on success, else the error.</summary>
    internal static string? SetPauseBetweenSides(IntPtr handle, bool enabled) =>
        ResultMessage(SetPauseBetweenSidesPtr(handle, enabled));

    [DllImport(Dll, EntryPoint = "bae_save_discogs_token", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SaveDiscogsTokenPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string token);

    [DllImport(Dll, EntryPoint = "bae_revalidate_discogs_token", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr RevalidateDiscogsTokenPtr(IntPtr handle);

    [DllImport(Dll, EntryPoint = "bae_delete_discogs_token", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DeleteDiscogsTokenPtr(IntPtr handle);

    /// <summary>
    /// Validate the token against Discogs, then persist an accepted or unreachable
    /// one. Returns the outcome — "valid" (validated and stored), "unvalidated"
    /// (couldn't reach Discogs, stored anyway and re-validated later), or "rejected"
    /// (Discogs rejected it, nothing stored) — or null on an internal error (logged
    /// in the core). Validates over the network — call off the UI thread. On
    /// "valid"/"unvalidated" a ConfigChanged event follows; "rejected" persists
    /// nothing, so the caller surfaces it from this return value. Copies and frees.
    /// </summary>
    internal static string? SaveDiscogsToken(IntPtr handle, string token) =>
        CopyAndFree(SaveDiscogsTokenPtr(handle, token));

    /// <summary>
    /// Re-validate a stored-but-unvalidated Discogs token against Discogs (e.g. one
    /// saved while offline); no-op unless a key is stored with "unvalidated" status.
    /// On a result the status changes, so a ConfigChanged event follows. Validates
    /// over the network — call off the UI thread. Null on success, else the error.
    /// </summary>
    internal static string? RevalidateDiscogsToken(IntPtr handle) =>
        ResultMessage(RevalidateDiscogsTokenPtr(handle));

    /// <summary>Remove the Discogs token; null on success, else the error message.</summary>
    internal static string? DeleteDiscogsToken(IntPtr handle) =>
        ResultMessage(DeleteDiscogsTokenPtr(handle));

    [DllImport(Dll, EntryPoint = "bae_disconnect_cloud", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DisconnectCloudPtr(IntPtr handle);

    [DllImport(Dll, EntryPoint = "bae_save_sync_config", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SaveSyncConfigPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string bucket,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string region,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string endpoint,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string keyPrefix,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string accessKey,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string secretKey,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string storage);

    /// <summary>
    /// Connect sync to an S3 bucket; null on success, else the error. `storage`
    /// is "opaque" (encrypted) or "browsable" (stored in the clear).
    /// </summary>
    internal static string? SaveSyncConfig(
        IntPtr handle, string bucket, string region, string endpoint,
        string keyPrefix, string accessKey, string secretKey, string storage) =>
        ResultMessage(SaveSyncConfigPtr(handle, bucket, region, endpoint, keyPrefix, accessKey, secretKey, storage));

    [DllImport(Dll, EntryPoint = "bae_disconnect_warning", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DisconnectWarningPtr(IntPtr handle);

    /// <summary>The data-loss warning before disconnecting sync, or null if none.</summary>
    internal static string? DisconnectWarning(IntPtr handle) => CopyAndFree(DisconnectWarningPtr(handle));

    [DllImport(Dll, EntryPoint = "bae_sign_in_cloud", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SignInCloudPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string provider,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string storage);

    /// <summary>
    /// Sign in to an OAuth provider (google_drive / dropbox / onedrive); null on
    /// success, else the error. `storage` is "opaque" (encrypted) or "browsable"
    /// (stored in the clear). Blocks on the browser flow — call off the UI thread.
    /// </summary>
    internal static string? SignInCloud(IntPtr handle, string provider, string storage) =>
        ResultMessage(SignInCloudPtr(handle, provider, storage));

    /// <summary>Disconnect cloud sync; null on success, else the error message.</summary>
    internal static string? DisconnectCloud(IntPtr handle) =>
        ResultMessage(DisconnectCloudPtr(handle));

    /// <summary>Trigger a sync pass now (no-op when not connected).</summary>
    [DllImport(Dll, EntryPoint = "bae_trigger_sync", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void TriggerSync(IntPtr handle);

    [DllImport(Dll, EntryPoint = "bae_generate_restore_code", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr GenerateRestoreCodePtr(IntPtr handle);

    /// <summary>This library's restore code, or null on error. Copies and frees.</summary>
    internal static string? GenerateRestoreCode(IntPtr handle) => CopyAndFree(GenerateRestoreCodePtr(handle));

    [DllImport(Dll, EntryPoint = "bae_get_members", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr GetMembersPtr(IntPtr handle);

    /// <summary>
    /// The library's membership as JSON <c>{members: [{pubkey, role, is_self,
    /// fingerprint, can_remove}], self_is_owner}</c>, or null on error. Blocks on
    /// a cloud read — call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? GetMembersJson(IntPtr handle) => CopyAndFree(GetMembersPtr(handle));

    [DllImport(Dll, EntryPoint = "bae_invite_member", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr InviteMemberPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string publicKeyHex);

    /// <summary>
    /// Approve a device into the library by its public key (hex); returns the
    /// invite code to hand back to the joining device, or null on error. Blocks on
    /// the cloud write — call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? InviteMember(IntPtr handle, string publicKeyHex) =>
        CopyAndFree(InviteMemberPtr(handle, publicKeyHex));

    [DllImport(Dll, EntryPoint = "bae_remove_member", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr RemoveMemberPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string publicKeyHex);

    /// <summary>
    /// Remove a device from the library by its public key (hex), rotating the
    /// library key; null on success, else the error. Blocks on the cloud write —
    /// call off the UI thread.
    /// </summary>
    internal static string? RemoveMember(IntPtr handle, string publicKeyHex) =>
        ResultMessage(RemoveMemberPtr(handle, publicKeyHex));

    [DllImport(Dll, EntryPoint = "bae_release_edit_seed", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ReleaseEditSeedPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>The metadata editor's raw form as JSON, or null. Copies and frees.</summary>
    internal static string? ReleaseEditSeedJson(IntPtr handle, string releaseId) =>
        CopyAndFree(ReleaseEditSeedPtr(handle, releaseId));

    [DllImport(Dll, EntryPoint = "bae_reset_metadata_to_source", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ResetMetadataToSourcePtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>
    /// The metadata editor's raw form re-seeded from the release's stored
    /// metadata source (its original identity), as JSON, or null. Discards
    /// in-progress edits without writing the DB. Copies and frees.
    /// </summary>
    internal static string? ResetMetadataToSourceJson(IntPtr handle, string releaseId) =>
        CopyAndFree(ResetMetadataToSourcePtr(handle, releaseId));

    [DllImport(Dll, EntryPoint = "bae_apply_release_edit", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ApplyReleaseEditPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string rawJson);

    /// <summary>Apply an edited raw form; null on success, else the error message.</summary>
    internal static string? ApplyReleaseEdit(IntPtr handle, string releaseId, string rawJson) =>
        ResultMessage(ApplyReleaseEditPtr(handle, releaseId, rawJson));

    [DllImport(Dll, EntryPoint = "bae_search_releases", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SearchReleasesPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string source,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string artist,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string album);

    /// <summary>
    /// Candidate releases matching artist + album from a metadata source as a
    /// JSON string, or null on error. Blocks on a network request — call off the
    /// UI thread. Copies and frees.
    /// </summary>
    internal static string? SearchReleasesJson(IntPtr handle, string source, string artist, string album) =>
        CopyAndFree(SearchReleasesPtr(handle, source, artist, album));

    [DllImport(Dll, EntryPoint = "bae_reidentify_release", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ReidentifyReleasePtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string chosenReleaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string source);

    /// <summary>
    /// Re-identify a release as the chosen candidate; null on success, else the
    /// error message. May block — call off the UI thread.
    /// </summary>
    internal static string? ReidentifyRelease(IntPtr handle, string releaseId, string chosenReleaseId, string source) =>
        ResultMessage(ReidentifyReleasePtr(handle, releaseId, chosenReleaseId, source));

    [DllImport(Dll, EntryPoint = "bae_scan_folder", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ScanFolderPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string path,
        [MarshalAs(UnmanagedType.I1)] bool clearFirst);

    /// <summary>
    /// Enqueue a folder scan; null on success, else the error message. Candidates
    /// arrive asynchronously as CandidateAdded events. May block briefly — call
    /// off the UI thread.
    /// </summary>
    internal static string? ScanFolder(IntPtr handle, string path, bool clearFirst) =>
        ResultMessage(ScanFolderPtr(handle, path, clearFirst));

    /// <summary>
    /// Start auto-identifying a folder candidate. Fire-and-forget; progress and
    /// results arrive as CandidateIdentifyState events keyed by candidateKey.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_auto_identify_folder", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void AutoIdentifyFolder(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateKey,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string folderPath);

    /// <summary>
    /// Exclude a signal from a candidate's triangulation, or re-include an
    /// already-excluded one. <paramref name="kind"/> is the badge's wire kind
    /// ("disc_id" / "barcode" / "catalog"); <paramref name="value"/> is the
    /// catalog number naming which catalog candidate to toggle (ignored for the
    /// disc-ID / barcode singletons — pass "" there). Fire-and-forget: the
    /// candidate re-derives and re-emits its CandidateIdentifyState event, which
    /// refreshes the badges.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_toggle_signal_for_candidate", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ToggleSignalForCandidate(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateKey,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string kind,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string value);

    /// <summary>
    /// Re-run a candidate's identification lookups, keeping the user's signal
    /// exclusions. Fire-and-forget; progress and the re-derived outcome arrive as
    /// CandidateIdentifyState events keyed by candidateKey.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_rerun_identify_for_candidate", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void RerunIdentifyForCandidate(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateKey);

    /// <summary>Preview-play an audio file by path (auditioning before import).</summary>
    [DllImport(Dll, EntryPoint = "bae_preview_play", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void PreviewPlay(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string path);

    /// <summary>Stop preview playback.</summary>
    [DllImport(Dll, EntryPoint = "bae_preview_stop", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void PreviewStop(IntPtr handle);

    /// <summary>Toggle preview play/pause.</summary>
    [DllImport(Dll, EntryPoint = "bae_preview_toggle_pause", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void PreviewTogglePause(IntPtr handle);

    [DllImport(Dll, EntryPoint = "bae_prefetch_candidate_edit", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr PrefetchCandidateEditPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string source,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string folderPath);

    /// <summary>
    /// The import confirmation seed as JSON (a <see cref="PrefetchedEdit"/>:
    /// <c>{edit, remote_covers, local_artwork}</c>), or null on error. The
    /// <c>edit</c> is the editor's raw form seeded from the chosen release;
    /// <c>remote_covers</c> are the prefetched detail's cover-art options and
    /// <c>local_artwork</c> the image files in <paramref name="folderPath"/> —
    /// the cover choices for the import picker. Blocks on a network request —
    /// call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? PrefetchCandidateEditJson(IntPtr handle, string releaseId, string source, string folderPath) =>
        CopyAndFree(PrefetchCandidateEditPtr(handle, releaseId, source, folderPath));

    [DllImport(Dll, EntryPoint = "bae_check_release_in_library", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr CheckReleaseInLibraryPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string source);

    /// <summary>
    /// Whether the chosen candidate (<paramref name="releaseId"/> from
    /// <paramref name="source"/>) is already in the library, as a JSON
    /// <see cref="LibraryStatus"/> (<c>{release_in_library, album_id}</c>), or null
    /// on error. The import confirmation shows a banner when
    /// <c>release_in_library</c> is set, linking to <c>album_id</c>. Reads the
    /// database — call off the UI thread. Copies and frees.
    /// </summary>
    internal static string? CheckReleaseInLibraryJson(IntPtr handle, string releaseId, string source) =>
        CopyAndFree(CheckReleaseInLibraryPtr(handle, releaseId, source));

    [DllImport(Dll, EntryPoint = "bae_import_candidate", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr ImportCandidatePtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string candidateKey,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string folderPath,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string chosenReleaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string source,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string storageMode,
        [MarshalAs(UnmanagedType.I1)] bool pin,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string userEditJson,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string selectedCoverJson);

    /// <summary>
    /// Import a candidate as the chosen identity; null on a successful enqueue,
    /// else the error message. <paramref name="userEditJson"/> overlays the user's
    /// confirmed metadata edits (a serialized ReleaseEdit) — pass an empty string
    /// for no edit. <paramref name="selectedCoverJson"/> is the cover the user
    /// picked (a serialized cover selection) — pass an empty string to use the
    /// import's default cover. <paramref name="storageMode"/> is <c>unmanaged</c>
    /// (leave the files in place) or <c>managed</c> (upload to the cloud);
    /// <paramref name="pin"/> is the orthogonal "keep offline" choice, meaningful
    /// only for a managed import. The import runs in the background (progress via
    /// CandidateImport* events). May block briefly — call off the UI thread.
    /// </summary>
    internal static string? ImportCandidate(
        IntPtr handle, string candidateKey, string folderPath, string chosenReleaseId, string source, string storageMode, bool pin, string userEditJson, string selectedCoverJson) =>
        ResultMessage(ImportCandidatePtr(handle, candidateKey, folderPath, chosenReleaseId, source, storageMode, pin, userEditJson, selectedCoverJson));

    private static string DiagnosticFieldsJson(IEnumerable<KeyValuePair<string, string>>? fields) =>
        JsonSerializer.Serialize((fields ?? []).Select(field => new DiagnosticField(field.Key, field.Value)));

    private sealed record DiagnosticField(
        [property: JsonPropertyName("key")] string Key,
        [property: JsonPropertyName("value")] string Value);

    /// <summary>
    /// Copy a Rust-owned UTF-8 string into managed memory and free the native one,
    /// or return null when the pointer is <see cref="IntPtr.Zero"/>.
    /// </summary>
    private static string? CopyAndFree(IntPtr ptr)
    {
        if (ptr == IntPtr.Zero)
        {
            return null;
        }

        try
        {
            return Marshal.PtrToStringUTF8(ptr);
        }
        finally
        {
            StringFree(ptr);
        }
    }

    /// Interpret a command result pointer: null = success, else an owned error
    /// string this copies and frees.
    private static string? ResultMessage(IntPtr ptr) => CopyAndFree(ptr);

    /// <summary>
    /// Callback invoked with each UI event's JSON. Fires on a background thread;
    /// the app must marshal to its UI thread. Keep the delegate instance alive for
    /// as long as the subscription lasts (it is passed to native code).
    /// </summary>
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void EventCallback(IntPtr json);

    /// <summary>Subscribe to the core UI event bus.</summary>
    [DllImport(Dll, EntryPoint = "bae_subscribe", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void Subscribe(IntPtr handle, EventCallback callback);

    /// <summary>
    /// How a bytes call ended. Mirrors the FFI's <c>BaeBytesStatus</c>
    /// (<c>#[repr(u8)]</c>).
    /// </summary>
    private enum BaeBytesStatus : byte
    {
        /// <summary>Bytes are present (<c>Ptr</c>/<c>Len</c> valid).</summary>
        Ok = 0,

        /// <summary>No such image — the caller renders the placeholder.</summary>
        Absent = 1,

        /// <summary>The call failed (bad input or a read error — already logged in Rust).</summary>
        Error = 2,
    }

    /// <summary>
    /// A byte buffer the native library hands back: a pointer, its length, and a
    /// <see cref="Status"/> that distinguishes present bytes from a genuinely absent
    /// image and from a failed call. Mirrors the FFI's <c>BaeBytes</c>
    /// (<c>#[repr(C)]</c>: <c>*mut u8</c>, <c>usize</c>, status byte). Free with
    /// <c>bae_bytes_free</c>.
    /// </summary>
    [StructLayout(LayoutKind.Sequential)]
    private struct BaeBytes
    {
        public IntPtr Ptr;
        public UIntPtr Len;
        public BaeBytesStatus Status;
    }

    [DllImport(Dll, EntryPoint = "bae_image_bytes", CallingConvention = CallingConvention.Cdecl)]
    private static extern BaeBytes ImageBytesNative(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string imageId);

    [DllImport(Dll, EntryPoint = "bae_gallery_bytes", CallingConvention = CallingConvention.Cdecl)]
    private static extern BaeBytes GalleryBytesNative(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string sourceJson);

    [DllImport(Dll, EntryPoint = "bae_bytes_free", CallingConvention = CallingConvention.Cdecl)]
    private static extern void BytesFree(BaeBytes bytes);

    /// <summary>
    /// The bytes of a library image (a cover or artist image) by id, read through
    /// coven's locality-aware read (fetched and decrypted from the cloud when not
    /// on disk), or null when there's no such image or the read failed (logged by
    /// the Rust side). Blocks on the read — call off the UI thread for cloud-only
    /// images. Copies the buffer into managed memory and frees the native one.
    /// </summary>
    internal static byte[]? ImageBytes(IntPtr handle, string imageId) =>
        CopyAndFreeBytes(ImageBytesNative(handle, imageId));

    /// <summary>
    /// The bytes of one gallery slot for (release id, source), where
    /// <paramref name="sourceJson"/> is the gallery item's <c>source</c> object
    /// forwarded verbatim — core dispatches the read on its <c>kind</c> (a cover by
    /// image id, a release file by file id). Read through coven (fetched and
    /// decrypted from the cloud when not on disk), or null on error (logged by the
    /// Rust side). Blocks on the read — call off the UI thread for cloud-only
    /// images. Copies the buffer into managed memory and frees the native one.
    /// </summary>
    internal static byte[]? GalleryBytes(IntPtr handle, string releaseId, string sourceJson) =>
        CopyAndFreeBytes(GalleryBytesNative(handle, releaseId, sourceJson));

    /// <summary>
    /// Copy a native byte buffer into a managed array and free the native one.
    /// Returns the bytes on <see cref="BaeBytesStatus.Ok"/>; null on
    /// <see cref="BaeBytesStatus.Absent"/> (no such image — the caller renders the
    /// placeholder) and on <see cref="BaeBytesStatus.Error"/> (the read failed). The
    /// error is logged here so it isn't mistaken for an absent image; the Rust side
    /// logged the cause. Always frees what the bytes call returned —
    /// <c>bae_bytes_free</c> is a no-op for an empty buffer.
    /// </summary>
    private static byte[]? CopyAndFreeBytes(BaeBytes bytes)
    {
        try
        {
            switch (bytes.Status)
            {
                case BaeBytesStatus.Ok:
                    {
                        var length = checked((int)bytes.Len.ToUInt64());
                        var managed = new byte[length];
                        Marshal.Copy(bytes.Ptr, managed, 0, length);
                        return managed;
                    }
                case BaeBytesStatus.Absent:
                    return null;
                case BaeBytesStatus.Error:
                    BaeDiagnostics.Logger.Warning("image bytes read failed (cause logged in core)");
                    return null;
                default:
                    throw new InvalidOperationException($"unknown bytes status: {bytes.Status}");
            }
        }
        finally
        {
            BytesFree(bytes);
        }
    }

    /// <summary>Start playing a release (optionally shuffled). <paramref name="startTrackIndex"/>
    /// is the track to start from; a negative value starts from the first track.</summary>
    [DllImport(Dll, EntryPoint = "bae_play_release", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void PlayRelease(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId,
        long startTrackIndex,
        [MarshalAs(UnmanagedType.I1)] bool shuffle);

    /// <summary>Play the whole library in a freshly seeded shuffle. An empty
    /// library is a no-op.</summary>
    [DllImport(Dll, EntryPoint = "bae_play_library_shuffled", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void PlayLibraryShuffled(IntPtr handle);

    /// <summary>Toggle play/pause.</summary>
    [DllImport(Dll, EntryPoint = "bae_play_pause", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void PlayPause(IntPtr handle);

    /// <summary>Seek the current track to a 0..1 ratio of its duration.</summary>
    [DllImport(Dll, EntryPoint = "bae_seek_by_ratio", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void SeekByRatio(IntPtr handle, double ratio);

    /// <summary>Set output volume (0..1).</summary>
    [DllImport(Dll, EntryPoint = "bae_set_volume", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void SetVolume(IntPtr handle, float volume);

    /// <summary>Toggle mute.</summary>
    [DllImport(Dll, EntryPoint = "bae_toggle_mute", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void ToggleMute(IntPtr handle);

    /// <summary>Current output volume (0..1).</summary>
    [DllImport(Dll, EntryPoint = "bae_get_volume", CallingConvention = CallingConvention.Cdecl)]
    internal static extern float GetVolume(IntPtr handle);

    /// <summary>Cycle repeat mode (off → context → track).</summary>
    [DllImport(Dll, EntryPoint = "bae_cycle_repeat_mode", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void CycleRepeatMode(IntPtr handle);

    /// <summary>Flip the playing context between sequential and shuffled order;
    /// the current track keeps playing.</summary>
    [DllImport(Dll, EntryPoint = "bae_set_shuffle", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void SetShuffle(IntPtr handle, [MarshalAs(UnmanagedType.I1)] bool on);

    /// <summary>Skip to the next track.</summary>
    [DllImport(Dll, EntryPoint = "bae_next", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void Next(IntPtr handle);

    /// <summary>Skip to the previous track.</summary>
    [DllImport(Dll, EntryPoint = "bae_previous", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void Previous(IntPtr handle);

    /// <summary>Jump to the queue entry with <paramref name="entryId"/>.</summary>
    [DllImport(Dll, EntryPoint = "bae_queue_skip_to_entry", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void QueueSkipTo(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string entryId);

    /// <summary>Remove the queue entry with <paramref name="entryId"/>.</summary>
    [DllImport(Dll, EntryPoint = "bae_queue_remove_entry", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void QueueRemove(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string entryId);

    /// <summary>Move the entry <paramref name="entryId"/> to sit before
    /// <paramref name="beforeEntryId"/>; a null <paramref name="beforeEntryId"/>
    /// moves it to the end of the queue.</summary>
    [DllImport(Dll, EntryPoint = "bae_queue_reorder_entry", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void QueueReorder(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string entryId,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string? beforeEntryId);

    /// <summary>Clear the play queue.</summary>
    [DllImport(Dll, EntryPoint = "bae_queue_clear", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void QueueClear(IntPtr handle);

    /// <summary>Append a release's tracks to the queue.</summary>
    [DllImport(Dll, EntryPoint = "bae_add_release_to_queue", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void AddReleaseToQueue(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>Queue a release's tracks to play next.</summary>
    [DllImport(Dll, EntryPoint = "bae_add_release_next", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void AddReleaseNext(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    [DllImport(Dll, EntryPoint = "bae_add_to_queue", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr AddToQueuePtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string trackIdsJson);

    /// <summary>Append specific tracks to the end of the queue. Returns an error message, or null on success.</summary>
    internal static string? AddToQueue(IntPtr handle, IReadOnlyList<string> trackIds) =>
        ResultMessage(AddToQueuePtr(handle, JsonSerializer.Serialize(trackIds)));

    [DllImport(Dll, EntryPoint = "bae_add_next", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr AddNextPtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string trackIdsJson);

    /// <summary>Queue specific tracks to play next. Returns an error message, or null on success.</summary>
    internal static string? AddNext(IntPtr handle, IReadOnlyList<string> trackIds) =>
        ResultMessage(AddNextPtr(handle, JsonSerializer.Serialize(trackIds)));

    [DllImport(Dll, EntryPoint = "bae_delete_release", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr DeleteReleasePtr(
        IntPtr handle,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string releaseId);

    /// <summary>Delete a release; null on success, else the error message.</summary>
    internal static string? DeleteRelease(IntPtr handle, string releaseId) =>
        ResultMessage(DeleteReleasePtr(handle, releaseId));

    /// <summary>Persist playback state and stop playback before exit. Call before
    /// <see cref="HandleFree"/>.</summary>
    [DllImport(Dll, EntryPoint = "bae_shutdown", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void Shutdown(IntPtr handle);

    /// <summary>
    /// Release a handle created by <see cref="Init"/>.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_handle_free", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void HandleFree(IntPtr handle);

    /// <summary>
    /// Release a string returned by this library.
    /// </summary>
    [DllImport(Dll, EntryPoint = "bae_string_free", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void StringFree(IntPtr ptr);
}

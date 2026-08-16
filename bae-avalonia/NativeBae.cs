using System.Linq;
using System.Text.Json;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Windows bridge adapter. Methods backed by generated bindings call
/// <c>BaeBridgeMethods</c>; remaining members expose the JSON/string contract
/// used by the current Windows view models.
/// </summary>
internal static partial class NativeBae
{
    /// <summary>One-time startup: register the OS credential store. Takes the
    /// telemetry sink so a store-creation failure ships
    /// <c>keyring_init_failed</c>.</summary>
    internal static string? Startup(BridgeDiagnostics diagnostics) =>
        CaptureError(() => BaeBridgeMethods.InitKeyring(diagnostics));

    /// <summary>
    /// Construct the telemetry sink and install the core's tracing subscriber.
    /// Infallible: the core falls back to the no-op sink (with a local error
    /// log) rather than let telemetry setup block a launch.
    /// </summary>
    internal static BridgeDiagnostics ConfigureDiagnostics(BridgeDiagnosticsConfig config) =>
        BaeBridgeMethods.ConfigureDiagnostics(config);

    /// <summary>
    /// Build the Datadog telemetry config the sink is constructed from. Local
    /// logging stays in <see cref="BaeLogger"/>.
    /// </summary>
    internal static BridgeDiagnosticsConfig DiagnosticsConfig(
        string? datadogSite,
        string? clientToken,
        string source,
        string service,
        string? environment,
        string appVersion,
        string edition,
        string? gitCommit) =>
        datadogSite is not null && clientToken is not null
            ? new BridgeDiagnosticsConfig.Enabled(new BridgeDatadogDiagnosticsConfig(
                datadogSite,
                clientToken,
                source,
                new BridgeAppDiagnosticMetadata(
                    service,
                    environment ?? string.Empty,
                    appVersion,
                    edition,
                    gitCommit ?? string.Empty)))
            : new BridgeDiagnosticsConfig.Disabled();

    /// <summary>Flush buffered telemetry through the standalone sink.</summary>
    internal static System.Threading.Tasks.Task<string?> FlushDiagnostics(
        BridgeDiagnostics diagnostics) =>
        CaptureError(() => diagnostics.Flush());

    /// <summary>Report a host UI screen open as a typed telemetry event through
    /// the standalone sink. Infallible; the core owns every other event.</summary>
    internal static void ReportScreen(BridgeDiagnostics diagnostics, BridgeScreen screen) =>
        diagnostics.Event(new BridgeTelemetryEvent.ScreenOpened(screen));

#if BAE_FULL_BRIDGE
    internal static string? SetOauthClientCreds(string credsJson) =>
        CaptureError(() => BaeBridgeMethods.SetOauthClientCreds(credsJson));
#else
    internal static string? SetOauthClientCreds(string credsJson) => throw new InvalidOperationException();
#endif

    /// <summary>
    /// The cloud-provider wire tags this generated bridge build supports. Always
    /// includes <c>"s3"</c>; <c>"google_drive"</c>/<c>"dropbox"</c>/<c>"onedrive"</c>
    /// are present only when the bridge was built with the oauth-providers feature.
    /// The UI offers only these providers, so an S3-only build has no path to start
    /// an OAuth flow.
    /// </summary>
    internal static string[] AvailableCloudProviders() =>
        BaeBridgeMethods.AvailableCloudProviders().Select(CloudProviderTag).ToArray();

    internal static bool IsCloudProviderAvailable(BridgeCloudProvider provider) =>
        AvailableCloudProviders().Contains(CloudProviderTag(provider));

    /// <summary>Whether this build's native library supports any OAuth cloud provider.</summary>
    internal static bool SupportsOAuthProviders() =>
        AvailableCloudProviders().Any(provider => provider is "google_drive" or "dropbox" or "onedrive");

    private static string CloudProviderTag(BridgeCloudProvider provider) =>
        provider switch
        {
            BridgeCloudProvider.S3 => "s3",
            BridgeCloudProvider.GoogleDrive => "google_drive",
            BridgeCloudProvider.Dropbox => "dropbox",
            BridgeCloudProvider.OneDrive => "onedrive",
            BridgeCloudProvider.CloudKit => "cloudkit",
            _ => throw new ArgumentOutOfRangeException(nameof(provider), provider, "Unknown cloud provider"),
        };

#if BAE_FULL_BRIDGE
    private static BridgeCloudProvider CloudProvider(string provider) =>
        provider switch
        {
            "s3" => BridgeCloudProvider.S3,
            "google_drive" => BridgeCloudProvider.GoogleDrive,
            "dropbox" => BridgeCloudProvider.Dropbox,
            "onedrive" => BridgeCloudProvider.OneDrive,
            "cloudkit" => BridgeCloudProvider.CloudKit,
            _ => throw new ArgumentOutOfRangeException(nameof(provider), provider, "Unknown cloud provider"),
        };
#endif

    /// <summary>The catalog key for a cloud provider's display name, or null for
    /// the brand-name providers the UI passes through verbatim. <paramref name="provider"/>
    /// is the wire tag ("s3"/"google_drive"/…) or null/"" for local-only.</summary>
    internal static string? CloudProviderLabelKey(string? provider)
    {
        BridgeCloudProvider? bridgeProvider = provider switch
        {
            null or "" => null,
            "s3" => BridgeCloudProvider.S3,
            "google_drive" => BridgeCloudProvider.GoogleDrive,
            "dropbox" => BridgeCloudProvider.Dropbox,
            "onedrive" => BridgeCloudProvider.OneDrive,
            "cloudkit" => BridgeCloudProvider.CloudKit,
            _ => null,
        };
        if (bridgeProvider is null && provider is not null and not "")
        {
            return null;
        }
        return BaeBridgeMethods.BridgeCloudProviderLabelKey(bridgeProvider);
    }

    internal static string? CloudProviderLabelKey(BridgeCloudProvider provider) =>
        BaeBridgeMethods.BridgeCloudProviderLabelKey(provider);

    /// <summary>The catalog key for an import prepare-step wire tag, or null for
    /// an unknown tag.</summary>
    internal static string? PrepareStepKey(string step)
    {
        BridgePrepareStep? bridgeStep = step switch
        {
            "queued" => BridgePrepareStep.Queued,
            "reading_folder" => BridgePrepareStep.ReadingFolder,
            "parsing_metadata" => BridgePrepareStep.ParsingMetadata,
            "writing_cover_art" => BridgePrepareStep.WritingCoverArt,
            "discovering_files" => BridgePrepareStep.DiscoveringFiles,
            "validating_tracks" => BridgePrepareStep.ValidatingTracks,
            _ => null,
        };
        return bridgeStep is null ? null : BaeBridgeMethods.BridgePrepareStepKey(bridgeStep.Value);
    }

    /// <summary>The catalog key for an import-phase wire tag, or null for an
    /// unknown tag.</summary>
    internal static string? ImportPhaseKey(string phase)
    {
        BridgeImportPhase? bridgePhase = phase switch
        {
            "reading_files" => BridgeImportPhase.ReadingFiles,
            "measuring_loudness" => BridgeImportPhase.MeasuringLoudness,
            "finalizing" => BridgeImportPhase.Finalizing,
            _ => null,
        };
        return bridgePhase is null ? null : BaeBridgeMethods.BridgeImportPhaseKey(bridgePhase.Value);
    }

    /// <summary>The catalog key for a transfer action's progress verb (a wire tag
    /// from a storage row's actions), or null for an unknown tag.</summary>
    internal static string? TransferActionKey(string action)
    {
        BridgeReleaseStorageAction? bridgeAction = action switch
        {
            "pin" => BridgeReleaseStorageAction.Pin,
            "unpin" => BridgeReleaseStorageAction.Unpin,
            "manage" => BridgeReleaseStorageAction.MakeRemote,
            "unmanage" => BridgeReleaseStorageAction.MakeLocal,
            _ => null,
        };
        return bridgeAction is null ? null : BaeBridgeMethods.BridgeTransferActionKey(bridgeAction.Value);
    }

    /// <summary>The wire tag for a storage action ("pin"/"unpin"/"manage"/
    /// "unmanage") — the inverse of <see cref="TransferActionKey"/>, used to
    /// carry a transfer action into the transfer overlay, which is bridge-type
    /// free.</summary>
    internal static string TransferActionToken(BridgeReleaseStorageAction action) =>
        action switch
        {
            BridgeReleaseStorageAction.Pin => "pin",
            BridgeReleaseStorageAction.Unpin => "unpin",
            BridgeReleaseStorageAction.MakeRemote => "manage",
            BridgeReleaseStorageAction.MakeLocal => "unmanage",
            _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
        };

    /// <summary>The libraries discovered on this device.</summary>
    internal static List<BridgeLibrary> Libraries() =>
        BaeBridgeMethods.DiscoverLibraries()
            .ToList();

    /// <summary>Create a new library; returns its id.</summary>
    internal static string CreateLibrary() => BaeBridgeMethods.CreateLibrary(name: null).Id;

    /// <summary>
    /// Run the desktop OAuth flow for a provider (google_drive / dropbox / onedrive)
    /// and return the provider token JSON for <see cref="JoinFromCode"/>.
    /// The core opens the system browser and runs the 127.0.0.1 callback listener,
    /// so call off the UI thread.
    /// </summary>
#if BAE_FULL_BRIDGE
    internal static string OAuthAuthorize(string provider) =>
        BaeBridgeMethods.OauthAuthorize(CloudProvider(provider));

    internal static string OAuthAuthorize(BridgeCloudProvider provider) =>
        BaeBridgeMethods.OauthAuthorize(provider);
#else
    internal static string OAuthAuthorize(string provider) => throw new InvalidOperationException();

    internal static string OAuthAuthorize(BridgeCloudProvider provider) => throw new InvalidOperationException();
#endif

    /// <summary>
    /// This device's join-request code and the fingerprint it encodes, to hand
    /// to an existing member for approval. The joining device has no library yet,
    /// so this needs no handle; it only requires <see cref="Startup"/>.
    /// </summary>
    /// <param name="email">The OAuth account address the joiner authenticated as,
    /// baked into the code so the approver can share the OAuth folder to it; null
    /// for S3, which shares no folder.</param>
    internal static BridgeJoinRequest GenerateJoinRequest(string? email = null) =>
        BaeBridgeMethods.GenerateJoinRequest(email);

    /// <summary>
    /// Decode a join-request code for owner-side approval.
    /// </summary>
    internal static BridgeJoinRequestInfo DecodeJoinRequest(string code) =>
        BaeBridgeMethods.DecodeJoinRequest(code);

    /// <summary>
    /// The invite bundle behind what the joiner pasted or scanned. The invite is
    /// the owner's signed join offer — a byte payload, not a human-typed code —
    /// so it travels as base64 wherever it has to be text.
    /// </summary>
    internal static byte[] InviteBytes(string pasted)
    {
        try
        {
            return Convert.FromBase64String(pasted.Trim());
        }
        catch (FormatException)
        {
            throw new BridgeException.Diagnostic(
                new BridgeErrorCategory.Config(),
                "the pasted invite is not base64 an invite bundle could be read from");
        }
    }

    /// <summary>
    /// Decode a pasted invite for UI preview. Reads the same bytes the join runs
    /// on, so the preview and the join cannot disagree about the library.
    /// </summary>
    internal static BridgeInviteCodeInfo DecodeInviteCode(string pasted) =>
        BaeBridgeMethods.DecodeScannedInvite(InviteBytes(pasted));

    /// <summary>
    /// Join a shared library from a pasted invite and return its id.
    /// <paramref name="joinRequestCode"/> is the code this device's own
    /// <see cref="GenerateJoinRequest"/> minted — coven needs it back to promote
    /// that pending identity into the store's custody. For OAuth providers pass
    /// the token JSON from <see cref="OAuthAuthorize"/>; for credential providers
    /// pass null. Blocks on a cloud pull — call off the UI thread.
    /// </summary>
    internal static string JoinFromCode(string pasted, string joinRequestCode, string? oauthTokenJson) =>
        BaeBridgeMethods.JoinFromScannedInvite(InviteBytes(pasted), joinRequestCode, oauthTokenJson).Id;

    /// <summary>
    /// Decode a restore code for UI preview — which library it names, whose cloud
    /// home it points at, and whether that provider still needs an OAuth sign-in.
    /// </summary>
    internal static BridgeRestoreCodeInfo DecodeRestoreCode(string code) =>
        BaeBridgeMethods.DecodeRestoreCode(code);

    /// <summary>
    /// Restore a library from its restore code, returning the restored library id.
    ///
    /// The code carries everything the restore needs — the library, its cloud home,
    /// that home's credentials, and the encryption key — so there is nothing to
    /// enter by hand. OAuth tokens are the one exception: they expire, so an
    /// OAuth-backed provider re-authenticates and passes the token JSON here; a
    /// credential provider passes null. Blocks on a cloud pull — call off the UI
    /// thread.
    /// </summary>
    internal static string RestoreFromCode(string code, string? oauthTokenJson) =>
        BaeBridgeMethods.RestoreFromCode(code, oauthTokenJson).Id;

    internal sealed class UiEventSink(Action<BridgeUiEvent> onEvent) : UiEventCallback
    {
        public void OnEvent(BridgeUiEvent @event) => onEvent(@event);
    }

    internal static AppHandle? Init(
        string libraryId,
        uint positionUpdateIntervalMs,
        bool restorePlayback,
        BridgeDiagnostics diagnostics)
    {
        try
        {
            return BaeBridgeMethods.InitApp(libraryId, positionUpdateIntervalMs, restorePlayback, diagnostics);
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error($"library open failed: {exception.Message}");
            return null;
        }
    }

    internal static BridgeCloudHomeKeyState CloudHomeKeyState(AppHandle handle) =>
        handle.CloudHomeKeyState();

    internal static void HandleFree(AppHandle handle) => handle.Dispose();

    internal static void Subscribe(AppHandle handle, UiEventSink callback) =>
        handle.SubscribeUiEvents(callback);

    internal static string? LockActiveLibrary(AppHandle handle) =>
        CaptureError(() => Await(handle.LockActiveLibrary));

    internal static string? ForgetLibrary(AppHandle handle) =>
        CaptureError(() => Await(handle.ForgetLibrary));

    internal static string? UnlockCloudHome(AppHandle handle, string serializedMasterKey) =>
        CaptureError(() => Await(() => handle.UnlockCloudHome(serializedMasterKey)));

    internal static string? RenameLibrary(AppHandle handle, string libraryId, string name) =>
        CaptureError(() => handle.RenameLibrary(libraryId, name));

    internal static string? SetPrimaryRelease(AppHandle handle, string albumId, string releaseId) =>
        CaptureError(() => Await(() => handle.SetPrimaryRelease(albumId, releaseId)));

    internal static (BridgeRemoteCover[]? Covers, string? Error) FetchRemoteCovers(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() => Await(() => handle.FetchRemoteCovers(releaseId)));

    internal static string RemoteCoverThumbnailUrl(BridgeRemoteCover cover) =>
        CoverImageSourceUrl(cover.CoverChoice.ThumbnailSource);

    internal static BridgeCoverSelection RemoteCoverSelection(BridgeRemoteCover cover) =>
        cover.CoverChoice.Selection;

    internal static string? ChangeCover(AppHandle handle, string releaseId, BridgeCoverSelection selection) =>
        CaptureError(() => Await(() => handle.ChangeCover(releaseId, selection)));

    internal static LiveSubscription SubscribeAlbumPage(
        AppHandle handle,
        ulong offset,
        ulong limit,
        IReadOnlyList<SortCriterion<AlbumSortField>> criteria,
        Action<IReadOnlyList<Album>, int> onValue,
        Action<Exception> onError) =>
        handle.SubscribeAlbumPage(ToBridge(criteria), offset, limit, new AlbumPageSink(onValue, onError));

    internal static LiveSubscription SubscribeAlbumDetail(
        AppHandle handle,
        string albumId,
        Action<AlbumDetail?> onValue,
        Action<Exception> onError) =>
        handle.SubscribeAlbumDetail(albumId, new AlbumDetailSink(onValue, onError));

    // The 0-based position of an album under the active sort, matching
    // GetAlbumPage's ordering, or null when the album isn't present. Lets a
    // reveal page in and scroll to an album whose page may never have been
    // fetched. Value-typed result (Option<u64>), so it can't use
    // CaptureBridgeValue's reference-type contract — it mirrors
    // CloudOnlyReleaseCount's explicit capture instead.
    internal static (long? Index, string? Error) AlbumIndex(
        AppHandle handle, IReadOnlyList<SortCriterion<AlbumSortField>> criteria, string albumId)
    {
        try
        {
            var index = Await(() => handle.GetAlbumIndex(ToBridge(criteria), albumId));
            return (index is null ? null : checked((long)index.Value), null);
        }
        catch (BridgeException.Cancelled)
        {
            return (null, null);
        }
        catch (BridgeException exception)
        {
            return (null, exception.Message);
        }
    }

    internal static LiveSubscription SubscribeComposerPage(
        AppHandle handle,
        ulong offset,
        ulong limit,
        IReadOnlyList<SortCriterion<ComposerSortField>> criteria,
        Action<IReadOnlyList<ComposerSummary>, int> onValue,
        Action<Exception> onError) =>
        handle.SubscribeComposerPage(ToBridge(criteria), offset, limit, new ComposerPageSink(onValue, onError));

    internal static LiveSubscription SubscribeArtistPage(
        AppHandle handle,
        ulong offset,
        ulong limit,
        IReadOnlyList<SortCriterion<ArtistSortField>> criteria,
        Action<IReadOnlyList<ArtistSummary>, int> onValue,
        Action<Exception> onError) =>
        handle.SubscribeArtistPage(ToBridge(criteria), offset, limit, new ArtistPageSink(onValue, onError));

    private sealed class AlbumPageSink(
        Action<IReadOnlyList<Album>, int> onValue,
        Action<Exception> onError) : AlbumPageCallback
    {
        public void OnValue(BridgeAlbumPage value) =>
            onValue(value.Rows.Select(row => new Album(row)).ToList(), checked((int)value.TotalCount));
        public void OnError(BridgeException error) => onError(new PageLoadException(error.Message));
    }

    private sealed class AlbumDetailSink(
        Action<AlbumDetail?> onValue,
        Action<Exception> onError) : AlbumDetailCallback
    {
        public void OnValue(BridgeAlbumDetail? value) =>
            onValue(value is null ? null : new AlbumDetail(value));
        public void OnError(BridgeException error) => onError(error);
    }

    private sealed class ComposerPageSink(
        Action<IReadOnlyList<ComposerSummary>, int> onValue,
        Action<Exception> onError) : ComposerPageCallback
    {
        public void OnValue(BridgeComposerPage value) =>
            onValue(value.Rows.Select(row => new ComposerSummary(row)).ToList(), checked((int)value.TotalCount));
        public void OnError(BridgeException error) => onError(new PageLoadException(error.Message));
    }

    private sealed class ArtistPageSink(
        Action<IReadOnlyList<ArtistSummary>, int> onValue,
        Action<Exception> onError) : ArtistPageCallback
    {
        public void OnValue(BridgeArtistPage value) =>
            onValue(value.Rows.Select(row => new ArtistSummary(row)).ToList(), checked((int)value.TotalCount));
        public void OnError(BridgeException error) => onError(new PageLoadException(error.Message));
    }

    internal static LiveSubscription SubscribeStorage(
        AppHandle handle,
        StorageTab tab,
        StorageSortField field,
        SortDirection direction,
        ulong offset,
        ulong limit,
        Action<IReadOnlyList<BridgeStorageRow>, int, long> onValue,
        Action<Exception> onError) =>
        handle.SubscribeStorageProjection(
            new BridgeStorageSort(ToBridge(field), ToBridgeStorageDirection(direction)),
            ToBridge(tab),
            offset,
            limit,
            new StorageProjectionSink(onValue, onError));

    private sealed class StorageProjectionSink(
        Action<IReadOnlyList<BridgeStorageRow>, int, long> onValue,
        Action<Exception> onError) : StorageProjectionCallback
    {
        public void OnValue(BridgeStorageProjection value) =>
            onValue(value.Page.Rows, checked((int)value.Page.TotalCount), checked((long)value.TotalSize));
        public void OnError(BridgeException error) => onError(error);
    }

    private static BridgeStorageFilter ToBridge(StorageTab tab) => tab switch
    {
        StorageTab.All => BridgeStorageFilter.All,
        StorageTab.Cloud => BridgeStorageFilter.Remote,
        StorageTab.Local => BridgeStorageFilter.Local,
        StorageTab.Uploading => BridgeStorageFilter.Uploading,
        _ => throw new ArgumentOutOfRangeException(nameof(tab), tab, "Unknown storage tab"),
    };

    private static BridgeStorageSortField ToBridge(StorageSortField field) => field switch
    {
        StorageSortField.AlbumTitle => BridgeStorageSortField.AlbumTitle,
        StorageSortField.ArtistNames => BridgeStorageSortField.ArtistNames,
        StorageSortField.Format => BridgeStorageSortField.Format,
        StorageSortField.FileCount => BridgeStorageSortField.FileCount,
        StorageSortField.TotalSize => BridgeStorageSortField.TotalSize,
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown storage sort field"),
    };

    private static BridgeStorageSortDirection ToBridgeStorageDirection(SortDirection direction) => direction switch
    {
        SortDirection.Ascending => BridgeStorageSortDirection.Ascending,
        SortDirection.Descending => BridgeStorageSortDirection.Descending,
        _ => throw new ArgumentOutOfRangeException(nameof(direction), direction, "Unknown sort direction"),
    };


    // The album grid's bulk pin: the same enqueue as PinRelease, over every
    // targeted album's primary release.
    internal static string? PinReleases(AppHandle handle, IReadOnlyList<string> releaseIds) =>
        CaptureError(() => Await(() => handle.QueuePinReleases(releaseIds.ToArray())));

    internal static string? UnpinRelease(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(() => handle.UnpinRelease(releaseId)));

    internal static string? MakeReleaseRemote(AppHandle handle, string releaseId, bool pin) =>
        CaptureError(() => Await(() => handle.MakeReleaseRemote(releaseId, pin)));

    internal static string? MakeReleaseLocal(AppHandle handle, string releaseId, string newPath) =>
        CaptureError(() => Await(() => handle.MakeReleaseLocal(releaseId, newPath)));

    internal static (BridgeOutboxSnapshot? Snapshot, string? Error) OutboxSnapshot(AppHandle handle) =>
        CaptureBridgeValue(() => Await(() => handle.GetOutboxSnapshot()));

    internal static (BridgeDownloadSnapshot? Snapshot, string? Error) DownloadSnapshot(AppHandle handle) =>
        CaptureBridgeValue(handle.GetDownloadSnapshot);

    internal static (BridgeSyncStatusSnapshot? Status, string? Error) SyncStatus(AppHandle handle) =>
        CaptureBridgeValue(handle.GetSyncStatus);

    internal static void SetDownloadsPaused(AppHandle handle, bool paused) => handle.SetDownloadsPaused(paused);

    internal static void RetryDownloads(AppHandle handle) => handle.RetryDownloads();

    /// <summary>Cancel a release's download — drops a queued/failed entry or aborts
    /// the in-flight one (the release stays cloud-only).</summary>
    internal static void CancelDownload(AppHandle handle, string releaseId) => handle.CancelDownload(releaseId);

    internal static BridgeOutputSnapshot OutputSnapshot(AppHandle handle) =>
        handle.GetOutputSnapshot();

    internal static void SetOutputsPaused(AppHandle handle, bool paused) =>
        handle.SetOutputsPaused(paused);

    internal static void CancelOutput(AppHandle handle, string releaseId) =>
        handle.CancelOutput(releaseId);

    internal static void RetryOutputs(AppHandle handle) => handle.RetryOutputs();

    internal static string? RetryOutbox(AppHandle handle) => CaptureError(() => Await(() => handle.RetryOutbox()));

    internal static string? SetSyncPaused(AppHandle handle, bool paused) =>
        CaptureError(() => Await(() => handle.SetSyncPaused(paused)));

    internal static string? CancelReleaseTransition(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(() => handle.CancelReleaseTransition(releaseId)));

    internal static Settings GetSettings(AppHandle handle)
    {
        return Settings(
            handle.GetConfig(),
            Await(handle.GetMcpServerStatus),
            Await(handle.GetSubsonicServerStatus));
    }

    internal static Settings SettingsFromConfig(AppHandle handle, BridgeConfig config) =>
        Settings(
            config,
            Await(handle.GetMcpServerStatus),
            Await(handle.GetSubsonicServerStatus));

    internal static string? SetPauseBetweenSides(AppHandle handle, bool enabled) =>
        CaptureError(() => handle.SetPauseBetweenSides(enabled));

    internal static BridgeConfig GetConfig(AppHandle handle) => handle.GetConfig();

    internal static string? SetMaxConcurrentUploads(AppHandle handle, uint n) =>
        CaptureError(() => handle.SetMaxConcurrentUploads(n));

    internal static string? SetMaxConcurrentDownloads(AppHandle handle, uint n) =>
        CaptureError(() => handle.SetMaxConcurrentDownloads(n));

    /// <summary>Whether the seek bar's leading label counts down the time remaining.
    /// A synced preference: the config subscription re-renders the bar after the
    /// write.</summary>
    internal static string? SetShowRemainingTime(AppHandle handle, bool enabled) =>
        CaptureError(() => handle.SetShowRemainingTime(enabled));

    /// <summary>Whether the library page spans the window's full width. A synced
    /// preference; the config subscription re-renders the page through the
    /// settings mirror after the write.</summary>
    internal static string? SetLibraryFullWidth(AppHandle handle, bool enabled) =>
        CaptureError(() => handle.SetLibraryFullWidth(enabled));

    internal static string? SetSavePresets(AppHandle handle, IEnumerable<SavePreset> presets) =>
        CaptureError(() => handle.SetSavePresets(
            presets.Select(SavePresetBridge).ToArray()));

    internal static string? SetDefaultTrackSavePreset(AppHandle handle, string presetId) =>
        CaptureError(() => handle.SetDefaultTrackSavePreset(presetId));

    internal static string? SetDefaultReleaseSavePreset(AppHandle handle, string presetId) =>
        CaptureError(() => handle.SetDefaultReleaseSavePreset(presetId));

    internal static string? SetMcpServerConfig(AppHandle handle, bool enabled, ushort port) =>
        CaptureError(() => Await(() => handle.SetMcpServerConfig(enabled, port)));

    internal static BridgeMcpServerStatus McpServerStatus(AppHandle handle) =>
        Await(handle.GetMcpServerStatus);

    internal static string? GetMcpToken(AppHandle handle) => CaptureValue(handle.GetMcpToken);

    internal static string? GenerateMcpToken(AppHandle handle) => CaptureValue(handle.GenerateMcpToken);

    internal static string? SetMcpToken(AppHandle handle, string token) =>
        CaptureError(() => handle.SetMcpToken(token));

    internal static string? SetSubsonicServerConfig(AppHandle handle, bool enabled, ushort port, string username, string bindAddress) =>
        CaptureError(() => Await(() => handle.SetSubsonicServerConfig(enabled, port, username, bindAddress)));

    internal static BridgeSubsonicServerStatus SubsonicServerStatus(AppHandle handle) =>
        Await(handle.GetSubsonicServerStatus);

    internal static string? SetSubsonicPassword(AppHandle handle, string password) =>
        CaptureError(() => Await(() => handle.SetSubsonicPassword(password)));

    internal static string? SaveDiscogsToken(AppHandle handle, string token) =>
        CaptureValue(() => DiscogsSaveOutcomeTag(Await(() => handle.SaveDiscogsToken(token))));

    internal static string? RevalidateDiscogsToken(AppHandle handle) =>
        CaptureError(() => Await(() => handle.RevalidateDiscogsToken()));

    internal static string? DeleteDiscogsToken(AppHandle handle) =>
        CaptureError(handle.RemoveDiscogsToken);

    internal static string? SaveSyncConfig(
        AppHandle handle, string bucket, string region, string endpoint,
        string keyPrefix, string accessKey, string secretKey, string storage) =>
        CaptureError(() => Await(() => handle.SaveSyncConfig(new BridgeSaveSyncConfig(
            bucket,
            region,
            string.IsNullOrWhiteSpace(endpoint) ? null : endpoint.Trim(),
            string.IsNullOrWhiteSpace(keyPrefix) ? null : keyPrefix.Trim(),
            accessKey,
            secretKey,
            HomeStorage(storage)))));

    /// <summary>How many releases live only in the cloud and would become
    /// unplayable if this device disconnected; 0 means nothing is at risk. The
    /// caller renders the warning sentence from the count with its own locale's
    /// plural rules. A null count with a message means the check itself failed.</summary>
    internal static (long? Count, string? Error) CloudOnlyReleaseCount(AppHandle handle)
    {
        try
        {
            return (checked((long)Await(() => handle.CloudOnlyReleaseCount())), null);
        }
        catch (BridgeException.Cancelled)
        {
            return (null, null);
        }
        catch (BridgeException exception)
        {
            return (null, exception.Message);
        }
    }

#if BAE_FULL_BRIDGE
    internal static string? SignInCloud(AppHandle handle, string provider, string storage) =>
        CaptureError(() => Await(() => handle.SignInCloudProvider(CloudProvider(provider), HomeStorage(storage))));
#else
    internal static string? SignInCloud(AppHandle handle, string provider, string storage) => throw new InvalidOperationException();
#endif

    internal static string? DisconnectCloud(AppHandle handle) =>
        CaptureError(() => Await(handle.DisconnectCloudProvider));

    internal static void TriggerSync(AppHandle handle) => handle.TriggerSync();

    internal static string? GenerateRestoreCode(AppHandle handle) =>
        CaptureValue(() => Await(() => handle.GenerateRestoreCode()));

    internal static (BridgeMembership? Membership, string? Error) GetMembers(AppHandle handle) =>
        CaptureBridgeValue(() => Await(() => handle.GetMembers()));

    internal static string? InviteMember(AppHandle handle, string publicKeyHex) =>
        CaptureValue(() => Await(() => handle.InviteMember(publicKeyHex, providerAccountEmail: null)));

    /// <summary>Approve a joining device by its public key, baking in the OAuth
    /// account address to share the provider folder to; returns the invite code to
    /// hand back, or the error line.</summary>
    internal static string? InviteMember(AppHandle handle, string publicKeyHex, string? providerAccountEmail) =>
        CaptureValue(() => Await(() => handle.InviteMember(publicKeyHex, providerAccountEmail)));

    internal static string? RemoveMember(AppHandle handle, string publicKeyHex) =>
        CaptureError(() => Await(() => handle.RemoveMember(publicKeyHex)));

    internal static (BridgeRawReleaseEdit? Edit, string? Error) ReleaseEditSeed(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() => Await(() => handle.SeedReleaseEdit(releaseId)));

    internal static (BridgeRawReleaseEdit? Edit, string? Error) ResetMetadataToSource(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() => BaeBridgeMethods.RawReleaseEditFromUserEdit(
            Await(() => handle.ResetMetadataToSource(releaseId)),
            "reset-track"));

    internal static string? ApplyReleaseEdit(AppHandle handle, string releaseId, BridgeRawReleaseEdit edit) =>
        CaptureError(() => Await(() => handle.UpdateReleaseMetadataUserEdit(releaseId, ReleaseUserEdit(edit))));

    internal static (List<ReleaseCandidateChoice>? Candidates, string? Error) SearchReleases(
        AppHandle handle,
        string source,
        string artist,
        string album) =>
        CaptureBridgeValue(() => CandidateChoices(Await(() => handle.SearchForCandidate(
            new BridgeSearchQuery.General(artist, album, MetadataSource(source))))));

    internal static string? ReidentifyRelease(AppHandle handle, string releaseId, BridgeIdentityChoice choice) =>
        CaptureError(() => Await(() => handle.ReIdentifyRelease(releaseId, choice)));

    // What a candidate's track sheet may be bound to: the folder's audio, each
    // already offered or refused with core's own reason. Core probes to decide,
    // so this is asked for when the picker opens rather than carried on the row.
    internal static (List<ImportSheetBindingOption>? Options, string? Error) SheetBindingOptions(
        AppHandle handle,
        string candidateKey,
        string sheetFileId) =>
        CaptureBridgeValue(() => Await(() => handle.SheetBindingOptions(candidateKey, sheetFileId))
            .Select(option => new ImportSheetBindingOption
            {
                FileId = option.FileId,
                RefusalReason = BridgeDisplay.RefusalLine(option.Offer),
            })
            .ToList());

    internal static string? SetSheetBinding(
        AppHandle handle,
        string candidateKey,
        string sheetFileId,
        string? audioFileId) =>
        CaptureError(() => Await(() => handle.SetSheetBinding(candidateKey, sheetFileId, audioFileId)));

    /// <summary>Say which disc of the release one of a candidate's track sheets
    /// holds, or take it out of the tracklist. Cue filenames are arbitrary, so
    /// the assignment is the truth about which cue is which disc; core persists
    /// it and clears the candidate's stored identify verdict, because a
    /// re-assigned sheet is a different tracklist.</summary>
    internal static string? SetSheetDisc(
        AppHandle handle,
        string candidateKey,
        string sheetFileId,
        BridgeSheetDisc disc) =>
        CaptureError(() => Await(() => handle.SetSheetDisc(candidateKey, sheetFileId, disc)));

    /// <summary>Put one of a candidate's files in a role, or put it back in the
    /// one the scan proposed. Core persists it — taking a file out of the
    /// tracklist is a fact about the folder, not an edit to whichever pane is
    /// open — and clears the candidate's stored identify verdict.</summary>
    internal static string? SetFileRole(
        AppHandle handle,
        string candidateKey,
        string fileId,
        BridgeFileRoleChoice choice) =>
        CaptureError(() => Await(() => handle.SetFileRole(candidateKey, fileId, choice)));

    internal static string? ScanFolder(AppHandle handle, string path, bool clearFirst) =>
        CaptureError(() => Await(() => handle.AddWatchedFolder(path)));

    internal static string? RemoveWatchedFolder(AppHandle handle, string path) =>
        CaptureError(() => Await(() => handle.RemoveWatchedFolder(path)));

    internal static string? RefreshWatchedFolder(AppHandle handle, string path) =>
        CaptureError(() => Await(() => handle.RefreshWatchedFolder(path)));

    internal static string? SetFolderReleaseDecision(
        AppHandle handle,
        BridgeFolderReleaseDecisionKey key,
        BridgeFolderReleaseDecision decision) =>
        CaptureError(() => Await(() => handle.SetFolderReleaseDecision(key, decision)));

    internal static string? SetCandidateSkipped(AppHandle handle, string path, bool skipped) =>
        CaptureError(() => Await(() => handle.SetCandidateSkipped(path, skipped)));

    /// <summary>A text file's decoded contents (bridge-side encoding detection),
    /// or the error line. No session handle: the read is a free bridge call.</summary>
    internal static (string? Text, string? Error) ReadTextFile(string path) =>
        CaptureBridgeValue(() => BaeBridgeMethods.ReadTextFile(path));

    internal static void AutoIdentifyFolder(AppHandle handle, string candidateKey) =>
        handle.AutoIdentifyFolder(candidateKey);

    internal static void AutoIdentifyRelease(AppHandle handle, string candidateKey, string releaseId) =>
        handle.AutoIdentifyRelease(candidateKey, releaseId);

    internal static void CancelAutoIdentify(AppHandle handle, string candidateKey) =>
        handle.CancelAutoIdentify(candidateKey);

    /// <summary>
    /// Reseed a release's metadata from its (just re-pointed) metadata source:
    /// re-project via reset, then write the projection back. Identity rows are
    /// untouched; the user's prior edits are overwritten by design.
    /// </summary>
    internal static string? RefreshMetadataFromSource(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(async () => await handle.UpdateReleaseMetadataUserEdit(
            releaseId, await handle.ResetMetadataToSource(releaseId))));

    internal static void ToggleSignalForCandidate(AppHandle handle, string candidateKey, string kind, string value) =>
        handle.ToggleSignalForCandidate(candidateKey, ExcludedSignal(kind, value));

    internal static void RerunIdentifyForCandidate(AppHandle handle, string candidateKey) =>
        handle.RerunIdentifyForCandidate(candidateKey);

    internal static void PreviewPlay(AppHandle handle, string path) => handle.PreviewPlay(path);

    internal static void PreviewStop(AppHandle handle) => handle.PreviewStop();

    internal static void PreviewTogglePause(AppHandle handle) => handle.PreviewTogglePause();

    /// <summary>Decide the candidate's identity: persist the choice and come
    /// back with the seeded edit — the same payload the selection query
    /// serves, so a fresh launch renders exactly what the click rendered.</summary>
    internal static (DecidedEdit? Decided, string? Error) PickCandidateIdentity(
        AppHandle handle,
        string candidateKey,
        BridgeIdentityPick pick,
        IReadOnlyList<LocalArtwork> localArtwork) =>
        CaptureBridgeValue(() =>
            DecidedEdit(
                Await(() => handle.PickCandidateIdentity(candidateKey, pick)),
                localArtwork));

    /// <summary>The candidate's decided identity read back, or null while
    /// nothing is decided — what selecting a row asks.</summary>
    internal static (DecidedEdit? Decided, bool Undecided, string? Error) CandidateDecidedIdentity(
        AppHandle handle,
        string candidateKey,
        IReadOnlyList<LocalArtwork> localArtwork)
    {
        var (value, error) = CaptureBridgeValue(() =>
        {
            var answer = Await(() => handle.CandidateDecidedIdentity(candidateKey));
            return answer is null ? null : DecidedEdit(answer, localArtwork);
        });
        return (value, value is null && error is null, error);
    }

    /// <summary>Map either identity's answer onto the pane's edit shape; the
    /// discriminator rides along so the pane knows which side it seeds.</summary>
    private static DecidedEdit DecidedEdit(
        BridgeDecidedIdentity answer,
        IReadOnlyList<LocalArtwork> localArtwork) => answer switch
        {
            BridgeDecidedIdentity.Release release => new DecidedEdit
            {
                Release = (
                    release.Source,
                    release.ReleaseId,
                    release.Prefetch.Detail.SourceGroupId,
                    release.Prefetch.Claim.Level),
                Edit = new PrefetchedEdit
                {
                    Edit = BaeBridgeMethods.RawReleaseEditFromUserEdit(release.Prefetch.Seed, "prefetch-track"),
                    RemoteCovers = release.Prefetch.Detail.CoverArt.ToList(),
                    LocalArtwork = localArtwork.ToList(),
                    Claim = release.Prefetch.Claim,
                    ExactPressing = release.Prefetch.ExactPressing,
                    Mapping = release.Prefetch.Mapping,
                },
            },
            BridgeDecidedIdentity.Unknown unknown => new DecidedEdit
            {
                Release = null,
                Edit = new PrefetchedEdit
                {
                    Edit = BaeBridgeMethods.RawReleaseEditFromUserEdit(unknown.Seed, "unknown-track"),
                    Mapping = unknown.Mapping,
                    LocalArtwork = localArtwork.ToList(),
                },
            },
            _ => throw new ArgumentOutOfRangeException(nameof(answer), answer, "Unknown decided identity"),
        };

    /// <summary>What picking a release under a candidate claims, and where its
    /// metadata comes from. The re-identify dialog's path: it commits straight
    /// from the picked row, so it never prefetches.</summary>
    internal static BridgeClaimLine ClaimForPick(
        AppHandle handle, string candidateKey, BridgeMetadataResult result, BridgeClaimLevel level) =>
        handle.ClaimForPick(candidateKey, result, level);

    /// <summary>The mapping table for a candidate nobody has picked a release
    /// for: every source unit the folder offers, with what it becomes left
    /// open.</summary>
    internal static (BridgeMappingTable? Mapping, string? Error) CandidateMapping(
        AppHandle handle, string candidateKey) =>
        CaptureBridgeValue(() => handle.CandidateMapping(candidateKey));

    internal static LiveSubscription SubscribeReleaseLibraryStatus(
        AppHandle handle,
        BridgeMetadataSource source,
        string releaseId,
        string? sourceGroupId,
        Action<BridgeLibraryStatus> onValue,
        Action<Exception> onError) =>
        handle.SubscribeReleaseLibraryStatus(
            source,
            releaseId,
            sourceGroupId,
            new ReleaseLibraryStatusSink(onValue, onError));

    private sealed class ReleaseLibraryStatusSink(
        Action<BridgeLibraryStatus> onValue,
        Action<Exception> onError) : ReleaseLibraryStatusCallback
    {
        public void OnValue(BridgeLibraryStatus value) => onValue(value);
        public void OnError(BridgeException error) => onError(error);
    }

    internal static string? ImportCandidate(
        AppHandle handle, string candidateKey, BridgeIdentityChoice identity, string storageMode, bool pin, BridgeRawReleaseEdit userEdit, BridgeCoverSelection? selectedCover) =>
        CaptureError(() => Await(() => handle.StartImport(
            candidateKey,
            selectedCover,
            StorageMode(storageMode),
            pin,
            identity,
            ReleaseUserEdit(userEdit))));

    /// <summary>Provider art at a URL for the import flow's cover search — its
    /// bytes and the validator identifying them — or null when the source
    /// serves no image there, and on a failed fetch (logged). Core owns the
    /// socket; the UI never opens one.</summary>
    internal static BridgeRemoteImage? RemoteImage(AppHandle? handle, string url) =>
        handle is null ? null : Capture(() => Await(() => handle.FetchRemoteImageBytes(url)));

    internal static byte[]? LibraryImageBytes(AppHandle? handle, BridgeImageRef image) =>
        handle is null ? null : Capture(() => Await(() => handle.FetchLibraryImageBytes(image)));

    internal static byte[]? ReleaseImageBytes(AppHandle? handle, string releaseId, BridgeGallerySource source) =>
        handle is null ? null : Capture(() => Await(() => handle.FetchReleaseImageBytes(releaseId, source)));

}

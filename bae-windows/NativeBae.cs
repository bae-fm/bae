using System.Text.Json;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// Windows bridge adapter. Methods backed by generated bindings call
/// <c>BaeBridgeMethods</c>; remaining members expose the JSON/string contract
/// used by the current Windows view models.
/// </summary>
internal static class NativeBae
{
    /// <summary>One-time startup: register the OS credential store. Takes the
    /// telemetry sink so a store-creation failure ships
    /// <c>keyring_init_failed</c>.</summary>
    internal static void Startup(BridgeDiagnostics diagnostics) =>
        BaeBridgeMethods.InitKeyring(diagnostics);

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
    internal static string? FlushDiagnostics(BridgeDiagnostics diagnostics) =>
        CaptureError(() => Await(diagnostics.Flush()));

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
            "parsing_metadata" => BridgePrepareStep.ParsingMetadata,
            "writing_cover_art" => BridgePrepareStep.WritingCoverArt,
            "discovering_files" => BridgePrepareStep.DiscoveringFiles,
            "validating_tracks" => BridgePrepareStep.ValidatingTracks,
            "saving_to_database" => BridgePrepareStep.SavingToDatabase,
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
            "referencing_files" => BridgeImportPhase.ReferencingFiles,
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
    /// and return the provider token JSON for <see cref="RestoreFromCode"/>.
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

    /// <summary>Decode a restore code for UI preview.</summary>
    internal static BridgeRestoreCodeInfo DecodeRestoreCode(string code) =>
        BaeBridgeMethods.DecodeRestoreCode(code);

    /// <summary>
    /// Restore a library from a code and return its id. For OAuth providers pass
    /// the token JSON from <see cref="OAuthAuthorize"/>; for credential providers
    /// pass null. Blocks on a cloud pull — call off the UI thread.
    /// </summary>
    internal static string RestoreFromCode(string code, string? oauthTokenJson) =>
        BaeBridgeMethods.RestoreFromCode(code, oauthTokenJson).Id;

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
    /// Decode an invite code for UI preview.
    /// </summary>
    internal static BridgeInviteCodeInfo DecodeInviteCode(string code) =>
        BaeBridgeMethods.DecodeInviteCode(code);

    /// <summary>
    /// Join a shared library from an invite code and return its id.
    /// <paramref name="joinRequestCode"/> is the code this device's own
    /// <see cref="GenerateJoinRequest"/> minted — coven needs it back to promote
    /// that pending identity into the store's custody. For OAuth providers pass
    /// the token JSON from <see cref="OAuthAuthorize"/>; for credential providers
    /// pass null. Blocks on a cloud pull — call off the UI thread.
    /// </summary>
    internal static string JoinFromCode(string code, string joinRequestCode, string? oauthTokenJson) =>
        BaeBridgeMethods.JoinFromCode(code, joinRequestCode, oauthTokenJson).Id;

    /// <summary>
    /// Restore an S3-backed library by entering its cloud location and credentials
    /// directly, returning the restored library id. An empty
    /// <paramref name="libraryName"/> generates one. Blocks on a cloud pull — call
    /// off the UI thread.
    /// </summary>
    internal static string RestoreFromS3(
        string libraryId,
        string encryptionKeyHex,
        string? libraryName,
        string bucket,
        string region,
        string? endpoint,
        string accessKey,
        string secretKey)
    {
        var home = new BridgeRestoreHome.S3(
            bucket,
            region,
            string.IsNullOrWhiteSpace(endpoint) ? null : endpoint.Trim(),
            accessKey,
            secretKey);
        var config = new BridgeRestoreConfig(libraryId, encryptionKeyHex, home);
        return BaeBridgeMethods.RestoreFromCloud(libraryName, config).Id;
    }

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

    internal static bool HasEncryptionKey(AppHandle handle) => handle.HasEncryptionKey();

    internal static void HandleFree(AppHandle handle) => handle.Dispose();

    internal static void Subscribe(AppHandle handle, UiEventSink callback) =>
        handle.SubscribeUiEvents(callback);

    internal static string? LockActiveLibrary(AppHandle handle) =>
        CaptureError(() => handle.LockActiveLibrary());

    internal static string? ForgetLibrary(AppHandle handle) =>
        CaptureError(() => handle.ForgetLibrary());

    internal static string? UnlockLibrary(string libraryId, string keyHex) =>
        CaptureError(() => BaeBridgeMethods.UnlockLibrary(libraryId, keyHex));

    internal static string? RenameLibrary(AppHandle handle, string libraryId, string name) =>
        CaptureError(() => handle.RenameLibrary(libraryId, name));

    internal static string? SetPrimaryRelease(AppHandle handle, string albumId, string releaseId) =>
        CaptureError(() => Await(handle.SetPrimaryRelease(albumId, releaseId)));

    internal static string? SaveTrack(AppHandle handle, string trackId, string outputPath, string presetId) =>
        CaptureError(() => Await(handle.SaveTrack(trackId, outputPath, presetId)));

    internal static string? ExportRelease(AppHandle handle, string releaseId, string targetDir) =>
        CaptureError(() => Await(handle.EnqueueExport(releaseId, targetDir)));

    internal static string? SaveRelease(AppHandle handle, string releaseId, string targetDir, string presetId) =>
        CaptureError(() => Await(handle.EnqueueReleaseSave(releaseId, targetDir, presetId)));

    internal static string? SaveTrackSuggestedName(AppHandle handle, string trackId, string presetId) =>
        CaptureValue(() => Await(handle.SaveTrackSuggestedName(trackId, presetId)));

    internal static (BridgeFile[]? Images, string? Error) GetReleaseImages(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.FindReleaseDetail(releaseId));
            return detail?.ImageFiles.Where(file => file.IsImage).ToArray() ?? [];
        });

    internal static (BridgeRemoteCover[]? Covers, string? Error) FetchRemoteCovers(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() => Await(handle.FetchRemoteCovers(releaseId)));

    internal static string RemoteCoverThumbnailUrl(BridgeRemoteCover cover) =>
        CoverImageSourceUrl(cover.CoverChoice.ThumbnailSource);

    internal static BridgeCoverSelection RemoteCoverSelection(BridgeRemoteCover cover) =>
        cover.CoverChoice.Selection;

    internal static string? ChangeCover(AppHandle handle, string albumId, string releaseId, BridgeCoverSelection selection) =>
        CaptureError(() => Await(handle.ChangeCover(albumId, releaseId, selection)));

    internal static long AlbumCount(AppHandle handle) =>
        checked((long)Await(handle.GetAlbumCount()));

    internal static (List<Album>? Albums, string? Error) AlbumPage(AppHandle handle, ulong offset, ulong limit, IReadOnlyList<SortCriterion<AlbumSortField>> criteria) =>
        CaptureBridgeValue(() => Await(handle.GetAlbumPage(ToBridge(criteria), offset, limit))
            .Select(album => new Album(album))
            .ToList());

    internal static long ComposerCount(AppHandle handle) =>
        checked((long)Await(handle.GetComposerCount()));

    internal static (List<ComposerSummary>? Composers, string? Error) ComposerPage(AppHandle handle, ulong offset, ulong limit, IReadOnlyList<SortCriterion<ComposerSortField>> criteria) =>
        CaptureBridgeValue(() => Await(handle.GetComposerPage(ToBridge(criteria), offset, limit))
            .Select(composer => new ComposerSummary(composer))
            .ToList());

    internal static long ArtistCount(AppHandle handle) =>
        checked((long)Await(handle.GetArtistCount()));

    internal static (List<ArtistSummary>? Artists, string? Error) ArtistPage(AppHandle handle, ulong offset, ulong limit, IReadOnlyList<SortCriterion<ArtistSortField>> criteria) =>
        CaptureBridgeValue(() => Await(handle.GetArtistPage(ToBridge(criteria), offset, limit))
            .Select(artist => new ArtistSummary(artist))
            .ToList());

    internal static (BridgeGalleryItem[]? Items, string? Error) Gallery(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.FindReleaseDetail(releaseId));
            return detail?.GalleryItems ?? [];
        });

    // Total releases matching a storage tab, for the dialog's incremental
    // loading collection to know when it has everything.
    internal static long StorageCount(AppHandle handle, StorageTab tab) =>
        checked((long)Await(handle.StorageCount(ToBridge(tab))));

    // Sum of file sizes over every release matching a storage tab — the
    // dialog's footer "Total:" figure, independent of how many pages of the
    // tab's list have loaded.
    internal static long StorageTotalSize(AppHandle handle, StorageTab tab) =>
        checked((long)Await(handle.StorageTotalSize(ToBridge(tab))));

    // One page of storage rows for a tab, sorted server-side by the active
    // column — the dialog's incremental collection calls this per page instead
    // of fetching the whole library at once.
    internal static (BridgeStoragePage? Page, string? Error) StoragePage(
        AppHandle handle, StorageTab tab, StorageSortField field, SortDirection direction, ulong offset, ulong limit) =>
        CaptureBridgeValue(() => Await(handle.StoragePage(
            new BridgeStorageSort(ToBridge(field), ToBridgeStorageDirection(direction)),
            ToBridge(tab),
            offset,
            limit)));

    private static BridgeStorageFilter ToBridge(StorageTab tab) => tab switch
    {
        StorageTab.All => BridgeStorageFilter.All,
        // "Managed"/"Unmanaged" in this UI's vocabulary map onto the wire's
        // Remote/Local storage state, not a "managed by an import" concept.
        StorageTab.Managed => BridgeStorageFilter.Remote,
        StorageTab.Unmanaged => BridgeStorageFilter.Local,
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

    /// <summary>A single release's live storage fields (state, pin, available
    /// actions, in-flight transfer), for refreshing the album-detail storage
    /// band. Null when the release is gone.</summary>
    internal static (Release? Release, string? Error) ReleaseStorage(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() =>
        {
            var release = Await(handle.FindReleaseDetail(releaseId));
            return release is null ? null : new Release(release);
        });

    internal static string? PinRelease(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(handle.QueuePinReleases([releaseId])));

    // The album grid's bulk pin: the same enqueue as PinRelease, over every
    // targeted album's primary release.
    internal static string? PinReleases(AppHandle handle, IReadOnlyList<string> releaseIds) =>
        CaptureError(() => Await(handle.QueuePinReleases(releaseIds.ToArray())));

    internal static string? UnpinRelease(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(handle.UnpinRelease(releaseId)));

    internal static string? MakeReleaseRemote(AppHandle handle, string releaseId, bool pin) =>
        CaptureError(() => Await(handle.MakeReleaseRemote(releaseId, pin)));

    internal static string? MakeReleaseLocal(AppHandle handle, string releaseId, string newPath) =>
        CaptureError(() => Await(handle.MakeReleaseLocal(releaseId, newPath)));

    internal static (BridgeOutboxSnapshot? Snapshot, string? Error) OutboxSnapshot(AppHandle handle) =>
        CaptureBridgeValue(() => Await(handle.GetOutboxSnapshot()));

    internal static (BridgeDownloadSnapshot? Snapshot, string? Error) DownloadSnapshot(AppHandle handle) =>
        CaptureBridgeValue(handle.GetDownloadSnapshot);

    internal static (BridgeSyncStatusSnapshot? Status, string? Error) SyncStatus(AppHandle handle) =>
        CaptureBridgeValue(handle.GetSyncStatus);

    internal static void SetDownloadsPaused(AppHandle handle, bool paused) => handle.SetDownloadsPaused(paused);

    internal static void RetryDownloads(AppHandle handle) => handle.RetryDownloads();

    internal static BridgeOutputSnapshot OutputSnapshot(AppHandle handle) =>
        handle.GetOutputSnapshot();

    internal static void SetOutputsPaused(AppHandle handle, bool paused) =>
        handle.SetOutputsPaused(paused);

    internal static void CancelOutput(AppHandle handle, string releaseId) =>
        handle.CancelOutput(releaseId);

    internal static void RetryOutputs(AppHandle handle) => handle.RetryOutputs();

    internal static string? RetryOutbox(AppHandle handle) => CaptureError(() => Await(handle.RetryOutbox()));

    internal static string? SetSyncPaused(AppHandle handle, bool paused) =>
        CaptureError(() => Await(handle.SetSyncPaused(paused)));

    internal static string? CancelOutboxItem(AppHandle handle, long id) =>
        CaptureError(() => Await(handle.CancelOutboxItem(id)));

    internal static string? CancelReleaseTransition(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(handle.CancelReleaseTransition(releaseId)));

    internal static (LibrarySearchResults? Results, string? Error) Search(AppHandle handle, string query) =>
        CaptureBridgeValue(() => new LibrarySearchResults(Await(handle.SearchLibrary(query))));

    internal static (AlbumDetail? Detail, string? Error) GetAlbumDetail(AppHandle handle, string albumId) =>
        CaptureBridgeValue(() => new AlbumDetail(Await(handle.GetAlbumDetail(albumId))));

    internal static (ComposerDetail? Detail, string? Error) GetComposerDetail(AppHandle handle, string artistId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.GetComposerDetail(artistId));
            return detail is null ? null : new ComposerDetail(detail);
        });

    internal static (ArtistDetail? Detail, string? Error) GetArtistDetail(AppHandle handle, string artistId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.GetArtistDetail(artistId));
            return detail is null ? null : new ArtistDetail(detail);
        });

    internal static (WorkDetail? Detail, string? Error) GetWorkDetail(AppHandle handle, string workId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.GetWorkDetail(workId));
            return detail is null ? null : new WorkDetail(detail);
        });

    internal static Settings GetSettings(AppHandle handle) =>
        Settings(handle.GetConfig(), handle.GetMcpServerStatus(), handle.GetSyncStatus());

    internal static string? SetPauseBetweenSides(AppHandle handle, bool enabled) =>
        CaptureError(() => handle.SetPauseBetweenSides(enabled));

    internal static BridgeConfig GetConfig(AppHandle handle) => handle.GetConfig();

    internal static string? SetMaxConcurrentUploads(AppHandle handle, uint n) =>
        CaptureError(() => handle.SetMaxConcurrentUploads(n));

    internal static string? SetMaxConcurrentDownloads(AppHandle handle, uint n) =>
        CaptureError(() => handle.SetMaxConcurrentDownloads(n));

    /// <summary>Whether the seek bar's leading label counts down the time remaining.
    /// A synced preference: the write fires a config invalidation, which is what
    /// re-renders the bar.</summary>
    internal static string? SetShowRemainingTime(AppHandle handle, bool enabled) =>
        CaptureError(() => handle.SetShowRemainingTime(enabled));

    internal static string? SetSavePresets(AppHandle handle, IEnumerable<SavePreset> presets) =>
        CaptureError(() => handle.SetSavePresets(
            presets.Select(SavePresetBridge).ToArray()));

    internal static string? SetDefaultTrackSavePreset(AppHandle handle, string presetId) =>
        CaptureError(() => handle.SetDefaultTrackSavePreset(presetId));

    internal static string? SetDefaultReleaseSavePreset(AppHandle handle, string presetId) =>
        CaptureError(() => handle.SetDefaultReleaseSavePreset(presetId));

    internal static string? SetMcpServerConfig(AppHandle handle, bool enabled, ushort port) =>
        CaptureError(() => handle.SetMcpServerConfig(enabled, port));

    internal static BridgeMcpServerStatus McpServerStatus(AppHandle handle) => handle.GetMcpServerStatus();

    internal static string? GetMcpToken(AppHandle handle) => CaptureValue(handle.GetMcpToken);

    internal static string? GenerateMcpToken(AppHandle handle) => CaptureValue(handle.GenerateMcpToken);

    internal static string? SetMcpToken(AppHandle handle, string token) =>
        CaptureError(() => handle.SetMcpToken(token));

    internal static string? SaveDiscogsToken(AppHandle handle, string token) =>
        CaptureValue(() => DiscogsSaveOutcomeTag(Await(handle.SaveDiscogsToken(token))));

    internal static string? RevalidateDiscogsToken(AppHandle handle) =>
        CaptureError(() => Await(handle.RevalidateDiscogsToken()));

    internal static string? DeleteDiscogsToken(AppHandle handle) =>
        CaptureError(handle.RemoveDiscogsToken);

    internal static string? SaveSyncConfig(
        AppHandle handle, string bucket, string region, string endpoint,
        string keyPrefix, string accessKey, string secretKey, string storage) =>
        CaptureError(() => Await(handle.SaveSyncConfig(new BridgeSaveSyncConfig(
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
            return (checked((long)Await(handle.CloudOnlyReleaseCount())), null);
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
        CaptureError(() => Await(handle.SignInCloudProvider(CloudProvider(provider), HomeStorage(storage))));
#else
    internal static string? SignInCloud(AppHandle handle, string provider, string storage) => throw new InvalidOperationException();
#endif

    internal static string? DisconnectCloud(AppHandle handle) =>
        CaptureError(handle.DisconnectCloudProvider);

    internal static void TriggerSync(AppHandle handle) => handle.TriggerSync();

    internal static string? GenerateRestoreCode(AppHandle handle) =>
        CaptureValue(() => Await(handle.GenerateRestoreCode()));

    internal static (BridgeMembership? Membership, string? Error) GetMembers(AppHandle handle) =>
        CaptureBridgeValue(() => Await(handle.GetMembers()));

    internal static string? InviteMember(AppHandle handle, string publicKeyHex) =>
        CaptureValue(() => Await(handle.InviteMember(publicKeyHex, providerAccountEmail: null)));

    internal static string? RemoveMember(AppHandle handle, string publicKeyHex) =>
        CaptureError(() => Await(handle.RemoveMember(publicKeyHex)));

    internal static (BridgeRawReleaseEdit? Edit, string? Error) ReleaseEditSeed(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() => Await(handle.SeedReleaseEdit(releaseId)));

    internal static (BridgeRawReleaseEdit? Edit, string? Error) ResetMetadataToSource(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() => BaeBridgeMethods.RawReleaseEditFromUserEdit(
            Await(handle.ResetMetadataToSource(releaseId)),
            "reset-track"));

    internal static string? ApplyReleaseEdit(AppHandle handle, string releaseId, BridgeRawReleaseEdit edit) =>
        CaptureError(() => Await(handle.UpdateReleaseMetadataUserEdit(releaseId, ReleaseUserEdit(edit))));

    internal static (List<ReleaseCandidateChoice>? Candidates, string? Error) SearchReleases(
        AppHandle handle,
        string source,
        string artist,
        string album) =>
        CaptureBridgeValue(() => CandidateChoices(Await(handle.SearchForCandidate(
            new BridgeSearchQuery.General(artist, album, MetadataSource(source))))));

    internal static string? ReidentifyRelease(AppHandle handle, string releaseId, BridgeIdentityChoice choice) =>
        CaptureError(() => Await(handle.ReIdentifyRelease(releaseId, choice)));

    internal static string? ScanFolder(AppHandle handle, string path, bool clearFirst) =>
        CaptureError(() => handle.AddWatchedFolder(path));

    internal static string? RemoveWatchedFolder(AppHandle handle, string path) =>
        CaptureError(() => handle.RemoveWatchedFolder(path));

    internal static string? SetCandidateSkipped(AppHandle handle, string path, bool skipped) =>
        CaptureError(() => handle.SetCandidateSkipped(path, skipped));

    internal static (List<ImportCandidate> Rows, BridgeWatchedFolder[] Folders) ImportCandidates(AppHandle handle)
    {
        var snapshot = handle.GetImportCandidates();
        return (ImportCandidateRows(snapshot), snapshot.WatchedFolders);
    }

    /// <summary>A text file's decoded contents (bridge-side encoding detection),
    /// or the error line. No session handle: the read is a free bridge call.</summary>
    internal static (string? Text, string? Error) ReadTextFile(string path) =>
        CaptureBridgeValue(() => BaeBridgeMethods.ReadTextFile(path));

    internal static void AutoIdentifyFolder(AppHandle handle, string candidateKey, string folderPath) =>
        handle.AutoIdentifyFolder(candidateKey, folderPath);

    internal static void AutoIdentifyRelease(AppHandle handle, string candidateKey, string releaseId) =>
        handle.AutoIdentifyRelease(candidateKey, releaseId);

    internal static void CancelAutoIdentify(AppHandle handle, string candidateKey) =>
        handle.CancelAutoIdentify(candidateKey);

    /// <summary>
    /// The live identify-pipeline state for a re-identify candidate key: the
    /// localizable row status, the found pressings, and the signals-toolbar
    /// badges. Null until the pipeline's first event lands for the key.
    /// </summary>
    internal static (ImportCandidateRowStatus RowStatus, List<ReleaseCandidateChoice> Matches, List<SignalBadge> Signals)?
        ReidentifyPipeline(AppHandle handle, string candidateKey) =>
        handle.GetCandidate(candidateKey) is BridgeImportCandidateSnapshot.Runtime runtime
            ? (ImportRowStatus(runtime.RuntimeSnapshot),
               ImportMatches(runtime.RuntimeSnapshot.IdentifyState),
               runtime.RuntimeSnapshot.SignalsToolbar.Signals.Select(SignalBadge).ToList())
            : null;

    /// <summary>
    /// Reseed a release's metadata from its (just re-pointed) metadata source:
    /// re-project via reset, then write the projection back. Identity rows are
    /// untouched; the user's prior edits are overwritten by design.
    /// </summary>
    internal static string? RefreshMetadataFromSource(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(handle.UpdateReleaseMetadataUserEdit(
            releaseId, Await(handle.ResetMetadataToSource(releaseId)))));

    internal static void ToggleSignalForCandidate(AppHandle handle, string candidateKey, string kind, string value) =>
        handle.ToggleSignalForCandidate(candidateKey, ExcludedSignal(kind, value));

    internal static void RerunIdentifyForCandidate(AppHandle handle, string candidateKey) =>
        handle.RerunIdentifyForCandidate(candidateKey);

    internal static void PreviewPlay(AppHandle handle, string path) => handle.PreviewPlay(path);

    internal static void PreviewStop(AppHandle handle) => handle.PreviewStop();

    internal static void PreviewTogglePause(AppHandle handle) => handle.PreviewTogglePause();

    internal static (PrefetchedEdit? Prefetched, string? Error) PrefetchCandidateEdit(
        AppHandle handle,
        string releaseId,
        BridgeMetadataSource source,
        string folderPath) =>
        CaptureBridgeValue(() => PrefetchedEdit(handle, releaseId, source, folderPath));

    /// <summary>
    /// The skip-identify confirm seed: project the folder's embedded file tags
    /// into the edit form. No source release, so no remote covers and no kept
    /// detail — only the folder's local artwork is offered.
    /// </summary>
    internal static (PrefetchedEdit? Prefetched, string? Error) PrefetchUnknownEdit(
        AppHandle handle, string folderPath) =>
        CaptureBridgeValue(() => new PrefetchedEdit
        {
            Edit = BaeBridgeMethods.RawReleaseEditFromUserEdit(
                Await(handle.PreviewFileTagsForFolder(folderPath)), "unknown-track"),
            LocalArtwork = LocalArtwork(handle.GetCandidate(folderPath)),
        });

    internal static (BridgeLibraryStatus? Status, string? Error) CheckReleaseInLibrary(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.FindReleaseDetail(releaseId));
            return detail is null
                ? new BridgeLibraryStatus(releaseId, false, false, null, null)
                : new BridgeLibraryStatus(releaseId, true, true, null, detail.AlbumId);
        });

    internal static string? ImportCandidate(
        AppHandle handle, string candidateKey, string folderPath, BridgeIdentityChoice identity, string storageMode, bool pin, BridgeRawReleaseEdit userEdit, BridgeCoverSelection? selectedCover) =>
        CaptureError(() => handle.StartImport(
            candidateKey,
            folderPath,
            selectedCover,
            StorageMode(storageMode),
            pin,
            identity,
            ReleaseUserEdit(userEdit)));

    internal static byte[]? ImageBytes(AppHandle? handle, BridgeImageRef image) =>
        handle is null ? null : CaptureBytes(() => Await(handle.FetchImageBytes(image)));

    internal static byte[]? CoverImageBytes(AppHandle? handle, string imageId) =>
        handle is null ? null : CaptureBytes(() => Await(handle.FetchCoverImageBytes(imageId)));

    internal static byte[]? GalleryBytes(AppHandle? handle, string releaseId, BridgeGallerySource source) =>
        handle is null ? null : CaptureBytes(() => Await(handle.FetchGalleryBytes(releaseId, source)));

    internal static void PlayRelease(AppHandle handle, string releaseId, long startTrackIndex, bool shuffle) =>
        handle.PlayRelease(releaseId, startTrackIndex < 0 ? null : checked((uint)startTrackIndex), shuffle);

    // The album grid's bulk play: fills the context lane with every targeted
    // album's primary release, in the given order. Up Next is left alone and
    // still drains first.
    internal static void PlayReleases(AppHandle handle, IReadOnlyList<string> releaseIds) =>
        handle.PlayReleases(releaseIds.ToArray());

    internal static void PlayLibraryShuffled(AppHandle handle) => handle.PlayLibraryShuffled();

    internal static void Pause(AppHandle handle) => handle.Pause();

    internal static void Resume(AppHandle handle) => handle.Resume();

    internal static void SeekByRatio(AppHandle handle, double ratio) => handle.SeekByRatio(ratio);

    internal static void PreviewSeekByRatio(AppHandle handle, double ratio) => handle.PreviewSeekByRatio(ratio);

    internal static void SetVolume(AppHandle handle, float volume) => handle.SetVolume(volume);

    internal static void SetMuted(AppHandle handle, bool muted) => handle.SetMuted(muted);

    internal static float GetVolume(AppHandle handle) => Await(handle.GetVolume());

    internal static void SetRepeatMode(AppHandle handle, BridgeRepeatMode mode) => handle.SetRepeatMode(mode);

    /// <summary>The mode the repeat button steps to next. Playback only accepts an
    /// absolute mode, so the button computes its target — but the cycle's order is
    /// core's, not this app's.</summary>
    internal static BridgeRepeatMode NextRepeatMode(BridgeRepeatMode mode) =>
        BaeBridgeMethods.BridgeNextRepeatMode(mode);

    internal static void SetShuffle(AppHandle handle, bool on) => handle.SetShuffle(on);

    internal static void Next(AppHandle handle) => handle.NextTrack();

    internal static void Previous(AppHandle handle) => handle.PreviousTrack();

    internal static void QueueSkipTo(AppHandle handle, string entryId) => handle.SkipToEntry(entryId);

    internal static void QueueRemove(AppHandle handle, string entryId) => handle.RemoveEntry(entryId);

    internal static void QueueReorder(AppHandle handle, string entryId, string? beforeEntryId) =>
        handle.ReorderEntry(entryId, beforeEntryId);

    internal static void QueueClearUpNext(AppHandle handle) => handle.ClearUpNext();

    internal static void QueueClearPlayingFrom(AppHandle handle) => handle.ClearPlayingFrom();

    // One page of the context's upcoming tail past the window QueueUpdated
    // already carries. offset 0 is the first not-yet-played entry after the
    // current track — the same coordinate space as BridgePlaybackContext.Upcoming.
    internal static (BridgeQueueUpcomingPage? Page, string? Error) QueueUpcomingPage(AppHandle handle, uint offset, uint limit) =>
        CaptureBridgeValue(() => Await(handle.GetQueueUpcomingPage(offset, limit)));

    internal static void AddReleaseToQueue(AppHandle handle, string releaseId) => handle.AddReleaseToQueue(releaseId);

    internal static void AddReleaseNext(AppHandle handle, string releaseId) => handle.AddReleaseNext(releaseId);

    internal static string? AddToQueue(AppHandle handle, IReadOnlyList<string> trackIds) =>
        CaptureError(() => handle.AddToQueue(trackIds.ToArray()));

    internal static string? AddNext(AppHandle handle, IReadOnlyList<string> trackIds) =>
        CaptureError(() => handle.AddNext(trackIds.ToArray()));

    // Expand a list of album or track ids to track ids (album ids resolve to the
    // primary release's tracks), for a queue insert that carries a drag payload.
    internal static (List<string>? Value, string? Error) ResolveToTrackIds(AppHandle handle, IReadOnlyList<string> ids) =>
        CaptureBridgeValue(() => Await(handle.ResolveToTrackIds(ids.ToArray())).ToList());

    // Insert tracks into the manual lane at index (core clamps to the lane length).
    internal static void InsertInQueue(AppHandle handle, IReadOnlyList<string> trackIds, int index) =>
        handle.InsertInQueue(trackIds.ToArray(), checked((uint)index));

    internal static string? DeleteRelease(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(handle.DeleteRelease(releaseId)));

    internal static void Shutdown(AppHandle handle) => Await(handle.Shutdown());

    private static T Await<T>(System.Threading.Tasks.Task<T> task) => task.GetAwaiter().GetResult();

    private static void Await(System.Threading.Tasks.Task task) => task.GetAwaiter().GetResult();

    private static string? CaptureError(Action action)
    {
        try
        {
            action();
            return null;
        }
        catch (BridgeException.Cancelled)
        {
            return null;
        }
        catch (BridgeException exception)
        {
            return exception.Message;
        }
    }

    private static string? CaptureValue(Func<string?> action)
    {
        try
        {
            return action();
        }
        catch (BridgeException.Cancelled)
        {
            return null;
        }
        catch (BridgeException exception)
        {
            return exception.Message;
        }
    }

    private static (T? Value, string? Error) CaptureBridgeValue<T>(Func<T> action) where T : class
    {
        try
        {
            return (action(), null);
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

    private static byte[]? CaptureBytes(Func<byte[]?> action)
    {
        try
        {
            return action();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Warning($"image bytes read failed: {exception.Message}");
            return null;
        }
    }

    private static BridgeSortCriterion[] ToBridge(IReadOnlyList<SortCriterion<AlbumSortField>> criteria) =>
        criteria.Select(criterion =>
            new BridgeSortCriterion(ToBridge(criterion.Field), ToBridge(criterion.Direction))).ToArray();

    private static BridgeComposerSortCriterion[] ToBridge(IReadOnlyList<SortCriterion<ComposerSortField>> criteria) =>
        criteria.Select(criterion =>
            new BridgeComposerSortCriterion(ToBridge(criterion.Field), ToBridge(criterion.Direction))).ToArray();

    private static BridgeArtistSortCriterion[] ToBridge(IReadOnlyList<SortCriterion<ArtistSortField>> criteria) =>
        criteria.Select(criterion =>
            new BridgeArtistSortCriterion(ToBridge(criterion.Field), ToBridge(criterion.Direction))).ToArray();

    private static BridgeSortField ToBridge(AlbumSortField field) => field switch
    {
        AlbumSortField.DateAdded => BridgeSortField.DateAdded,
        AlbumSortField.Title => BridgeSortField.Title,
        AlbumSortField.Artist => BridgeSortField.Artist,
        AlbumSortField.Year => BridgeSortField.Year,
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown album sort field"),
    };

    private static BridgeComposerSortField ToBridge(ComposerSortField field) => field switch
    {
        ComposerSortField.Name => BridgeComposerSortField.Name,
        ComposerSortField.WorkCount => BridgeComposerSortField.WorkCount,
        ComposerSortField.LinkedReleaseCount => BridgeComposerSortField.LinkedReleaseCount,
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown composer sort field"),
    };

    private static BridgeArtistSortField ToBridge(ArtistSortField field) => field switch
    {
        ArtistSortField.Name => BridgeArtistSortField.Name,
        ArtistSortField.AlbumCount => BridgeArtistSortField.AlbumCount,
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown artist sort field"),
    };

    private static BridgeSortDirection ToBridge(SortDirection direction) => direction switch
    {
        SortDirection.Ascending => BridgeSortDirection.Ascending,
        SortDirection.Descending => BridgeSortDirection.Descending,
        _ => throw new ArgumentOutOfRangeException(nameof(direction), direction, "Unknown sort direction"),
    };

    private static Settings Settings(
        BridgeConfig config,
        BridgeMcpServerStatus mcpStatus,
        BridgeSyncStatusSnapshot syncStatus) =>
        new()
        {
            LibraryName = config.LibraryName,
            LibraryId = config.LibraryId,
            DiscogsStatus = DiscogsStatusTag(config.DiscogsTokenStatus),
            DiscogsUsable = config.DiscogsUsable,
            SyncProvider = config.Sync is null ? null : SyncProviderTag(config.Sync.Provider),
            SyncAccount = config.Sync?.CloudAccountDisplay,
            SyncReady = syncStatus.SyncReady,
            PauseBetweenSides = config.PauseBetweenSides,
            ShowRemainingTime = config.ShowRemainingTime,
            LibraryFullWidth = config.LibraryFullWidth,
            SavePresets = config.SavePresets.Select(SavePreset).ToList(),
            DefaultTrackSavePreset = config.DefaultTrackSavePreset,
            DefaultReleaseSavePreset = config.DefaultReleaseSavePreset,
            McpEnabled = config.Mcp.Enabled,
            McpPort = config.Mcp.Port,
            McpStatus = mcpStatus,
        };

    private static List<ReleaseCandidateChoice> CandidateChoices(BridgeCandidateSearchResults results) =>
        results.Groups
            .SelectMany(group => group.Pressings.Select(pressing => new ReleaseCandidateChoice(group, pressing)))
            .ToList();

    private static PrefetchedEdit PrefetchedEdit(
        AppHandle handle,
        string releaseId,
        BridgeMetadataSource source,
        string folderPath)
    {
        var localTrackCount = LocalTrackCount(handle.GetCandidate(folderPath));
        var prefetch = Await(handle.PrefetchRelease(releaseId, source, localTrackCount));
        // A picked release claims the exact pressing by default; the confirm
        // dialog re-shapes this seed if the user switches to metadata only.
        var edit = ShapeCandidateEdit(prefetch.Seed, new BridgeIdentityChoice.Exact(releaseId, source));
        return new PrefetchedEdit
        {
            Edit = edit,
            RemoteCovers = prefetch.Detail.CoverArt.ToList(),
            LocalArtwork = LocalArtwork(handle.GetCandidate(folderPath)),
            Seed = prefetch.Seed,
        };
    }

    private static uint? LocalTrackCount(BridgeImportCandidateSnapshot? snapshot) =>
        snapshot is BridgeImportCandidateSnapshot.Folder folder
            ? folder.Candidate.TrackCount
            : null;

    private static List<LocalArtwork> LocalArtwork(BridgeImportCandidateSnapshot? snapshot) =>
        snapshot is BridgeImportCandidateSnapshot.Folder folder
            ? folder.Candidate.Files.Artwork.Select(LocalArtwork).ToList()
            : [];

    private static LocalArtwork LocalArtwork(BridgeArtworkFile artwork)
    {
        var releaseImage = artwork.CoverChoice.Selection as BridgeCoverSelection.ReleaseImage
            ?? throw new JsonException("local artwork did not carry a release-image selection");
        return new LocalArtwork
        {
            FileId = releaseImage.FileId,
            Path = artwork.File.LocalPath,
        };
    }

    private static List<ImportCandidate> ImportCandidateRows(BridgeImportCandidatesSnapshot snapshot)
    {
        var folderRows = snapshot.FolderCandidates
            .Select(candidate => ImportCandidateRow(candidate.Candidate, candidate.Runtime));
        var invalidRows = snapshot.InvalidCandidates.Select(InvalidImportCandidateRow);
        return folderRows.Concat(invalidRows).ToList();
    }

    private static ImportCandidate ImportCandidateRow(
        BridgeFolderCandidate candidate,
        BridgeCandidateRuntimeSnapshot runtime) =>
        new()
        {
            Key = candidate.FolderPath,
            Name = candidate.SourceFolderName,
            TrackCount = checked((int)candidate.TrackCount),
            Format = ImportFormat(candidate.Files.Audio),
            RowStatus = ImportRowStatus(runtime),
            Matches = ImportMatches(runtime.IdentifyState),
            Signals = runtime.SignalsToolbar.Signals.Select(SignalBadge).ToList(),
            AudioPaths = AudioPaths(candidate.Files.Audio).ToList(),
            Documents = ImportDocuments(candidate.Files),
            FolderPath = candidate.FolderPath,
            Skipped = candidate.Skipped,
            IsAdded = candidate.IsAdded,
        };

    private static ImportCandidate InvalidImportCandidateRow(BridgeInvalidCandidate candidate) =>
        new()
        {
            Key = candidate.FolderPath,
            Name = candidate.SourceFolderName,
            TrackCount = 0,
            Format = string.Empty,
            RowStatus = new ImportCandidateRowStatus
            {
                Kind = "error",
                InvalidReason = candidate.Reason,
            },
            Matches = [],
            Signals = [],
            AudioPaths = [],
            Documents = [],
            FolderPath = candidate.FolderPath,
            Invalid = true,
        };

    private static ImportCandidateRowStatus ImportRowStatus(BridgeCandidateRuntimeSnapshot runtime)
    {
        if (runtime.ImportStatus is BridgeCandidateImportStatus.Importing importing)
        {
            return new ImportCandidateRowStatus
            {
                Kind = "importing",
                ProgressPercent = checked((int)importing.ProgressPercent),
                Step = importing.Step is null ? null : ImportStep(importing.Step),
            };
        }
        if (runtime.ImportStatus is BridgeCandidateImportStatus.Complete)
        {
            return new ImportCandidateRowStatus { Kind = "complete" };
        }
        if (runtime.ImportStatus is BridgeCandidateImportStatus.Error error)
        {
            return new ImportCandidateRowStatus
            {
                Kind = "error",
                Error = error.ErrorValue,
            };
        }

        return runtime.IdentifyState switch
        {
            BridgeIdentifyState.Idle => new ImportCandidateRowStatus { Kind = string.Empty },
            BridgeIdentifyState.Triangulating => new ImportCandidateRowStatus { Kind = "identifying" },
            BridgeIdentifyState.Found found => new ImportCandidateRowStatus { Kind = "found", Count = checked((int)found.Group.Pressings.Length) },
            BridgeIdentifyState.Conflict => new ImportCandidateRowStatus { Kind = "conflict" },
            BridgeIdentifyState.NotFoundAnywhere => new ImportCandidateRowStatus { Kind = "not_found" },
            BridgeIdentifyState.ManualOnly => new ImportCandidateRowStatus { Kind = "manual" },
            _ => throw new ArgumentOutOfRangeException(nameof(runtime), runtime.IdentifyState, "Unknown identify state"),
        };
    }

    private static List<ReleaseCandidateChoice> ImportMatches(BridgeIdentifyState state) =>
        state is BridgeIdentifyState.Found found
            ? found.Group.Pressings.Select(pressing => new ReleaseCandidateChoice(found.Group, pressing)).ToList()
            : [];

    private static SignalBadge SignalBadge(BridgeToolbarSignal signal) =>
        new()
        {
            Kind = SignalKindTag(signal.Kind),
            Value = signal.Value,
            State = SignalState(signal.State),
            Excluded = signal.Excluded,
        };

    private static SignalBadgeState SignalState(BridgeSignalState state) =>
        state switch
        {
            BridgeSignalState.LookingUp => new() { Kind = "looking_up" },
            BridgeSignalState.Found found => new() { Kind = "found", Count = found.Count },
            BridgeSignalState.NoMatch => new() { Kind = "no_match" },
            BridgeSignalState.Skipped => new() { Kind = "skipped" },
            BridgeSignalState.Failed failed => new() { Kind = "failed", Failure = failed.Failure },
            BridgeSignalState.Confirms confirms => new() { Kind = "confirms", Count = confirms.Count },
            _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown signal state"),
        };

    private static ImportStep ImportStep(BridgeImportStep step) =>
        step switch
        {
            BridgeImportStep.Preparing preparing => new() { Kind = "preparing", StepTag = PrepareStepTag(preparing.Step) },
            BridgeImportStep.Running running => new() { Kind = "running", Phase = ImportPhaseTag(running.Phase) },
            _ => throw new ArgumentOutOfRangeException(nameof(step), step, "Unknown import step"),
        };

    private static string[] AudioPaths(BridgeAudioContent audio) =>
        audio switch
        {
            BridgeAudioContent.CueFlacPairs cue => cue.Pairs.Select(pair => pair.FlacLocalPath).ToArray(),
            BridgeAudioContent.TrackFiles tracks => tracks.Files.Select(file => file.LocalPath).ToArray(),
            _ => throw new ArgumentOutOfRangeException(nameof(audio), audio, "Unknown audio content"),
        };

    // A candidate's readable evidence files: paired CUE sheets first (they live
    // on the audio content as CUE/FLAC pairs), then core's path-sorted document
    // files (logs, unpaired CUEs, text). The same visual order as the file pane
    // on other platforms, where the CUE rows sit above the documents section.
    private static List<ImportDocument> ImportDocuments(BridgeCandidateFiles files)
    {
        var cues = files.Audio switch
        {
            BridgeAudioContent.CueFlacPairs cue => cue.Pairs.Select(pair => new ImportDocument
            {
                Name = pair.CueName,
                Path = pair.CueLocalPath,
                SizeBytes = checked((long)pair.CueSize),
            }),
            BridgeAudioContent.TrackFiles => Enumerable.Empty<ImportDocument>(),
            _ => throw new ArgumentOutOfRangeException(nameof(files), files.Audio, "Unknown audio content"),
        };
        var documents = files.Documents.Select(file => new ImportDocument
        {
            Name = file.Name,
            Path = file.LocalPath,
            SizeBytes = checked((long)file.Size),
        });
        return cues.Concat(documents).ToList();
    }

    private static string ImportFormat(BridgeAudioContent audio) =>
        audio is BridgeAudioContent.CueFlacPairs ? "CUE/FLAC" : string.Empty;

    private static string CoverImageSourceUrl(BridgeCoverImageSource source) =>
        source switch
        {
            BridgeCoverImageSource.Remote remote => remote.Url,
            BridgeCoverImageSource.Local local => local.Path,
            _ => throw new ArgumentOutOfRangeException(nameof(source), source, "Unknown cover image source"),
        };

    private static SavePreset SavePreset(BridgeSavePreset preset) =>
        new()
        {
            Id = preset.Id,
            Name = preset.Name,
            Codec = preset.Codec,
            Extension = preset.Extension,
            FilenameTokens = preset.FilenameTokens,
            PregapPlacement = preset.PregapPlacement,
            AppliesToTrack = preset.AppliesToTrack,
            AppliesToRelease = preset.AppliesToRelease,
            EmbedCover = preset.EmbedCover,
        };

    private static BridgeSavePreset SavePresetBridge(SavePreset preset) =>
        new(
            preset.Id,
            preset.Name,
            preset.Codec,
            preset.Extension,
            preset.FilenameTokens,
            preset.PregapPlacement,
            preset.AppliesToTrack,
            preset.AppliesToRelease,
            preset.EmbedCover);

    private static string DiscogsStatusTag(BridgeDiscogsTokenStatus status) =>
        status switch
        {
            BridgeDiscogsTokenStatus.NotConfigured => "not_configured",
            BridgeDiscogsTokenStatus.Valid => "valid",
            BridgeDiscogsTokenStatus.Unvalidated => "unvalidated",
            BridgeDiscogsTokenStatus.Rejected => "rejected",
            _ => throw new ArgumentOutOfRangeException(nameof(status), status, "Unknown Discogs token status"),
        };

    private static string SyncProviderTag(BridgeSyncProvider provider) =>
        provider switch
        {
            BridgeSyncProvider.S3 => "s3",
            BridgeSyncProvider.GoogleDrive => "google_drive",
            BridgeSyncProvider.Dropbox => "dropbox",
            BridgeSyncProvider.OneDrive => "onedrive",
            BridgeSyncProvider.CloudKit => "cloudkit",
            _ => throw new ArgumentOutOfRangeException(nameof(provider), provider, "Unknown sync provider"),
        };

    private static string SignalKindTag(BridgeSignalKind kind) =>
        kind switch
        {
            BridgeSignalKind.DiscId => "disc_id",
            BridgeSignalKind.Barcode => "barcode",
            BridgeSignalKind.Catalog => "catalog",
            _ => throw new ArgumentOutOfRangeException(nameof(kind), kind, "Unknown signal kind"),
        };

    private static string PrepareStepTag(BridgePrepareStep step) =>
        step switch
        {
            BridgePrepareStep.ParsingMetadata => "parsing_metadata",
            BridgePrepareStep.WritingCoverArt => "writing_cover_art",
            BridgePrepareStep.DiscoveringFiles => "discovering_files",
            BridgePrepareStep.ValidatingTracks => "validating_tracks",
            BridgePrepareStep.SavingToDatabase => "saving_to_database",
            _ => throw new ArgumentOutOfRangeException(nameof(step), step, "Unknown prepare step"),
        };

    private static string ImportPhaseTag(BridgeImportPhase phase) =>
        phase switch
        {
            BridgeImportPhase.ReferencingFiles => "referencing_files",
            BridgeImportPhase.MeasuringLoudness => "measuring_loudness",
            BridgeImportPhase.Finalizing => "finalizing",
            _ => throw new ArgumentOutOfRangeException(nameof(phase), phase, "Unknown import phase"),
        };

    private static string ValidationReasonTag(BridgeValidationReason reason) =>
        reason switch
        {
            BridgeValidationReason.EmptyAlbumTitle => "empty_album_title",
            BridgeValidationReason.NoAlbumArtist => "no_album_artist",
            BridgeValidationReason.InvalidYear => "invalid_year",
            _ => throw new ArgumentOutOfRangeException(nameof(reason), reason, "Unknown validation reason"),
        };


    private static BridgeMetadataSource MetadataSource(string source) =>
        source == "discogs" ? BridgeMetadataSource.Discogs : BridgeMetadataSource.MusicBrainz;

    private static BridgeHomeStorage HomeStorage(string storage) =>
        storage == "browsable" ? BridgeHomeStorage.Browsable : BridgeHomeStorage.Opaque;

    private static BridgeStorageMode StorageMode(string storageMode) =>
        storageMode == "managed" ? BridgeStorageMode.Remote : BridgeStorageMode.Local;

    private static BridgeExcludedSignal ExcludedSignal(string kind, string value) =>
        kind switch
        {
            "disc_id" => new BridgeExcludedSignal.Disc(),
            "barcode" => new BridgeExcludedSignal.Barcode(),
            _ => new BridgeExcludedSignal.Catalog(value),
        };

    /// <summary>
    /// The identity claim for a source-backed pick: <c>Exact</c> keeps the picked
    /// pressing's fields, <c>Approximate</c> claims only the album group (core
    /// blanks the pressing fields). The only place these two variants are built.
    /// </summary>
    internal static BridgeIdentityChoice SourceIdentityChoice(bool exact, string releaseId, BridgeMetadataSource source) =>
        exact
            ? new BridgeIdentityChoice.Exact(releaseId, source)
            : new BridgeIdentityChoice.Approximate(releaseId, source);

    /// <summary>
    /// Re-shape the confirm-form seed from the kept editor seed under a new
    /// identity claim — used when the user flips exact ↔ metadata only, with no
    /// re-fetch. Approximate nils the pressing fields; Exact keeps them.
    /// </summary>
    internal static BridgeRawReleaseEdit ShapeCandidateEdit(BridgeReleaseUserEdit seed, BridgeIdentityChoice choice) =>
        BaeBridgeMethods.RawReleaseEditFromUserEdit(
            BaeBridgeMethods.ShapeUserEditForChoice(seed, choice),
            "prefetch-track");

    private static BridgeReleaseUserEdit ReleaseUserEdit(BridgeRawReleaseEdit edit) =>
        BaeBridgeMethods.ShapeReleaseEdit(edit) switch
        {
            BridgeShapeResult.Valid valid => valid.Edit,
            BridgeShapeResult.Invalid invalid => throw new ArgumentException(
                $"invalid release edit: {ValidationReasonTag(invalid.Reason)}",
                nameof(edit)),
            _ => throw new ArgumentException("invalid release edit", nameof(edit)),
        };

    private static string DiscogsSaveOutcomeTag(BridgeDiscogsSaveOutcome outcome) =>
        outcome switch
        {
            BridgeDiscogsSaveOutcome.Valid => "valid",
            BridgeDiscogsSaveOutcome.Unvalidated => "unvalidated",
            BridgeDiscogsSaveOutcome.Rejected => "rejected",
            _ => throw new ArgumentOutOfRangeException(nameof(outcome), outcome, "Unknown Discogs save outcome"),
        };


}

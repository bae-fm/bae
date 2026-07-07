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
    private static BridgeDiagnostics? Diagnostics;

    /// <summary>One-time startup: register the OS credential store.</summary>
    internal static void Startup() => BaeBridgeMethods.InitKeyring();

    internal static string? ConfigureDiagnostics(
        string? datadogSite,
        string? clientToken,
        string source,
        string service,
        string? environment,
        string appVersion,
        string edition,
        string? gitCommit)
    {
        BridgeDiagnosticsConfig config = datadogSite is not null && clientToken is not null
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

        return CaptureError(() => Diagnostics = BaeBridgeMethods.ConfigureDiagnostics(config));
    }

    internal static string? DiagnosticsLog(
        string level,
        string target,
        string message,
        IEnumerable<KeyValuePair<string, string>>? fields = null) =>
        CaptureError(() => Diagnostics?.Log(DiagnosticLevel(level), target, message, DiagnosticFields(fields)));

    internal static string? DiagnosticsEvent(
        string name,
        IEnumerable<KeyValuePair<string, string>>? fields = null) =>
        CaptureError(() => Diagnostics?.Event(name, DiagnosticFields(fields)));

    internal static string? FlushDiagnostics() =>
        CaptureError(() => Diagnostics?.Flush().GetAwaiter().GetResult());

    internal static string? SetOauthClientCreds(string credsJson) =>
        CaptureError(() => BaeBridgeMethods.SetOauthClientCreds(credsJson));

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

    private static BridgeDiagnosticField[] DiagnosticFields(IEnumerable<KeyValuePair<string, string>>? fields) =>
        (fields ?? []).Select(field => new BridgeDiagnosticField(field.Key, field.Value)).ToArray();

    private static BridgeDiagnosticLevel DiagnosticLevel(string level) =>
        level switch
        {
            "trace" => BridgeDiagnosticLevel.Trace,
            "debug" => BridgeDiagnosticLevel.Debug,
            "info" => BridgeDiagnosticLevel.Info,
            "warn" => BridgeDiagnosticLevel.Warn,
            "error" => BridgeDiagnosticLevel.Error,
            _ => throw new ArgumentOutOfRangeException(nameof(level), level, "Unknown diagnostics level"),
        };

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

    /// <summary>
    /// The joiner's account email for an OAuth provider, fetched from its
    /// authenticated session. <paramref name="provider"/> is the wire tag
    /// ("google_drive"/…).
    /// </summary>
    internal static string FetchAccountEmail(string provider, string oauthTokenJson) =>
        BaeBridgeMethods.FetchAccountEmail(CloudProvider(provider), oauthTokenJson);

    /// <summary>The catalog key for a channel count's word ("mono"/"stereo"), or
    /// null for counts the UI renders as "{n}ch".</summary>
    internal static string? AudioChannelsKey(long channels) =>
        BaeBridgeMethods.BridgeAudioChannelsKey(channels);

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
    internal static string OAuthAuthorize(string provider) =>
        BaeBridgeMethods.OauthAuthorize(CloudProvider(provider));

    internal static string OAuthAuthorize(BridgeCloudProvider provider) =>
        BaeBridgeMethods.OauthAuthorize(provider);

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
    /// Join a shared library from an invite code and return its id. For OAuth
    /// providers pass the token JSON from <see cref="OAuthAuthorize"/>; for
    /// credential providers pass null. Blocks on a cloud pull — call off the UI
    /// thread.
    /// </summary>
    internal static string JoinFromCode(string code, string? oauthTokenJson) =>
        BaeBridgeMethods.JoinFromCode(code, oauthTokenJson).Id;

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
        var source = new BridgeRestoreSource.S3(
            bucket,
            region,
            string.IsNullOrWhiteSpace(endpoint) ? null : endpoint.Trim(),
            accessKey,
            secretKey);
        return BaeBridgeMethods.RestoreFromCloud(libraryId, encryptionKeyHex, libraryName, source).Id;
    }

    internal sealed class EventCallback(Action<BridgeUiEvent> onEvent) : UiEventCallback
    {
        public void OnEvent(BridgeUiEvent @event) => onEvent(@event);
    }

    internal static AppHandle? Init(string libraryId, uint positionUpdateIntervalMs)
    {
        try
        {
            return BaeBridgeMethods.InitApp(libraryId, positionUpdateIntervalMs);
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error($"library open failed: {exception.Message}");
            return null;
        }
    }

    internal static bool HasEncryptionKey(AppHandle handle) => handle.HasEncryptionKey();

    internal static void HandleFree(AppHandle handle) => handle.Dispose();

    internal static void Subscribe(AppHandle handle, EventCallback callback) =>
        handle.SubscribeUiEvents(callback);

    internal static string? LockActiveLibrary(AppHandle handle) =>
        CaptureError(() => handle.LockActiveLibrary());

    internal static string? RenameLibrary(AppHandle handle, string libraryId, string name) =>
        CaptureError(() => handle.RenameLibrary(libraryId, name));

    internal static string? SetPrimaryRelease(AppHandle handle, string albumId, string releaseId) =>
        CaptureError(() => Await(handle.SetPrimaryRelease(albumId, releaseId)));

    internal static string? ExportTrack(AppHandle handle, string trackId, string outputPath, BridgeExportSelection selection) =>
        CaptureError(() => Await(handle.ExportTrack(trackId, outputPath, selection)));

    internal static string? ExportRelease(AppHandle handle, string releaseId, string targetDir, BridgeExportSelection selection) =>
        CaptureError(() => Await(handle.EnqueueExport(releaseId, targetDir, selection)));

    internal static string? ExportTrackSuggestedName(AppHandle handle, string trackId) =>
        CaptureValue(() => Await(handle.ExportTrackSuggestedName(trackId)));

    internal static string? ExportTrackExtension(AppHandle handle, string trackId, BridgeExportSelection selection) =>
        CaptureValue(() => Await(handle.ExportTrackExtension(trackId, selection)));

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

    internal static (List<Album>? Albums, string? Error) AlbumPage(AppHandle handle, ulong offset, ulong limit, string sortField, bool ascending) =>
        CaptureBridgeValue(() => Await(handle.GetAlbumPage(SortCriteria(sortField, ascending), offset, limit))
            .Select(album => new Album(album))
            .ToList());

    internal static long ComposerCount(AppHandle handle) =>
        checked((long)Await(handle.GetComposerCount()));

    internal static (List<ComposerSummary>? Composers, string? Error) ComposerPage(AppHandle handle, ulong offset, ulong limit, string sortField, bool ascending) =>
        CaptureBridgeValue(() => Await(handle.GetComposerPage(ComposerSort(sortField, ascending), offset, limit))
            .Select(composer => new ComposerSummary(composer))
            .ToList());

    internal static (BridgeGalleryItem[]? Items, string? Error) Gallery(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.FindReleaseDetail(releaseId));
            return detail?.GalleryItems ?? [];
        });

    internal static (BridgeStorageRow[]? Rows, string? Error) StorageRows(AppHandle handle) =>
        CaptureBridgeValue(() =>
        {
            var filter = BridgeStorageFilter.All;
            var sort = new BridgeStorageSort(BridgeStorageSortField.AlbumTitle, BridgeStorageSortDirection.Ascending);
            var count = Await(handle.StorageCount(filter));
            return Await(handle.StoragePage(sort, filter, 0, count)).Rows;
        });

    internal static string? PinRelease(AppHandle handle, string releaseId) =>
        CaptureError(() => Await(handle.QueuePinReleases([releaseId])));

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

    internal static string? SetExportFilenameTemplate(AppHandle handle, string template) =>
        CaptureError(() => handle.SetExportFilenameTemplate(template));

    internal static string? SetExportPresets(AppHandle handle, IEnumerable<ExportPreset> presets) =>
        CaptureError(() => handle.SetExportPresets(
            presets.Select(ExportPresetBridge).ToArray()));

    internal static string? SetDefaultTrackExportSelection(AppHandle handle, BridgeExportSelection selection) =>
        CaptureError(() => handle.SetDefaultTrackExportSelection(selection));

    internal static string? SetDefaultReleaseExportSelection(AppHandle handle, BridgeExportSelection selection) =>
        CaptureError(() => handle.SetDefaultReleaseExportSelection(selection));

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

    internal static string? DisconnectWarning(AppHandle handle) =>
        CaptureValue(() => Await(handle.DisconnectWarningMessage()));

    internal static string? SignInCloud(AppHandle handle, string provider, string storage) =>
        CaptureError(() => Await(handle.SignInCloudProvider(CloudProvider(provider), HomeStorage(storage))));

    internal static string? DisconnectCloud(AppHandle handle) =>
        CaptureError(handle.DisconnectCloudProvider);

    internal static void TriggerSync(AppHandle handle) => handle.TriggerSync();

    internal static string? GenerateRestoreCode(AppHandle handle) => CaptureValue(handle.GenerateRestoreCode);

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

    internal static string? ReidentifyRelease(AppHandle handle, string releaseId, string chosenReleaseId, BridgeMetadataSource source) =>
        CaptureError(() => Await(handle.ReIdentifyRelease(releaseId, new BridgeIdentityChoice.Exact(chosenReleaseId, source))));

    internal static string? ScanFolder(AppHandle handle, string path, bool clearFirst) =>
        CaptureError(() => handle.AddWatchedFolder(path));

    internal static List<ImportCandidate> ImportCandidates(AppHandle handle) =>
        ImportCandidateRows(handle.GetImportCandidates());

    internal static void AutoIdentifyFolder(AppHandle handle, string candidateKey, string folderPath) =>
        handle.AutoIdentifyFolder(candidateKey, folderPath);

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

    internal static (BridgeLibraryStatus? Status, string? Error) CheckReleaseInLibrary(AppHandle handle, string releaseId) =>
        CaptureBridgeValue(() =>
        {
            var detail = Await(handle.FindReleaseDetail(releaseId));
            return detail is null
                ? new BridgeLibraryStatus(releaseId, false, false, null, null)
                : new BridgeLibraryStatus(releaseId, true, true, null, detail.AlbumId);
        });

    internal static string? ImportCandidate(
        AppHandle handle, string candidateKey, string folderPath, string chosenReleaseId, BridgeMetadataSource source, string storageMode, bool pin, BridgeRawReleaseEdit userEdit, BridgeCoverSelection? selectedCover) =>
        CaptureError(() => handle.StartImport(
            candidateKey,
            folderPath,
            selectedCover,
            StorageMode(storageMode),
            pin,
            new BridgeIdentityChoice.Exact(chosenReleaseId, source),
            ReleaseUserEdit(userEdit)));

    internal static byte[]? ImageBytes(AppHandle? handle, BridgeImageRef image) =>
        handle is null ? null : CaptureBytes(() => Await(handle.FetchImageBytes(image)));

    internal static byte[]? CoverImageBytes(AppHandle? handle, string imageId) =>
        handle is null ? null : CaptureBytes(() => Await(handle.FetchCoverImageBytes(imageId)));

    internal static byte[]? GalleryBytes(AppHandle? handle, string releaseId, BridgeGallerySource source) =>
        handle is null ? null : CaptureBytes(() => Await(handle.FetchGalleryBytes(releaseId, source)));

    internal static void PlayRelease(AppHandle handle, string releaseId, long startTrackIndex, bool shuffle) =>
        handle.PlayRelease(releaseId, startTrackIndex < 0 ? null : checked((uint)startTrackIndex), shuffle);

    internal static void PlayLibraryShuffled(AppHandle handle) => handle.PlayLibraryShuffled();

    internal static void PlayPause(AppHandle handle) => handle.TogglePlayPause();

    internal static void SeekByRatio(AppHandle handle, double ratio) => handle.SeekByRatio(ratio);

    internal static void SetVolume(AppHandle handle, float volume) => handle.SetVolume(volume);

    internal static void ToggleMute(AppHandle handle) => handle.ToggleMute();

    internal static float GetVolume(AppHandle handle) => Await(handle.GetVolume());

    internal static void CycleRepeatMode(AppHandle handle) => handle.CycleRepeatMode();

    internal static void SetShuffle(AppHandle handle, bool on) => handle.SetShuffle(on);

    internal static void Next(AppHandle handle) => handle.NextTrack();

    internal static void Previous(AppHandle handle) => handle.PreviousTrack();

    internal static void QueueSkipTo(AppHandle handle, string entryId) => handle.SkipToEntry(entryId);

    internal static void QueueRemove(AppHandle handle, string entryId) => handle.RemoveEntry(entryId);

    internal static void QueueReorder(AppHandle handle, string entryId, string? beforeEntryId) =>
        handle.ReorderEntry(entryId, beforeEntryId);

    internal static void QueueClear(AppHandle handle) => handle.ClearQueue();

    internal static void AddReleaseToQueue(AppHandle handle, string releaseId) => handle.AddReleaseToQueue(releaseId);

    internal static void AddReleaseNext(AppHandle handle, string releaseId) => handle.AddReleaseNext(releaseId);

    internal static string? AddToQueue(AppHandle handle, IReadOnlyList<string> trackIds) =>
        CaptureError(() => handle.AddToQueue(trackIds.ToArray()));

    internal static string? AddNext(AppHandle handle, IReadOnlyList<string> trackIds) =>
        CaptureError(() => handle.AddNext(trackIds.ToArray()));

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

    private static BridgeSortCriterion[] SortCriteria(string sortField, bool ascending) =>
    [
        new BridgeSortCriterion(
            sortField switch
            {
                "title" => BridgeSortField.Title,
                "artist" => BridgeSortField.Artist,
                "year" => BridgeSortField.Year,
                _ => BridgeSortField.DateAdded,
            },
            ascending ? BridgeSortDirection.Ascending : BridgeSortDirection.Descending),
    ];

    private static BridgeComposerSortCriterion ComposerSort(string sortField, bool ascending) =>
        new(
            sortField switch
            {
                "work_count" => BridgeComposerSortField.WorkCount,
                "linked_release_count" => BridgeComposerSortField.LinkedReleaseCount,
                _ => BridgeComposerSortField.Name,
            },
            ascending ? BridgeSortDirection.Ascending : BridgeSortDirection.Descending);

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
            ExportFilenameTemplate = config.ExportFilenameTemplate,
            ExportPresets = config.ExportPresets.Select(ExportPreset).ToList(),
            DefaultTrackExportSelection = config.DefaultTrackExportSelection,
            DefaultReleaseExportSelection = config.DefaultReleaseExportSelection,
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
        var detail = Await(handle.PrefetchRelease(releaseId, source, localTrackCount));
        var choice = new BridgeIdentityChoice.Exact(releaseId, source);
        var edit = BaeBridgeMethods.RawReleaseEditFromUserEdit(
            BaeBridgeMethods.ShapeUserEditFromReleaseDetail(detail, choice),
            "prefetch-track");
        return new PrefetchedEdit
        {
            Edit = edit,
            RemoteCovers = detail.CoverArt.ToList(),
            LocalArtwork = LocalArtwork(handle.GetCandidate(folderPath)),
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
            FolderPath = candidate.FolderPath,
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
            FolderPath = candidate.FolderPath,
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

    private static string ImportFormat(BridgeAudioContent audio) =>
        audio is BridgeAudioContent.CueFlacPairs ? "CUE/FLAC" : string.Empty;

    private static string CoverImageSourceUrl(BridgeCoverImageSource source) =>
        source switch
        {
            BridgeCoverImageSource.Remote remote => remote.Url,
            BridgeCoverImageSource.Local local => local.Path,
            _ => throw new ArgumentOutOfRangeException(nameof(source), source, "Unknown cover image source"),
        };

    private static ExportPreset ExportPreset(BridgeExportPreset preset) =>
        new()
        {
            Id = preset.Id,
            Name = preset.Name,
            Codec = preset.Codec,
            Extension = preset.Extension,
            FilenameTemplate = preset.FilenameTemplate,
            PregapPlacement = preset.PregapPlacement,
            AppliesToTrack = preset.AppliesToTrack,
            AppliesToRelease = preset.AppliesToRelease,
        };

    private static BridgeExportPreset ExportPresetBridge(ExportPreset preset) =>
        new(
            preset.Id,
            preset.Name,
            preset.Codec,
            preset.Extension,
            preset.FilenameTemplate,
            preset.PregapPlacement,
            preset.AppliesToTrack,
            preset.AppliesToRelease);

    private static string DiscogsStatusTag(BridgeDiscogsTokenStatus status) =>
        status switch
        {
            BridgeDiscogsTokenStatus.Valid => "valid",
            BridgeDiscogsTokenStatus.Unvalidated => "unvalidated",
            BridgeDiscogsTokenStatus.Rejected => "rejected",
            _ => "not_configured",
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

    private static string MetadataSourceTag(BridgeMetadataSource source) =>
        source == BridgeMetadataSource.Discogs ? "discogs" : "musicbrainz";

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
            _ => "rejected",
        };


}

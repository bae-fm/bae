using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The import scan and identify operations — the C# mirror of the macOS-only
/// <c>Importer</c> service (BaeKit has no cross-platform counterpart). Wraps the
/// watched-folder scan, the candidate identify/re-identify/signal toggles, the
/// skip write, and the candidate-list read the import mirror seeds from. The
/// preview transport lives on <c>PlaybackService</c>; the search/prefetch/confirm
/// reads the import dialogs use are not yet here (those dialogs migrate with their
/// own story). Operations are async off the UI thread and carry the session-swap
/// currency plus the error line the bridge surfaces; every delegate defaults to a
/// fail-loud stub and <see cref="FromSession"/> is the production wiring.
/// </summary>
internal sealed class ImportService
{
    public Func<string, Task<(bool Current, (string[]? Folders, string? Error) Result)>> CandidateSourceFolders { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: CandidateSourceFolders not wired");

    public Func<BridgeImportCandidateDetail, ImportCandidate> ProjectFolderCandidate { get; init; }
        = _ => throw new InvalidOperationException(
            "ImportService stub: ProjectFolderCandidate not wired");

    /// <summary>The import tab's list, as one reconfigurable subscription: the
    /// view travels in the request, the windows are set as the list realizes
    /// rows, and each Next carries the windows plus the chrome around them.
    /// Null when the session moved on.</summary>
    public Func<BridgeImportListView, IImportListSubscription?> SubscribeImportList { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: SubscribeImportList not wired");

    /// <summary>One candidate as the pane reads it, and every later read of
    /// it. A null value means the key names no scanned folder any more, which
    /// is what clears a selection.</summary>
    public Func<string, Action<BridgeImportCandidateDetail?>, Action<Exception>, IDisposable?> SubscribeImportCandidate { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SubscribeImportCandidate not wired");

    /// <summary>What every candidate has in flight, keyed by candidate: a
    /// run's identify state and a running import's progress.</summary>
    public Func<Action<BridgeCandidateRuntimeChange>, IDisposable?> SubscribeCandidateRuntime { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: SubscribeCandidateRuntime not wired");

    /// <summary>What is in flight for one key right now — the read a control
    /// does once when it appears, after it has subscribed to the changes.
    /// </summary>
    public Func<string, BridgeCandidateRuntimeSnapshot?> CandidateRuntime { get; init; }
        = _ => null;

    /// <summary>What one key has in flight, as a control renders it: its
    /// status, the pressings the run is offering, and its badge row. A pure
    /// projection over what the stream already delivered — it needs no handle,
    /// so it stands whether or not a session is open.</summary>
    public Func<BridgeCandidateRuntimeSnapshot?, (
        ImportCandidateRowStatus? RowStatus,
        List<ReleaseCandidateChoice> Matches,
        List<SignalBadge> Signals)> ProjectRun
    { get; init; } = NativeBae.ImportRun;

    /// <summary>The pressing rows a set of album cards offers as choices, each
    /// already picked down to the release committing it applies. A pure
    /// projection over what the stream already delivered — it needs no handle,
    /// so it stands whether or not a session is open.</summary>
    public Func<IEnumerable<BridgeReleaseGroup>, List<ReleaseCandidateChoice>> GroupChoices
    { get; init; } = NativeBae.GroupChoices;

    /// <summary>Scan a folder into the watched set (clearing the prior scan);
    /// candidates and watched folders arrive through the import-candidate stream.
    /// Returns the error line, or null on success.</summary>
    public Func<string, Task<(bool Current, string? Error)>> ScanFolder { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: ScanFolder not wired");

    /// <summary>Drop a folder from the watched set.</summary>
    public Func<string, Task<(bool Current, string? Error)>> RemoveWatchedFolder { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: RemoveWatchedFolder not wired");
    public Func<string, Task<(bool Current, string? Error)>> RefreshWatchedFolder { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: RefreshWatchedFolder not wired");
    public Func<BridgeFolderReleaseDecisionKey, BridgeFolderReleaseDecision,
        Task<(bool Current, string? Error)>> SetFolderReleaseDecision
    { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SetFolderReleaseDecision not wired");

    /// <summary>Skip or un-skip a candidate; the import-candidate stream carries
    /// the updated row.</summary>
    public Func<string, bool, Task<(bool Current, string? Error)>> SetCandidateSkipped { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SetCandidateSkipped not wired");

    /// <summary>What a candidate's track sheet may be bound to: the folder's
    /// audio, each already offered or refused with core's reason.</summary>
    public Func<string, string, Task<(bool Current, (List<ImportSheetBindingOption>? Options, string? Error) Result)>> SheetBindingOptions { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SheetBindingOptions not wired");

    /// <summary>Name the audio a track sheet describes, or clear it with null.
    /// Core persists the decision and drops the candidate's stored identify
    /// verdict; the import-candidate stream carries both the new roles and a
    /// fresh identification.</summary>
    public Func<string, string, string?, Task<(bool Current, string? Error)>> SetSheetBinding { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SetSheetBinding not wired");

    /// <summary>Say which disc of the release a track sheet's entries are, or
    /// take them out of the tracklist. Cue filenames are arbitrary, so the
    /// assignment is the truth about which cue is which disc. Core persists it
    /// and drops the candidate's stored identify verdict, because a re-assigned
    /// sheet is a different tracklist.</summary>
    public Func<string, string, BridgeSheetDisc, Task<(bool Current, string? Error)>> SetSheetDisc { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SetSheetDisc not wired");
    /// <summary>Put one of a candidate's files in a role, or put it back in the
    /// one the scan proposed — what the roles table's control and a slot's
    /// Exclude action both call. Core persists it and clears the candidate's
    /// stored identify verdict; the import-candidate stream carries both the new
    /// roles and a fresh identification.</summary>
    public Func<string, string, BridgeFileRoleChoice, Task<(bool Current, string? Error)>> SetFileRole { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SetFileRole not wired");

    /// <summary>Identify an idle candidate after the person enters Lookup.</summary>
    public Func<string, Task<bool>> IdentifyFolderForLookup { get; init; }
        = _ => throw new InvalidOperationException(
            "ImportService stub: IdentifyFolderForLookup not wired");

    /// <summary>Re-dispatch a candidate's lookups, keeping the user's signal
    /// exclusions.</summary>
    public Func<string, Task<bool>> RerunIdentifyForCandidate { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: RerunIdentifyForCandidate not wired");

    /// <summary>Toggle a signal in or out of a candidate's triangulation.</summary>
    public Func<string, string, string, Task<bool>> ToggleSignalForCandidate { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: ToggleSignalForCandidate not wired");

    /// <summary>Start the auto-identify pipeline for a release under a candidate key
    /// (the re-identify dialog runs one against the release's own files).</summary>
    public Func<string, string, bool> AutoIdentifyRelease { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: AutoIdentifyRelease not wired");

    /// <summary>Stop the identify driver and any in-flight artwork OCR for a
    /// candidate key, on dialog close.</summary>
    public Func<string, bool> CancelAutoIdentify { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: CancelAutoIdentify not wired");

    /// <summary>Submit a candidate's typed search. Fire-and-forget: every
    /// configured provider is asked at once and each answer lands on the
    /// candidate's runtime, which the pane already watches.</summary>
    public Func<string, BridgeSearchQuery, bool> StartCandidateSearch { get; init; }
        = (_, _) => throw new InvalidOperationException(
            "ImportService stub: StartCandidateSearch not wired");

    /// <summary>Re-ask only the providers whose part of the search failed.</summary>
    public Func<string, bool> RetryCandidateSearch { get; init; }
        = _ => throw new InvalidOperationException(
            "ImportService stub: RetryCandidateSearch not wired");

    /// <summary>Drop a candidate's search, so its result area goes back to
    /// whatever identification has to say.</summary>
    public Func<string, bool> ClearCandidateSearch { get; init; }
        = _ => throw new InvalidOperationException(
            "ImportService stub: ClearCandidateSearch not wired");

    /// <summary>Replace the draft from an online release, claiming every
    /// source the pick carried.</summary>
    public Func<string, BridgeMetadataProvenance.ExternalRelease,
        Task<(bool Current, (ulong? Revision, string? Error) Result)>>
        ApplyCandidateExternalMetadata
    { get; init; }
        = (_, _) => throw new InvalidOperationException(
            "ImportService stub: ApplyCandidateExternalMetadata not wired");

    /// <summary>Replace the draft from the candidate's file tags.</summary>
    public Func<string, Task<(bool Current, (ulong? Revision, string? Error) Result)>>
        ApplyCandidateFileTags
    { get; init; }
        = _ => throw new InvalidOperationException(
            "ImportService stub: ApplyCandidateFileTags not wired");

    /// <summary>Clear the draft while preserving file mapping decisions.</summary>
    public Func<string, Task<(bool Current, (ulong? Revision, string? Error) Result)>>
        ClearCandidateMetadata
    { get; init; }
        = _ => throw new InvalidOperationException(
            "ImportService stub: ClearCandidateMetadata not wired");

    /// <summary>Read the folder's embedded tags without selecting them.</summary>
    public Func<string, Task<(bool Current, (BridgeReleaseUserEdit? Edit, string? Error) Result)>> PreviewFileTags { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: PreviewFileTags not wired");

    /// <summary>Replace the candidate's ordered album-artist assignments.</summary>
    public Func<string, IReadOnlyList<BridgeArtistAssignment>, Task<(bool Current, string? Error)>> SetCandidateAlbumArtists { get; init; }
        = (_, _) => throw new InvalidOperationException(
            "ImportService stub: SetCandidateAlbumArtists not wired");

    /// <summary>Record the cover this candidate commits with.</summary>
    public Func<string, BridgeCoverSelection, Task<(bool Current, string? Error)>> SetCandidateCover { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SetCandidateCover not wired");

    /// <summary>Record one album-level metadata field as the user left it.</summary>
    public Func<string, BridgeCandidateEditField, string, Task<(bool Current, string? Error)>> SetCandidateEditField { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SetCandidateEditField not wired");

    /// <summary>Record one mapping-table row as the user left it.</summary>
    public Func<string, BridgeRawTrackEdit, Task<(bool Current, string? Error)>> SetCandidateTrackEdit { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SetCandidateTrackEdit not wired");

    /// <summary>Replace the artist assignments of the named mapping-table rows
    /// in one commit.</summary>
    public Func<string, IReadOnlyList<string>, BridgeTrackArtistAssignments,
        Task<(bool Current, string? Error)>> SetCandidateTrackArtists
    { get; init; }
        = (_, _, _) => throw new InvalidOperationException(
            "ImportService stub: SetCandidateTrackArtists not wired");

    /// <summary>Take one mapping-table row out of the import.</summary>
    public Func<string, string, Task<(bool Current, string? Error)>> DropCandidateTrack { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: DropCandidateTrack not wired");

    /// <summary>Seed the import confirm form for a skip-identify import: the folder's
    /// embedded file tags projected into the edit form, with only the folder's local
    /// artwork offered (no source release).</summary>
    /// <summary>The chosen source release's current library membership. The
    /// confirm pane keeps this subscription for as long as that release is selected.</summary>
    public Func<BridgeMetadataSource, string, string?, Action<BridgeLibraryStatus>, Action<Exception>, IDisposable?> SubscribeReleaseLibraryStatus { get; init; }
        = (_, _, _, _, _) => throw new InvalidOperationException("ImportService stub: SubscribeReleaseLibraryStatus not wired");

    /// <summary>Commit the import of a candidate at the storage the pane chose.
    /// Everything about the release is stored under the candidate, so the
    /// commit reads the very values the pane drew. The import runs in the
    /// background; its result updates the candidate row and catalog
    /// subscriptions. Returns the error line, or null on accept.</summary>
    public Func<string, string, bool, Task<(bool Current, string? Error)>> CommitImport { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: CommitImport not wired");

    /// <summary>Decode a candidate's evidence file (CUE sheet, rip log, info text) to
    /// its text for the document viewer, or its read error. A handle-less file read.</summary>
    public static (string? Text, string? Error) ReadDocumentText(string path) =>
        NativeBae.ReadTextFile(path);

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static ImportService FromSession(SessionStore session) => new()
    {
        ProjectFolderCandidate = NativeBae.ImportCandidateRow,
        CandidateSourceFolders = key => session.RunForCurrentHandle(
            handle => NativeBae.CandidateSourceFolders(handle, key)),
        CandidateRuntime = candidateKey =>
        {
            var (current, runtime) = session.WithCurrentHandle(handle =>
                NativeBae.CandidateRuntime(handle, candidateKey));
            return current ? runtime : null;
        },
        SubscribeImportList = view =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeImportList(handle, view));
            return current ? subscription : null;
        },
        SubscribeImportCandidate = (candidateKey, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeImportCandidate(handle, candidateKey, onValue, onError));
            return current ? subscription : null;
        },
        SubscribeCandidateRuntime = onChange =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeCandidateRuntime(handle, onChange));
            return current ? subscription : null;
        },
        ScanFolder = path => session.RunForCurrentHandle(handle => NativeBae.ScanFolder(handle, path, true)),
        RemoveWatchedFolder = path =>
            session.RunForCurrentHandle(handle => NativeBae.RemoveWatchedFolder(handle, path)),
        RefreshWatchedFolder = path =>
            session.RunForCurrentHandle(handle => NativeBae.RefreshWatchedFolder(handle, path)),
        SetFolderReleaseDecision = (key, decision) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.SetFolderReleaseDecision(handle, key, decision)),
        SetCandidateSkipped = (path, skipped) =>
            session.RunForCurrentHandle(handle => NativeBae.SetCandidateSkipped(handle, path, skipped)),
        SheetBindingOptions = (candidateKey, sheetFileId) =>
            session.RunForCurrentHandle(handle => NativeBae.SheetBindingOptions(handle, candidateKey, sheetFileId)),
        SetSheetBinding = (candidateKey, sheetFileId, audioFileId) =>
            session.RunForCurrentHandle(handle => NativeBae.SetSheetBinding(handle, candidateKey, sheetFileId, audioFileId)),
        SetSheetDisc = (candidateKey, sheetFileId, disc) =>
            session.RunForCurrentHandle(handle => NativeBae.SetSheetDisc(handle, candidateKey, sheetFileId, disc)),
        SetFileRole = (candidateKey, fileId, choice) =>
            session.RunForCurrentHandle(handle => NativeBae.SetFileRole(handle, candidateKey, fileId, choice)),
        IdentifyFolderForLookup = candidateKey =>
            session.RunForCurrentHandle(handle =>
                NativeBae.IdentifyFolderForLookup(handle, candidateKey)),
        RerunIdentifyForCandidate = candidateKey =>
            session.RunForCurrentHandle(handle => NativeBae.RerunIdentifyForCandidate(handle, candidateKey)),
        ToggleSignalForCandidate = (candidateKey, kind, value) =>
            session.RunForCurrentHandle(handle => NativeBae.ToggleSignalForCandidate(handle, candidateKey, kind, value)),
        AutoIdentifyRelease = (candidateKey, releaseId) =>
            session.WithCurrentHandle(handle => NativeBae.AutoIdentifyRelease(handle, candidateKey, releaseId)),
        CancelAutoIdentify = candidateKey =>
            session.WithCurrentHandle(handle => NativeBae.CancelAutoIdentify(handle, candidateKey)),
        StartCandidateSearch = (candidateKey, query) =>
            session.WithCurrentHandle(handle =>
                NativeBae.StartCandidateSearch(handle, candidateKey, query)),
        RetryCandidateSearch = candidateKey =>
            session.WithCurrentHandle(handle =>
                NativeBae.RetryCandidateSearch(handle, candidateKey)),
        ClearCandidateSearch = candidateKey =>
            session.WithCurrentHandle(handle =>
                NativeBae.ClearCandidateSearch(handle, candidateKey)),
        ApplyCandidateExternalMetadata = (candidateKey, provenance) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.ApplyCandidateExternalMetadata(
                    handle, candidateKey, provenance)),
        ApplyCandidateFileTags = candidateKey =>
            session.RunForCurrentHandle(handle =>
                NativeBae.ApplyCandidateFileTags(handle, candidateKey)),
        ClearCandidateMetadata = candidateKey =>
            session.RunForCurrentHandle(handle =>
                NativeBae.ClearCandidateMetadata(handle, candidateKey)),
        PreviewFileTags = candidateKey =>
            session.RunForCurrentHandle(handle =>
                NativeBae.PreviewFileTags(handle, candidateKey)),
        SetCandidateAlbumArtists = (candidateKey, assignments) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.SetCandidateAlbumArtists(
                    handle, candidateKey, assignments)),
        SetCandidateCover = (candidateKey, cover) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.SetCandidateCover(handle, candidateKey, cover)),
        SetCandidateEditField = (candidateKey, field, value) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.SetCandidateEditField(handle, candidateKey, field, value)),
        SetCandidateTrackEdit = (candidateKey, track) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.SetCandidateTrackEdit(handle, candidateKey, track)),
        SetCandidateTrackArtists = (candidateKey, trackIds, assignments) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.SetCandidateTrackArtists(
                    handle, candidateKey, trackIds, assignments)),
        DropCandidateTrack = (candidateKey, trackId) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.DropCandidateTrack(handle, candidateKey, trackId)),
        SubscribeReleaseLibraryStatus = (source, releaseId, sourceGroupId, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeReleaseLibraryStatus(
                    handle, source, releaseId, sourceGroupId, onValue, onError));
            return current ? subscription : null;
        },
        CommitImport = (candidateKey, storageMode, pin) =>
            session.RunForCurrentHandle(handle =>
                NativeBae.ImportCandidate(handle, candidateKey, storageMode, pin)),
    };
}

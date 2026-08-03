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
    /// <summary>The sidebar's pre-shaped triage sections and tab counts — core's
    /// projection. Read again on an import-domain invalidation; the UI iterates
    /// and renders it without computing tab or group membership itself.</summary>
    public Func<Task<(bool Current, (BridgeTriageQueue? Queue, string? Error) Result)>> TriageQueue { get; init; }
        = () => throw new InvalidOperationException("ImportService stub: TriageQueue not wired");

    /// <summary>The folders currently watched for imports, for the sidebar's "+"
    /// menu.</summary>
    public Func<(bool Current, List<BridgeWatchedFolder> Folders)> WatchedFolders { get; init; }
        = () => throw new InvalidOperationException("ImportService stub: WatchedFolders not wired");

    /// <summary>The full candidate for one triage row's key — signals, matches,
    /// audio paths, and documents the row itself doesn't carry — fetched fresh
    /// when a row's picker opens.</summary>
    public Func<string, (bool Current, ImportCandidate? Candidate)> CandidateForKey { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: CandidateForKey not wired");

    /// <summary>Scan a folder into the watched set (clearing the prior scan);
    /// candidates stream in through invalidations. Returns the error line, or null
    /// on success.</summary>
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

    /// <summary>Skip or un-skip a candidate; the candidate invalidation re-tabs the
    /// row.</summary>
    public Func<string, bool, Task<(bool Current, string? Error)>> SetCandidateSkipped { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SetCandidateSkipped not wired");

    /// <summary>What a candidate's track sheet may be bound to: the folder's
    /// audio, each already offered or refused with core's reason.</summary>
    public Func<string, string, Task<(bool Current, (List<ImportSheetBindingOption>? Options, string? Error) Result)>> SheetBindingOptions { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SheetBindingOptions not wired");

    /// <summary>Name the audio a track sheet describes, or clear it with null.
    /// Core persists the decision and drops the candidate's stored identify
    /// verdict, so the candidate invalidation brings back both the new roles and
    /// a fresh identification.</summary>
    public Func<string, string, string?, Task<(bool Current, string? Error)>> SetSheetBinding { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SetSheetBinding not wired");

    /// <summary>Say which disc of the release a track sheet's entries are, or
    /// take them out of the tracklist. Cue filenames are arbitrary, so the
    /// assignment is the truth about which cue is which disc. Core persists it
    /// and drops the candidate's stored identify verdict, because a re-assigned
    /// sheet is a different tracklist.</summary>
    public Func<string, string, BridgeSheetDisc, Task<(bool Current, string? Error)>> SetSheetDisc { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SetSheetDisc not wired");

    /// <summary>The mapping table for a candidate nobody has picked a release
    /// for: every source unit the folder offers, with what it becomes left open.
    /// Synchronous — a read of the folder the scan already categorized.</summary>
    public Func<string, (bool Current, (BridgeMappingTable? Mapping, string? Error) Result)> CandidateMapping { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: CandidateMapping not wired");

    /// <summary>Put one of a candidate's files in a role, or put it back in the
    /// one the scan proposed — what the roles table's control and a slot's
    /// Exclude action both call. Core persists it and clears the candidate's
    /// stored identify verdict, so the candidate invalidation brings back both
    /// the new roles and a fresh identification.</summary>
    public Func<string, string, BridgeFileRoleChoice, Task<(bool Current, string? Error)>> SetFileRole { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SetFileRole not wired");

    /// <summary>Kick off auto-identification for an as-yet unidentified candidate.</summary>
    public Func<string, Task<bool>> AutoIdentifyFolder { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: AutoIdentifyFolder not wired");

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

    /// <summary>Read the identify pipeline snapshot for a candidate key: the
    /// localizable row status, the found pressings, and the signals-toolbar badges.
    /// Null until the pipeline's first event lands. Synchronous — the dialog reads it
    /// on each candidate invalidation.</summary>
    public Func<string, (bool Current, (ImportCandidateRowStatus RowStatus, List<ReleaseCandidateChoice> Matches, List<SignalBadge> Signals)? Snapshot)> ReidentifyPipeline { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: ReidentifyPipeline not wired");

    /// <summary>What picking a release under a candidate claims, and where its
    /// metadata comes from. The re-identify dialog's path: it commits straight
    /// from the picked row, so it never prefetches. Synchronous — a read of the
    /// candidate's already-settled identify state.</summary>
    public Func<string, BridgeMetadataResult, BridgeClaimLevel, (bool Current, BridgeClaimLine? Claim)> ClaimForPick { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: ClaimForPick not wired");

    /// <summary>Manual metadata search (the re-identify dialog's fallback and the
    /// import confirm's search). Async — it blocks on network / DB.</summary>
    public Func<string, string, string, Task<(bool Current, (List<ReleaseCandidateChoice>? Candidates, string? Error) Result)>> SearchReleases { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SearchReleases not wired");

    /// <summary>Seed the import confirm form from a picked source release: the
    /// editor edit (already masked for the claim), the claim the pick implies,
    /// the source's remote covers, and the folder's local artwork. The candidate
    /// key is what lets bae-core derive the claim from that candidate's identify
    /// evidence.</summary>
    /// <summary>Decide the candidate's identity: persist the choice and come
    /// back with the seeded edit the pane renders.</summary>
    public Func<string, BridgeIdentityPick, Task<(bool Current, (DecidedEdit? Decided, string? Error) Result)>> PickCandidateIdentity { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: PickCandidateIdentity not wired");

    /// <summary>The candidate's decided identity read back; `Undecided` when
    /// nothing is decided, which is a result rather than an error.</summary>
    public Func<string, Task<(bool Current, (DecidedEdit? Decided, bool Undecided, string? Error) Result)>> CandidateDecidedIdentity { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: CandidateDecidedIdentity not wired");

    /// <summary>Seed the import confirm form for a skip-identify import: the folder's
    /// embedded file tags projected into the edit form, with only the folder's local
    /// artwork offered (no source release).</summary>
    /// <summary>Whether a chosen release is already in the library, so the confirm
    /// dialog can warn before importing a duplicate (advisory, not a gate).</summary>
    public Func<string, Task<(bool Current, (BridgeLibraryStatus? Status, string? Error) Result)>> CheckReleaseInLibrary { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: CheckReleaseInLibrary not wired");

    /// <summary>Commit the import of a candidate with the confirm dialog's edits,
    /// storage mode, pin, chosen identity, and cover overlaid. The import runs in the
    /// background; its result updates the candidate row and grid through
    /// invalidations. Returns the validation error line, or null on accept.</summary>
    public Func<string, string, BridgeIdentityChoice, string, bool, BridgeRawReleaseEdit, BridgeCoverSelection?, Task<(bool Current, string? Error)>> CommitImport { get; init; }
        = (_, _, _, _, _, _, _) => throw new InvalidOperationException("ImportService stub: CommitImport not wired");

    /// <summary>Decode a candidate's evidence file (CUE sheet, rip log, info text) to
    /// its text for the document viewer, or its read error. A handle-less file read.</summary>
    public static (string? Text, string? Error) ReadDocumentText(string path) =>
        NativeBae.ReadTextFile(path);

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static ImportService FromSession(SessionStore session) => new()
    {
        TriageQueue = () => session.RunForCurrentHandle(NativeBae.ImportTriageQueue),
        WatchedFolders = () => session.WithCurrentHandle(NativeBae.WatchedFolders),
        CandidateForKey = key => session.WithCurrentHandle(handle => NativeBae.Candidate(handle, key)),
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
        CandidateMapping = candidateKey =>
            session.WithCurrentHandle(handle => NativeBae.CandidateMapping(handle, candidateKey)),
        SetFileRole = (candidateKey, fileId, choice) =>
            session.RunForCurrentHandle(handle => NativeBae.SetFileRole(handle, candidateKey, fileId, choice)),
        AutoIdentifyFolder = candidateKey =>
            session.RunForCurrentHandle(handle => NativeBae.AutoIdentifyFolder(handle, candidateKey)),
        RerunIdentifyForCandidate = candidateKey =>
            session.RunForCurrentHandle(handle => NativeBae.RerunIdentifyForCandidate(handle, candidateKey)),
        ToggleSignalForCandidate = (candidateKey, kind, value) =>
            session.RunForCurrentHandle(handle => NativeBae.ToggleSignalForCandidate(handle, candidateKey, kind, value)),
        AutoIdentifyRelease = (candidateKey, releaseId) =>
            session.WithCurrentHandle(handle => NativeBae.AutoIdentifyRelease(handle, candidateKey, releaseId)),
        CancelAutoIdentify = candidateKey =>
            session.WithCurrentHandle(handle => NativeBae.CancelAutoIdentify(handle, candidateKey)),
        ReidentifyPipeline = candidateKey =>
            session.WithCurrentHandle(handle => NativeBae.ReidentifyPipeline(handle, candidateKey)),
        SearchReleases = (source, artist, album) =>
            session.RunForCurrentHandle(handle => NativeBae.SearchReleases(handle, source, artist, album)),
        ClaimForPick = (candidateKey, result, level) =>
            session.WithCurrentHandle(handle => NativeBae.ClaimForPick(handle, candidateKey, result, level)),
        PickCandidateIdentity = (candidateKey, pick) =>
            session.RunForCurrentHandle(handle => NativeBae.PickCandidateIdentity(handle, candidateKey, pick)),
        CandidateDecidedIdentity = candidateKey =>
            session.RunForCurrentHandle(handle => NativeBae.CandidateDecidedIdentity(handle, candidateKey)),
        CheckReleaseInLibrary = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.CheckReleaseInLibrary(handle, releaseId)),
        CommitImport = (candidateKey, folderPath, identity, storageMode, pin, userEdit, cover) =>
            session.RunForCurrentHandle(handle => NativeBae.ImportCandidate(
                handle, candidateKey, identity, storageMode, pin, userEdit, cover)),
    };
}

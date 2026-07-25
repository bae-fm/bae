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
    /// <summary>The scanned candidates in core snapshot order plus the watched
    /// folders behind them — the baseline the import mirror tabs and sorts.</summary>
    public Func<Task<(bool Current, (List<ImportCandidate> Rows, BridgeWatchedFolder[] Folders) Result)>> ImportCandidates { get; init; }
        = () => throw new InvalidOperationException("ImportService stub: ImportCandidates not wired");

    /// <summary>Scan a folder into the watched set (clearing the prior scan);
    /// candidates stream in through invalidations. Returns the error line, or null
    /// on success.</summary>
    public Func<string, Task<(bool Current, string? Error)>> ScanFolder { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: ScanFolder not wired");

    /// <summary>Drop a folder from the watched set.</summary>
    public Func<string, Task<(bool Current, string? Error)>> RemoveWatchedFolder { get; init; }
        = _ => throw new InvalidOperationException("ImportService stub: RemoveWatchedFolder not wired");

    /// <summary>Skip or un-skip a candidate; the candidate invalidation re-tabs the
    /// row.</summary>
    public Func<string, bool, Task<(bool Current, string? Error)>> SetCandidateSkipped { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: SetCandidateSkipped not wired");

    /// <summary>Kick off auto-identification for an as-yet unidentified candidate.</summary>
    public Func<string, string, Task<bool>> AutoIdentifyFolder { get; init; }
        = (_, _) => throw new InvalidOperationException("ImportService stub: AutoIdentifyFolder not wired");

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

    /// <summary>Manual metadata search (the re-identify dialog's fallback and the
    /// import confirm's search). Async — it blocks on network / DB.</summary>
    public Func<string, string, string, Task<(bool Current, (List<ReleaseCandidateChoice>? Candidates, string? Error) Result)>> SearchReleases { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ImportService stub: SearchReleases not wired");

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static ImportService FromSession(SessionStore session) => new()
    {
        ImportCandidates = () => session.RunForCurrentHandle(NativeBae.ImportCandidates),
        ScanFolder = path => session.RunForCurrentHandle(handle => NativeBae.ScanFolder(handle, path, true)),
        RemoveWatchedFolder = path =>
            session.RunForCurrentHandle(handle => NativeBae.RemoveWatchedFolder(handle, path)),
        SetCandidateSkipped = (path, skipped) =>
            session.RunForCurrentHandle(handle => NativeBae.SetCandidateSkipped(handle, path, skipped)),
        AutoIdentifyFolder = (candidateKey, folderPath) =>
            session.RunForCurrentHandle(handle => NativeBae.AutoIdentifyFolder(handle, candidateKey, folderPath)),
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
    };
}

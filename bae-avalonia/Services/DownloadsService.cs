using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Controls for the in-memory download (pin) queue — the C# mirror of BaeKit's
/// <c>Downloads</c> closure-struct: enqueue releases to pin, unpin, pause/resume
/// the queue, cancel a release's download, retry failed ones, and the persisted
/// concurrency write. The queue itself lives in bae-core; the Downloads pane reads
/// its state from the download projection. Commands carry the session-swap
/// currency the Windows session exposes (whether a handle was current, or the
/// error line for the ones that surface one). Every delegate defaults to a
/// fail-loud stub; <see cref="FromSession"/> is the production wiring.
/// </summary>
internal sealed class DownloadsService
{
    /// <summary>Enqueue releases to pin locally; they join the serial download
    /// queue. Fire-and-forget — progress arrives via download-queue events.</summary>
    public Func<IReadOnlyList<string>, Task<(bool Current, string? Error)>> QueuePins { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: QueuePins not wired");

    /// <summary>Unpin a release: its blobs move to the evictable cache; the release
    /// stays remote and playable.</summary>
    public Func<string, Task<(bool Current, string? Error)>> UnpinRelease { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: UnpinRelease not wired");

    /// <summary>Pause or resume the download queue. In-flight downloads finish; the
    /// queue stops starting new ones until resumed.</summary>
    public Func<bool, bool> SetDownloadsPaused { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: SetDownloadsPaused not wired");

    /// <summary>Cancel a release's download — drops a queued/failed entry or aborts
    /// the in-flight one (the release stays cloud-only).</summary>
    public Func<string, bool> CancelDownload { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: CancelDownload not wired");

    /// <summary>Retry every failed download now (flips them back to queued).</summary>
    public Func<bool> RetryDownloads { get; init; }
        = () => throw new InvalidOperationException("DownloadsService stub: RetryDownloads not wired");

    /// <summary>How many downloads a pin fetches at once (1..8). A persisted
    /// device-local config write; it surfaces an error on an out-of-range value or a
    /// failed write.</summary>
    public Func<uint, (bool Current, string? Error)> SetMaxConcurrentDownloads { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: SetMaxConcurrentDownloads not wired");

    /// <summary>Pause or resume the output (save/export) queue — the sibling of
    /// the download queue on the storage sheet's Exporting band.</summary>
    public Func<bool, bool> SetOutputsPaused { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: SetOutputsPaused not wired");

    /// <summary>Cancel a release's in-flight or queued output run.</summary>
    public Func<string, bool> CancelOutput { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: CancelOutput not wired");

    /// <summary>Retry every failed output now (flips them back to queued).</summary>
    public Func<bool> RetryOutputs { get; init; }
        = () => throw new InvalidOperationException("DownloadsService stub: RetryOutputs not wired");

    /// <summary>Move a release to cloud storage. With <c>pin</c> it also keeps a
    /// pinned copy. The macOS ReleaseEditor's moveReleaseToCloud analog; folded
    /// here beside pin/unpin since Windows has no
    /// release-editor service yet.</summary>
    public Func<string, bool, Task<(bool Current, string? Error)>> MakeReleaseRemote { get; init; }
        = (_, _) => throw new InvalidOperationException("DownloadsService stub: MakeReleaseRemote not wired");

    /// <summary>Make a release local at a destination folder. The macOS
    /// ReleaseEditor's makeReleaseLocal analog.</summary>
    public Func<string, string, Task<(bool Current, string? Error)>> MakeReleaseLocal { get; init; }
        = (_, _) => throw new InvalidOperationException("DownloadsService stub: MakeReleaseLocal not wired");

    /// <summary>The in-memory download (pin) queue snapshot — which releases are
    /// queued or downloading — for the storage sheet's pin-cancel availability.</summary>
    public Func<Task<(bool Current, (BridgeDownloadSnapshot? Snapshot, string? Error) Result)>> DownloadSnapshot { get; init; }
        = () => throw new InvalidOperationException("DownloadsService stub: DownloadSnapshot not wired");

    /// <summary>The output (save/export) queue snapshot — the queued and in-flight
    /// exports — for the storage sheet's Exporting band. Infallible bar a dropped
    /// handle, so it carries currency but no error line.</summary>
    public Func<Task<(bool Current, BridgeOutputSnapshot Snapshot)>> OutputSnapshot { get; init; }
        = () => throw new InvalidOperationException("DownloadsService stub: OutputSnapshot not wired");

    /// <summary>Replace the configured export presets whole (the settings Formats
    /// section sends the entire set, never one mutated field); returns the error
    /// line, or null on success.</summary>
    public Func<IReadOnlyList<SavePreset>, Task<(bool Current, string? Error)>> SetSavePresets { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: SetSavePresets not wired");

    /// <summary>Set the default save preset for a track-scope save; returns the
    /// error line, or null on success.</summary>
    public Func<string, Task<(bool Current, string? Error)>> SetDefaultTrackSavePreset { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: SetDefaultTrackSavePreset not wired");

    /// <summary>Set the default save preset for a release-scope save; returns the
    /// error line, or null on success.</summary>
    public Func<string, Task<(bool Current, string? Error)>> SetDefaultReleaseSavePreset { get; init; }
        = _ => throw new InvalidOperationException("DownloadsService stub: SetDefaultReleaseSavePreset not wired");

    /// <summary>Wire every control through the open session's current handle.</summary>
    public static DownloadsService FromSession(SessionStore session) => new()
    {
        QueuePins = releaseIds => session.RunForCurrentHandle(handle => NativeBae.PinReleases(handle, releaseIds)),
        UnpinRelease = releaseId => session.RunForCurrentHandle(handle => NativeBae.UnpinRelease(handle, releaseId)),
        SetDownloadsPaused = paused => session.WithCurrentHandle(handle => NativeBae.SetDownloadsPaused(handle, paused)),
        CancelDownload = releaseId => session.WithCurrentHandle(handle => NativeBae.CancelDownload(handle, releaseId)),
        RetryDownloads = () => session.WithCurrentHandle(NativeBae.RetryDownloads),
        SetMaxConcurrentDownloads = n => session.WithCurrentHandle(handle => NativeBae.SetMaxConcurrentDownloads(handle, n)),
        SetOutputsPaused = paused => session.WithCurrentHandle(handle => NativeBae.SetOutputsPaused(handle, paused)),
        CancelOutput = releaseId => session.WithCurrentHandle(handle => NativeBae.CancelOutput(handle, releaseId)),
        RetryOutputs = () => session.WithCurrentHandle(NativeBae.RetryOutputs),
        MakeReleaseRemote = (releaseId, pin) =>
            session.RunForCurrentHandle(handle => NativeBae.MakeReleaseRemote(handle, releaseId, pin)),
        MakeReleaseLocal = (releaseId, newPath) =>
            session.RunForCurrentHandle(handle => NativeBae.MakeReleaseLocal(handle, releaseId, newPath)),
        DownloadSnapshot = () => session.RunForCurrentHandle(NativeBae.DownloadSnapshot),
        OutputSnapshot = () => session.RunForCurrentHandle(NativeBae.OutputSnapshot),
        SetSavePresets = presets => session.RunForCurrentHandle(handle => NativeBae.SetSavePresets(handle, presets)),
        SetDefaultTrackSavePreset = presetId =>
            session.RunForCurrentHandle(handle => NativeBae.SetDefaultTrackSavePreset(handle, presetId)),
        SetDefaultReleaseSavePreset = presetId =>
            session.RunForCurrentHandle(handle => NativeBae.SetDefaultReleaseSavePreset(handle, presetId)),
    };
}

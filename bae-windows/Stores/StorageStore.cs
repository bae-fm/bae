using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The storage sheet's non-UI operations: which of a selection's releases have a
// transition in flight, the transitions they all allow (intersected), and running
// a transition across the selection. Also tracks the releases whose unmanage is
// running right now — a blocking foreground transfer (unlike pin, which enqueues,
// or upload, which lives in the outbox), so it has no queue snapshot to read; the
// row offers to cancel it from this set while RunStorageActionForReleases awaits.
// UI-thread only.
internal sealed class StorageStore
{
    private readonly DownloadsService _downloads;
    private readonly SyncService _sync;
    private readonly TransferProgressStore _transfers;
    private readonly HashSet<string> _unmanagingReleases = new();

    public StorageStore(DownloadsService downloads, SyncService sync, TransferProgressStore transfers)
    {
        _downloads = downloads;
        _sync = sync;
        _transfers = transfers;
    }

    public bool IsUnmanaging(string releaseId) => _unmanagingReleases.Contains(releaseId);

    // Of the given releases, those with a transition in flight: an outbox upload,
    // a queued/downloading pin, a running unmanage, or an event-reported transfer
    // in the overlay. A release in this set offers only Cancel — the storage
    // actions would race the transition. Returns the outbox read error, if any,
    // so the caller can surface it like the panel load does.
    public async System.Threading.Tasks.Task<(HashSet<string> Transitioning, string? Error)> TransitioningReleases(
        List<string> releaseIds)
    {
        var (uploading, uploadError) = await UploadingReleases(releaseIds);
        var transitioning = new HashSet<string>(uploading);
        transitioning.UnionWith(await DownloadingReleases(releaseIds));
        transitioning.UnionWith(releaseIds.Where(IsUnmanaging));
        transitioning.UnionWith(releaseIds.Where(id => _transfers.TokenFor(id) is not null));
        return (transitioning, uploadError);
    }

    // The transitions every release in the selection allows, intersected so the
    // menu only offers actions applicable to all. Order follows the first release's
    // action list (core's order); a release not in the current rows contributes an
    // empty set, so the intersection is empty.
    public List<BridgeReleaseStorageAction> IntersectedStorageActions(
        List<string> releaseIds, Dictionary<string, BridgeStorageRow> rowsById)
    {
        var perRelease = releaseIds
            .Select(id => (IReadOnlyList<BridgeReleaseStorageAction>)(
                rowsById.TryGetValue(id, out var row)
                    ? row.Release.StorageActions
                    : Array.Empty<BridgeReleaseStorageAction>()))
            .ToList();
        return OrderedIntersection.Intersect<BridgeReleaseStorageAction>(perRelease);
    }

    // Of the given releases, those with uploads queued or in flight. Core omits
    // idle releases from the per-release map, so presence there is the signal.
    // Returns the error line when the outbox couldn't be read.
    public async System.Threading.Tasks.Task<(List<string> Releases, string? Error)> UploadingReleases(
        List<string> releaseIds)
    {
        var (current, result) = await _sync.OutboxSnapshot();
        if (!current)
        {
            return (new List<string>(), null);
        }
        if (result.Error is not null)
        {
            return (new List<string>(), result.Error);
        }
        var snapshot = result.Snapshot;
        if (snapshot is null)
        {
            // Couldn't read the outbox; surface it like the panel load does rather
            // than silently dropping the cancel action.
            return (new List<string>(), Loc.Chrome("outbox.read_failed"));
        }

        return (releaseIds.Where(snapshot.PerRelease.ContainsKey).ToList(), null);
    }

    // Of the given releases, those queued or downloading in the pin queue. The pin
    // queue is in-memory and read is infallible bar a dropped handle, so a failure
    // is an app-state fault — log it and offer no pin-cancel rather than a toast.
    public async System.Threading.Tasks.Task<List<string>> DownloadingReleases(List<string> releaseIds)
    {
        var (current, result) = await _downloads.DownloadSnapshot();
        if (!current)
        {
            return new List<string>();
        }
        if (result.Error is not null)
        {
            BaeDiagnostics.Logger.Warning(
                $"couldn't read the download snapshot; pin-cancel unavailable: {result.Error}");
            return new List<string>();
        }
        var snapshot = result.Snapshot;
        if (snapshot is null)
        {
            BaeDiagnostics.Logger.Warning(
                "couldn't read the download snapshot; pin-cancel unavailable");
            return new List<string>();
        }

        var pinning = snapshot.Downloads.Select(op => op.ReleaseId).ToHashSet();
        return releaseIds.Where(pinning.Contains).ToList();
    }

    // Run a storage transition on every release in the selection off the UI thread.
    // "unmanage" asks once for a destination folder (via pickFolder), then moves
    // each release into it, marking them as unmanaging so a right-click can cancel
    // the blocking transfer while it runs. Returns null on success (or a cancelled
    // picker), else the first error message.
    public async System.Threading.Tasks.Task<string?> RunStorageActionForReleases(
        BridgeReleaseStorageAction action,
        List<string> releaseIds,
        Func<System.Threading.Tasks.Task<string?>> pickFolder)
    {
        if (action == BridgeReleaseStorageAction.MakeLocal)
        {
            var path = await pickFolder();
            if (path is null)
            {
                return null;
            }

            foreach (var releaseId in releaseIds)
            {
                _unmanagingReleases.Add(releaseId);
            }
            try
            {
                return await FirstErrorAcross(
                    releaseIds, releaseId => _downloads.MakeReleaseLocal(releaseId, path));
            }
            finally
            {
                foreach (var releaseId in releaseIds)
                {
                    _unmanagingReleases.Remove(releaseId);
                }
            }
        }

        switch (action)
        {
            case BridgeReleaseStorageAction.Pin:
                // Pin enqueues the whole selection in one call (the download queue
                // takes a list); the rest apply per release.
                var (pinCurrent, pinError) = await _downloads.QueuePins(releaseIds);
                return pinCurrent ? pinError : null;
            case BridgeReleaseStorageAction.Unpin:
                return await FirstErrorAcross(releaseIds, _downloads.UnpinRelease);
            case BridgeReleaseStorageAction.MakeRemote:
                // Manage without a local pinned copy (false).
                return await FirstErrorAcross(
                    releaseIds, releaseId => _downloads.MakeReleaseRemote(releaseId, false));
            case BridgeReleaseStorageAction.MakeLocal:
                throw new InvalidOperationException(
                    "make-local storage actions must choose a destination before running");
            default:
                throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action");
        }
    }

    // Run a per-release transition across the selection, stopping at the first
    // error and returning its line (or null on success / no handle). Each release
    // is applied under its own handle acquisition; on the UI thread they all see
    // the same handle, so this matches the prior single-batch result.
    private static async System.Threading.Tasks.Task<string?> FirstErrorAcross(
        List<string> releaseIds,
        Func<string, System.Threading.Tasks.Task<(bool Current, string? Error)>> apply)
    {
        foreach (var releaseId in releaseIds)
        {
            var (current, error) = await apply(releaseId);
            if (current && error is not null)
            {
                return error;
            }
        }

        return null;
    }
}

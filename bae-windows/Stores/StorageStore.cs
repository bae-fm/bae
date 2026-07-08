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
    private readonly SessionStore _session;
    private readonly HashSet<string> _unmanagingReleases = new();

    public StorageStore(SessionStore session)
    {
        _session = session;
    }

    public bool IsUnmanaging(string releaseId) => _unmanagingReleases.Contains(releaseId);

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
        var (current, result) = await _session.RunForCurrentHandle(NativeBae.OutboxSnapshot);
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
        var (current, result) = await _session.RunForCurrentHandle(NativeBae.DownloadSnapshot);
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
                var (storageActionCurrent, error) = await _session.RunForCurrentHandle(handle =>
                {
                    foreach (var releaseId in releaseIds)
                    {
                        var error = NativeBae.MakeReleaseLocal(handle, releaseId, path);
                        if (error is not null)
                        {
                            return error;
                        }
                    }

                    return (string?)null;
                });
                return storageActionCurrent ? error : null;
            }
            finally
            {
                foreach (var releaseId in releaseIds)
                {
                    _unmanagingReleases.Remove(releaseId);
                }
            }
        }

        var (current, actionError) = await _session.RunForCurrentHandle(handle =>
        {
            foreach (var releaseId in releaseIds)
            {
                var error = action switch
                {
                    BridgeReleaseStorageAction.Pin => NativeBae.PinRelease(handle, releaseId),
                    BridgeReleaseStorageAction.Unpin => NativeBae.UnpinRelease(handle, releaseId),
                    BridgeReleaseStorageAction.MakeRemote => NativeBae.MakeReleaseRemote(handle, releaseId, pin: false),
                    BridgeReleaseStorageAction.MakeLocal => throw new InvalidOperationException(
                        "make-local storage actions must choose a destination before running"),
                    _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
                };
                if (error is not null)
                {
                    return error;
                }
            }

            return (string?)null;
        });
        return current ? actionError : null;
    }
}

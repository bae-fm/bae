using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The storage sheet's non-UI operations: which of a selection's releases have a
// transition in flight, the transitions they all allow (intersected), and running
// a transition across the selection. Database rows and runtime queue snapshots
// carry the current values used for those decisions.
// UI-thread only.
internal sealed class StorageStore
{
    private readonly DownloadsService _downloads;
    private readonly Func<bool> _loadPinPreference;
    private BridgeOutboxSnapshot? _outbox;
    private BridgeDownloadSnapshot? _downloadsSnapshot;
    private BridgeOutputSnapshot? _outputSnapshot;
    private readonly Dictionary<string, CloudUploadHandoff> _cloudUploadHandoffs = new();
    private readonly Dictionary<string, HashSet<CloudUploadCommand>> _cloudUploadCommands = new();
    public event Action? Changed;

    public StorageStore(DownloadsService downloads, Func<bool> loadPinPreference)
    {
        _downloads = downloads;
        _loadPinPreference = loadPinPreference;
    }

    public void ApplyOutbox(BridgeOutboxSnapshot snapshot)
    {
        if (_outbox is { } current && snapshot.Revision < current.Revision)
        {
            BaeDiagnostics.Logger.Debug(
                $"Dropped outbox snapshot at revision {snapshot.Revision}; revision {current.Revision} is already applied.");
            return;
        }
        _outbox = snapshot;
        foreach (var (releaseId, handoff) in _cloudUploadHandoffs.ToArray())
        {
            if (handoff is CloudUploadHandoff.Queueing
                    && snapshot.PerRelease.ContainsKey(releaseId)
                || handoff is CloudUploadHandoff.Awaiting awaiting
                    && snapshot.Revision >= awaiting.Revision)
            {
                _cloudUploadHandoffs.Remove(releaseId);
                _cloudUploadCommands.Remove(releaseId);
            }
        }
        Changed?.Invoke();
    }

    public void ApplyDownloads(BridgeDownloadSnapshot snapshot)
    {
        _downloadsSnapshot = snapshot;
        Changed?.Invoke();
    }

    public void ApplyOutputs(BridgeOutputSnapshot snapshot)
    {
        _outputSnapshot = snapshot;
        Changed?.Invoke();
    }

    public BridgeOutboxSnapshot? Outbox => _outbox;
    public BridgeDownloadSnapshot? Downloads => _downloadsSnapshot;
    public BridgeOutputSnapshot? Outputs => _outputSnapshot;

    // Of the given releases, those with a transition in flight: an outbox upload,
    // a queued/downloading pin, or a transfer carried by the current storage row.
    // A release in this set offers only Cancel — the storage
    // actions would race the transition. Returns the outbox read error, if any,
    // so the caller can surface it like the panel load does.
    public async System.Threading.Tasks.Task<(HashSet<string> Transitioning, string? Error)> TransitioningReleases(
        List<string> releaseIds, Dictionary<string, BridgeStorageRow> rowsById)
    {
        var (uploading, uploadError) = await UploadingReleases(releaseIds);
        var transitioning = new HashSet<string>(uploading);
        transitioning.UnionWith(await DownloadingReleases(releaseIds));
        transitioning.UnionWith(releaseIds.Where(id =>
            rowsById.TryGetValue(id, out var row) && row.Release.TransferAction is not null));
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
        var snapshot = _outbox;
        if (snapshot is null)
        {
            // Couldn't read the outbox; surface it like the panel load does rather
            // than silently dropping the cancel action.
            return (new List<string>(), Loc.Chrome("outbox.read_failed"));
        }

        return (
            releaseIds.Where(id =>
                snapshot.PerRelease.ContainsKey(id)
                || _cloudUploadHandoffs.ContainsKey(id)).ToList(),
            null);
    }

    public CloudUploadHandoff? UploadHandoff(string releaseId) =>
        _cloudUploadHandoffs.GetValueOrDefault(releaseId);

    public bool CanCancelUpload(string releaseId) =>
        _outbox?.PerRelease.TryGetValue(releaseId, out var progress) == true
        && progress.Progress.CanCancel;

    private CloudUploadCommand BeginCloudUploads(IReadOnlyList<string> releaseIds)
    {
        var command = new CloudUploadCommand(releaseIds);
        var selected = releaseIds.ToHashSet();
        if (releaseIds.Count == 0
            || selected.Count != releaseIds.Count)
        {
            return command;
        }
        foreach (var releaseId in releaseIds)
        {
            if (_outbox?.PerRelease.ContainsKey(releaseId) == true)
            {
                continue;
            }
            if (!_cloudUploadHandoffs.TryGetValue(releaseId, out var handoff))
            {
                _cloudUploadHandoffs.Add(releaseId, new CloudUploadHandoff.Queueing());
                _cloudUploadCommands.Add(releaseId, [command]);
            }
            else if (handoff is CloudUploadHandoff.Queueing)
            {
                if (!_cloudUploadCommands.TryGetValue(releaseId, out var commands))
                {
                    commands = [];
                    _cloudUploadCommands.Add(releaseId, commands);
                }
                commands.Add(command);
            }
        }
        Changed?.Invoke();
        return command;
    }

    private void CloudUploadsQueued(CloudUploadCommand command, ulong revision)
    {
        foreach (var releaseId in command.ReleaseIds)
        {
            _cloudUploadCommands.Remove(releaseId);
            if (_outbox?.Revision >= revision)
            {
                _cloudUploadHandoffs.Remove(releaseId);
            }
            else
            {
                _cloudUploadHandoffs[releaseId] = new CloudUploadHandoff.Awaiting(revision);
            }
        }
        Changed?.Invoke();
    }

    private void EndCloudUploadBatchWithoutReceipt(CloudUploadCommand command)
    {
        var changed = false;
        foreach (var releaseId in command.ReleaseIds)
        {
            if (!_cloudUploadCommands.TryGetValue(releaseId, out var commands))
            {
                continue;
            }
            commands.Remove(command);
            if (commands.Count == 0)
            {
                _cloudUploadCommands.Remove(releaseId);
                if (_cloudUploadHandoffs.GetValueOrDefault(releaseId)
                    is CloudUploadHandoff.Queueing)
                {
                    changed |= _cloudUploadHandoffs.Remove(releaseId);
                }
            }
        }
        if (changed)
        {
            Changed?.Invoke();
        }
    }

    // Of the given releases, those queued or downloading in the pin queue. The pin
    // queue is in-memory and read is infallible bar a dropped handle, so a failure
    // is an app-state fault — log it and offer no pin-cancel rather than a toast.
    public async System.Threading.Tasks.Task<List<string>> DownloadingReleases(List<string> releaseIds)
    {
        var snapshot = _downloadsSnapshot;
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
    // each release into it. Returns null on success (or a cancelled picker), else
    // the first error message.
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

            return await FirstErrorAcross(
                releaseIds, releaseId => _downloads.MakeReleaseLocal(releaseId, path));
        }

        switch (action)
        {
            case BridgeReleaseStorageAction.Pin:
                // Pin enqueues the whole selection in one call; Move to Cloud
                // has its own batch path below, while Unpin applies per release.
                var (pinCurrent, pinError) = await _downloads.QueuePins(releaseIds);
                return pinCurrent ? pinError : null;
            case BridgeReleaseStorageAction.Unpin:
                return await FirstErrorAcross(releaseIds, _downloads.UnpinRelease);
            case BridgeReleaseStorageAction.MakeRemote:
                return await MoveToCloudAcross(releaseIds);
            case BridgeReleaseStorageAction.MakeLocal:
                throw new InvalidOperationException(
                    "make-local storage actions must choose a destination before running");
            default:
                throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action");
        }
    }

    private async System.Threading.Tasks.Task<string?> MoveToCloudAcross(
        List<string> releaseIds)
    {
        var command = BeginCloudUploads(releaseIds);
        var (current, result) = await _downloads.MakeReleasesRemote(
            releaseIds,
            _loadPinPreference());
        if (!current)
        {
            EndCloudUploadBatchWithoutReceipt(command);
            return null;
        }
        if (result.Error is { } error)
        {
            EndCloudUploadBatchWithoutReceipt(command);
            return error;
        }
        if (result.Revision is not { } revision)
        {
            EndCloudUploadBatchWithoutReceipt(command);
            return null;
        }
        CloudUploadsQueued(command, revision);
        return null;
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

internal sealed class CloudUploadCommand
{
    public CloudUploadCommand(IReadOnlyList<string> releaseIds)
    {
        ReleaseIds = releaseIds.ToArray();
    }

    public IReadOnlyList<string> ReleaseIds { get; }
}

internal abstract record CloudUploadHandoff
{
    internal sealed record Queueing : CloudUploadHandoff;
    internal sealed record Awaiting(ulong Revision) : CloudUploadHandoff;
}

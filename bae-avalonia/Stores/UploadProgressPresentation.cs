using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal abstract record ImportUploadObservation
{
    internal sealed record Awaiting : ImportUploadObservation;
    internal sealed record Active(BridgeUploadProgress Progress) : ImportUploadObservation;
    internal sealed record Finished : ImportUploadObservation;
}

// Locale-rendered cloud-upload state shared by Import and Storage Manager.
// Core owns phase selection and counters; this type only formats that projection.
internal static class UploadProgressPresentation
{
    public static ImportUploadObservation? ResolveImport(
        BridgeCandidateImportStatus? status,
        BridgeOutboxSnapshot? snapshot)
    {
        var releaseId = status switch
        {
            BridgeCandidateImportStatus.CloudUploadQueued queued =>
                queued.ReleaseId,
            BridgeCandidateImportStatus.Complete complete =>
                complete.ReleaseId,
            _ => null,
        };
        if (releaseId is null)
        {
            return null;
        }
        if (snapshot?.PerRelease.TryGetValue(releaseId, out var progress) == true)
        {
            return new ImportUploadObservation.Active(progress);
        }
        return status switch
        {
            BridgeCandidateImportStatus.CloudUploadQueued queued
                when snapshot is null || snapshot.Revision < queued.OutboxRevision =>
                new ImportUploadObservation.Awaiting(),
            _ => new ImportUploadObservation.Finished(),
        };
    }

    public static string QueueSummary(
        BridgeOutboxPauseState pauseState,
        IReadOnlyList<BridgeCountLabel> parts)
    {
        var queue = SummaryParts(parts);
        var pause = pauseState switch
        {
            BridgeOutboxPauseState.Running => string.Empty,
            BridgeOutboxPauseState.Pausing => Loc.Core("core.outbox.pausing"),
            BridgeOutboxPauseState.Paused => Loc.Chrome("download.paused"),
            _ => throw new ArgumentOutOfRangeException(
                nameof(pauseState),
                pauseState,
                "Unknown upload pause state"),
        };
        return (pause, queue) switch
        {
            ("", _) => queue,
            (_, "") => pause,
            _ => $"{pause} · {queue}",
        };
    }

    public static string SummaryParts(IReadOnlyList<BridgeCountLabel> parts) =>
        string.Join(
            " · ",
            parts.Select(part => Loc.Core(part.Key, "count", part.Count)));

    public static string ActivityLabel(BridgeUploadProgress progress) =>
        progress.Activity switch
        {
            BridgeUploadActivity.Cancelling => Loc.Core("core.outbox.cancelling", "count", progress.Cancelling),
            BridgeUploadActivity.Publishing => Loc.Core("core.outbox.publishing", "count", progress.Publishing),
            BridgeUploadActivity.Uploading => Loc.Core("core.queue.uploading", "count", progress.Uploading),
            BridgeUploadActivity.Preparing => Loc.Core("core.outbox.preparing", "count", progress.Preparing),
            BridgeUploadActivity.Retrying => Loc.Core("core.outbox.retrying", "count", progress.Retrying),
            BridgeUploadActivity.Prepared => Loc.Core("core.outbox.prepared", "count", progress.Prepared),
            BridgeUploadActivity.Queued => Loc.Core("core.queue.queued", "count", progress.Queued),
            BridgeUploadActivity.Uploaded => Loc.Core("core.outbox.uploaded", "count", progress.Uploaded),
            _ => throw new InvalidOperationException(
                "An active cloud upload has no projected activity"),
        };

    // "Uploading 3 MB of 221.2 MB": the phase the bar fills with, then the
    // exact bytes it fills with. Both come off the bar itself, so the text and
    // the fill always count the same thing.
    public static string BarLabel(BridgeUploadBar? bar) =>
        bar is null
            ? string.Empty
            : Loc.Core(
                BaeBridgeMethods.BridgeUploadPhaseBytesKey(bar.Phase),
                new Dictionary<string, object?>
                {
                    ["done"] = Loc.Bytes(checked((long)bar.BytesDone)),
                    ["total"] = Loc.Bytes(checked((long)bar.BytesTotal)),
                });

    public static double? BarFraction(BridgeUploadBar? bar)
    {
        if (bar is null || bar.BytesTotal == 0)
        {
            return null;
        }
        if (bar.BytesDone > bar.BytesTotal)
        {
            throw new InvalidOperationException(
                "Cloud upload progress cannot exceed its exact total");
        }
        return (double)bar.BytesDone / bar.BytesTotal;
    }
}

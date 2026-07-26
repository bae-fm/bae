using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The storage sheet's locale-rendered labels: the outbox / download / upload text
// core produces as raw counts, keys, and byte figures, joined and formatted here.
// Split out of StorageDialog; pure, no Avalonia types.
internal sealed partial class StorageDialog
{
    private static BridgeDownloadTransferProgress? DownloadProgress(BridgeDownloadState state) =>
        state is BridgeDownloadState.Active active ? active.Progress : null;

    // Core decides the outbox summary's parts (uploading/failed/queued/pending
    // deletes), their order, and the drop-if-zero rule; this only localizes and
    // joins them.
    private static string OutboxSummary(BridgeOutboxSnapshot snapshot) =>
        QueueSummaryText(snapshot.SummaryParts);

    // Render core's queue-summary parts into one " · "-joined line.
    private static string QueueSummaryText(IReadOnlyList<BridgeCountLabel> parts) =>
        string.Join(" · ", parts.Select(part => Loc.Core(part.Key, "count", part.Count)));

    private static string OutboxThroughputLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.ThroughputBps > 0
            ? Loc.Core("core.outbox.throughput", "rate", Loc.Bytes(checked((long)snapshot.ThroughputBps)))
            : string.Empty;

    private static string OutboxEtaLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.EtaSeconds is { } seconds
            ? Loc.Core("core.outbox.eta", "duration", BridgeDisplay.Clock(checked(checked((long)seconds) * 1000)))
            : string.Empty;

    private static string OutboxBytesLabel(BridgeOutboxSnapshot snapshot)
    {
        if (snapshot.Total.BytesTotal == 0) return string.Empty;
        return Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)snapshot.Total.BytesDone)),
                ["total"] = Loc.Bytes(checked((long)snapshot.Total.BytesTotal)),
            });
    }

    // A release group's aggregate badge, mirroring the macOS queue pane: the
    // dominant activity plus the unshipped file count. Finished releases aren't
    // rendered, so there is no terminal badge. Empty for a group core would never
    // emit (no activity).
    private static string UploadBadgeLabel(BridgeUploadProgress progress)
    {
        var pending = progress.Queued + progress.Active + progress.Failed;
        return progress.Activity switch
        {
            BridgeUploadActivity.Uploading => Loc.Chrome("outbox.badge.uploading", "count", pending),
            BridgeUploadActivity.Queued => Loc.Chrome("outbox.badge.queued", "count", pending),
            BridgeUploadActivity.Retrying => Loc.Chrome("outbox.badge.retrying", "count", pending),
            _ => string.Empty,
        };
    }

    // A release group's cumulative byte progress over the queue burst, matching the
    // bar beside it.
    private static string UploadBytesLabel(BridgeUploadProgress progress) =>
        Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)progress.BytesDone)),
                ["total"] = Loc.Bytes(checked((long)progress.BytesTotal)),
            });

    private static string FileStateLabel(BridgeUploadFileState state) => state switch
    {
        BridgeUploadFileState.Uploading => Loc.Chrome("outbox.state.uploading"),
        BridgeUploadFileState.Queued => Loc.Chrome("outbox.state.queued"),
        BridgeUploadFileState.Retrying => Loc.Chrome("outbox.state.retrying"),
        BridgeUploadFileState.Done => Loc.Chrome("outbox.state.done"),
        _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown upload file state"),
    };

    // A file's byte text: "6.2 MB of 12.4 MB" while transferring; just the size
    // otherwise.
    private static string FileBytesLabel(BridgeUploadFileOp file)
    {
        var total = Loc.Bytes(checked((long)file.BytesTotal));
        if (file.State != BridgeUploadFileState.Uploading) return total;
        return Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)file.BytesDone)),
                ["total"] = total,
            });
    }

    private static string DeleteLabel(BridgeDeleteOp delete) =>
        $"{delete.Namespace}/{delete.BlobId} — {Loc.Chrome("outbox.delete.kind")}";
}

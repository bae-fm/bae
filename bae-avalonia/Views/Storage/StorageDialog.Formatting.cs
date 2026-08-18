using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The storage sheet's locale-rendered labels: the outbox / download / upload text
// core produces as raw counts, keys, and byte figures, joined and formatted here.
// Split out of StorageDialog; pure, no Avalonia types.
internal sealed partial class StorageDialog
{
    private static BridgeDownloadTransferProgress? DownloadProgress(BridgeDownloadState state) =>
        state is BridgeDownloadState.Active active ? active.Progress : null;

    // Core decides the outbox summary's parts (uploading/retrying/queued/pending
    // deletes), their order, and the drop-if-zero rule; this only localizes and
    // joins them.
    private static string OutboxSummary(BridgeOutboxSnapshot snapshot) =>
        UploadProgressPresentation.QueueSummary(
            snapshot.PauseState,
            snapshot.SummaryParts);

    private static string OutboxThroughputLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.ThroughputBps > 0
            ? Loc.Core("core.outbox.throughput", "rate", Loc.Bytes(checked((long)snapshot.ThroughputBps)))
            : string.Empty;

    private static string OutboxEtaLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.EtaSeconds is { } seconds
            ? Loc.Core("core.outbox.eta", "duration", BridgeDisplay.Clock(checked(checked((long)seconds) * 1000)))
            : string.Empty;

    private static string OutboxBytesLabel(BridgeOutboxSnapshot snapshot) =>
        UploadProgressPresentation.StageBytesLabel(snapshot.Total);

    // A release group's aggregate badge, mirroring the macOS queue pane: the
    // dominant activity. Provider-complete releases remain through publication;
    // there is no terminal badge because the group leaves after publication.
    // Empty for a group core would never emit (no activity).
    private static string UploadBadgeLabel(BridgeUploadProgress progress) =>
        UploadProgressPresentation.ActivityLabel(progress);

    private static string UploadBytesLabel(BridgeUploadProgress progress) =>
        UploadProgressPresentation.StageBytesLabel(progress);

    private static string FileStateLabel(BridgeUploadFileState state) => state switch
    {
        BridgeUploadFileState.Preparing => Loc.Core("core.outbox.preparing", "count", 1),
        BridgeUploadFileState.Prepared => Loc.Core("core.outbox.prepared", "count", 1),
        BridgeUploadFileState.Uploading => Loc.Chrome("outbox.state.uploading"),
        BridgeUploadFileState.Queued => Loc.Chrome("outbox.state.queued"),
        BridgeUploadFileState.Retrying => Loc.Chrome("outbox.state.retrying"),
        BridgeUploadFileState.Uploaded => Loc.Core("core.outbox.uploaded", "count", 1),
        _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown upload file state"),
    };

    private static string UploadFileLabel(BridgeUploadFileLabel label) => label switch
    {
        BridgeUploadFileLabel.Filename filename => filename.Name,
        BridgeUploadFileLabel.Cover => Loc.Core("core.import.role.cover"),
        BridgeUploadFileLabel.ArtistImage => Loc.Core("core.outbox.file.artist_image"),
        _ => throw new ArgumentOutOfRangeException(nameof(label), label, "Unknown upload file label"),
    };

    // A file's byte text: "6.2 MB of 12.4 MB" while transferring; just the size
    // otherwise.
    private static string FileBytesLabel(BridgeUploadFileOp file)
    {
        var sourceSize = Loc.Bytes(checked((long)file.SourceBytesTotal));
        if ((file.State != BridgeUploadFileState.Preparing
            && file.State != BridgeUploadFileState.Uploading)
            || file.ProgressBytesTotal == 0)
        {
            return sourceSize;
        }
        var total = Loc.Bytes(checked((long)file.ProgressBytesTotal));
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

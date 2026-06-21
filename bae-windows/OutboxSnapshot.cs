using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace Bae.Windows;

/// <summary>
/// The cloud outbox snapshot, deserialized from the FFI's
/// <c>bae_outbox_snapshot</c> JSON. The locale never crosses the bridge: counts,
/// throughput, ETA, and byte totals are raw, composed into localized lines here.
/// </summary>
public sealed class OutboxSnapshot
{
    /// <summary>Uploads grouped by release for the queue pane's per-release rows
    /// (matching the storage table). Resolved by core.</summary>
    public List<UploadReleaseGroup> UploadGroups { get; set; } = new();

    public List<DeleteOp> Deletes { get; set; } = new();

    /// <summary>True when the user paused the upload pipeline — drives the
    /// pause/resume toggle and dims the progress bar.</summary>
    public bool Paused { get; set; }

    /// <summary>Rolling-window throughput in bytes per second; 0 when idle or paused.</summary>
    public long ThroughputBps { get; set; }

    /// <summary>Estimated seconds remaining at the current rate; null when not computable.</summary>
    public long? EtaSeconds { get; set; }

    /// <summary>Aggregate byte progress; the master bar reads its done/total.</summary>
    public UploadProgress Total { get; set; } = new();

    /// <summary>Per-release upload aggregate, keyed by release id. Core omits
    /// idle releases, so a key's presence means the release has upload work
    /// queued or in flight — the storage row reads this to offer "Cancel
    /// Upload".</summary>
    public Dictionary<string, UploadProgress> PerRelease { get; set; } = new();

    /// <summary>One-line queue summary, e.g. "2 uploading · 1 failed · 3 queued";
    /// empty when the queue is idle so the UI can hide it. Each part is a
    /// localized count message from the shared catalog.</summary>
    [JsonIgnore]
    public string Summary
    {
        get
        {
            var parts = new List<string>();
            if (Total.Active > 0) parts.Add(Loc.Core("core.queue.uploading", "count", Total.Active));
            if (Total.Failed > 0) parts.Add(Loc.Core("core.queue.failed", "count", Total.Failed));
            if (Total.Queued > 0) parts.Add(Loc.Core("core.queue.queued", "count", Total.Queued));
            if (Deletes.Count > 0)
                parts.Add(Loc.Core("core.outbox.pending_deletes", "count", Deletes.Count));
            return string.Join(" · ", parts);
        }
    }

    /// <summary>Formatted throughput, e.g. "5.2 MB/s"; empty when idle. The rate
    /// is a locale-formatted byte count substituted into the shared message.</summary>
    [JsonIgnore]
    public string ThroughputLabel =>
        ThroughputBps > 0 ? Loc.Core("core.outbox.throughput", "rate", Loc.Bytes(ThroughputBps)) : string.Empty;

    /// <summary>Formatted ETA, e.g. "2m 14s remaining"; empty when not computable.
    /// The duration is locale-formatted and substituted into the shared message.</summary>
    [JsonIgnore]
    public string EtaLabel =>
        EtaSeconds is { } secs
            ? Loc.Core("core.outbox.eta", "duration", Loc.Duration(secs * 1000))
            : string.Empty;

    /// <summary>Formatted aggregate progress, e.g. "1.2 GB of 14.4 GB"; empty when
    /// there is nothing to upload. Both byte counts are locale-formatted and
    /// substituted into the shared message.</summary>
    [JsonIgnore]
    public string BytesLabel
    {
        get
        {
            if (Total.BytesTotal <= 0) return string.Empty;
            return Loc.Core(
                "core.outbox.bytes_progress",
                new Dictionary<string, object?>
                {
                    ["done"] = Loc.Bytes(Total.BytesDone),
                    ["total"] = Loc.Bytes(Total.BytesTotal),
                });
        }
    }
}

/// <summary>Aggregate upload progress across the queue.</summary>
public sealed class UploadProgress
{
    public uint Queued { get; set; }
    public uint Active { get; set; }
    public uint Failed { get; set; }
    public long BytesDone { get; set; }
    public long BytesTotal { get; set; }
}

/// <summary>A release's pending uploads, grouped for the queue pane's per-release
/// rows. <c>ReleaseId</c> is null for the orphaned-files bucket; <c>DisplayTitle</c>
/// is the row label, resolved by core.</summary>
public sealed class UploadReleaseGroup
{
    public string? ReleaseId { get; set; }
    public string DisplayTitle { get; set; } = string.Empty;
    public uint FileCount { get; set; }
    public UploadProgress Progress { get; set; } = new();
}

/// <summary>One queued cloud delete.</summary>
public sealed class DeleteOp
{
    public long Id { get; set; }
    public string CloudKey { get; set; } = string.Empty;

    /// <summary>The list row: the cloud key being deleted, with a localized kind.</summary>
    [JsonIgnore]
    public string Label => $"{CloudKey} — {Loc.Chrome("outbox.delete.kind")}";
}

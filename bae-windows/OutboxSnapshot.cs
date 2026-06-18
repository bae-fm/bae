using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// The cloud outbox snapshot, deserialized from the FFI's
/// <c>bae_outbox_snapshot</c> JSON: a one-line summary band plus the queued upload
/// and delete operations.
/// </summary>
public sealed class OutboxSnapshot
{
    /// <summary>Pre-formatted summary band, e.g. "2 uploading · 1 failed · 3 queued";
    /// empty when the queue is idle so the UI can hide it.</summary>
    public string Summary { get; set; } = string.Empty;
    public List<UploadOp> Uploads { get; set; } = new();
    public List<DeleteOp> Deletes { get; set; } = new();

    /// <summary>True when the user paused the upload pipeline — drives the
    /// pause/resume toggle and dims the progress bar.</summary>
    public bool Paused { get; set; }

    /// <summary>Pre-formatted throughput, e.g. "5.2 MB/s"; empty when idle.</summary>
    public string ThroughputLabel { get; set; } = string.Empty;

    /// <summary>Pre-formatted ETA, e.g. "7s remaining"; empty when not computable.</summary>
    public string EtaLabel { get; set; } = string.Empty;

    /// <summary>Pre-formatted aggregate progress, e.g. "1.2 GB of 14.4 GB"; empty
    /// when there's nothing to upload.</summary>
    public string BytesLabel { get; set; } = string.Empty;

    /// <summary>Aggregate byte progress; the master bar reads its done/total.</summary>
    public UploadProgress Total { get; set; } = new();
}

/// <summary>Aggregate upload progress across the queue.</summary>
public sealed class UploadProgress
{
    public long BytesDone { get; set; }
    public long BytesTotal { get; set; }
}

/// <summary>One queued upload.</summary>
public sealed class UploadOp
{
    public long Id { get; set; }

    /// <summary>Owning release id; null for an orphaned file. The storage row
    /// context menu reads this to find a release's queued uploads to cancel.</summary>
    public string? ReleaseId { get; set; }

    /// <summary>Album title when the upload still resolves to a release; null for an
    /// orphaned file.</summary>
    public string? Title { get; set; }
    public string CloudKey { get; set; } = string.Empty;

    /// <summary>Pre-formatted size, e.g. "12.4 MB".</summary>
    public string SizeLabel { get; set; } = string.Empty;
    public long AttemptCount { get; set; }

    /// <summary>Bytes of this file that have reached the cloud so far; advances
    /// mid-upload while <see cref="State"/> is "active".</summary>
    public long BytesDone { get; set; }

    /// <summary>Total bytes for this file (the encrypted payload size).</summary>
    public long BytesTotal { get; set; }

    /// <summary>"queued", "active", or "failed".</summary>
    public string State { get; set; } = string.Empty;

    /// <summary>The last failure message when <see cref="State"/> is "failed".</summary>
    public string? LastError { get; set; }

    /// <summary>True while this upload is in flight — drives the per-file mini
    /// progress bar.</summary>
    public bool IsActive => State == "active";

    /// <summary>Byte progress as a 0...1 fraction for the active-row mini bar.</summary>
    public double Fraction => BytesTotal > 0 ? (double)BytesDone / BytesTotal : 0;

    /// <summary>The list row: title-or-key, state, size, and attempt count. An
    /// active upload shows its live byte fraction (e.g. "45% · 31.5 of 70 MB")
    /// instead of the bare "active".</summary>
    public string Label
    {
        get
        {
            var name = string.IsNullOrEmpty(Title) ? CloudKey : Title;
            string detail;
            if (State == "failed" && !string.IsNullOrEmpty(LastError))
            {
                detail = $"failed: {LastError}";
            }
            else if (IsActive && BytesTotal > 0)
            {
                detail = $"uploading · {(int)(Fraction * 100)}%";
            }
            else
            {
                detail = State;
            }
            var size = string.IsNullOrEmpty(SizeLabel) ? string.Empty : $" · {SizeLabel}";
            var attempts = AttemptCount > 0
                ? $" · {AttemptCount} attempt{(AttemptCount == 1 ? "" : "s")}"
                : string.Empty;
            return $"{name} — upload · {detail}{size}{attempts}";
        }
    }
}

/// <summary>One queued cloud delete.</summary>
public sealed class DeleteOp
{
    public long Id { get; set; }
    public string CloudKey { get; set; } = string.Empty;

    /// <summary>The list row: the cloud key being deleted.</summary>
    public string Label => $"{CloudKey} — delete";
}

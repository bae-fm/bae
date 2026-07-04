using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>The in-memory download (pin) queue snapshot. Drives the Downloads
/// pane, and the storage row reads it to tell a pinning release (queued or
/// downloading) from an idle one so the row can offer to cancel the pin.</summary>
public sealed class DownloadSnapshot
{
    public List<DownloadOp> Downloads { get; set; } = new();

    /// <summary>Per-state counts rolled up across the queue — drives the pane
    /// header and the retry gate.</summary>
    public DownloadProgress Total { get; set; } = new();

    /// <summary>True when the user paused the queue — drives the pause/resume
    /// toggle.</summary>
    public bool Paused { get; set; }
}

/// <summary>Per-state counts for the download queue.</summary>
public sealed class DownloadProgress
{
    public uint Queued { get; set; }
    public uint Active { get; set; }
    public uint Failed { get; set; }
}

/// <summary>One queued download — a whole release being pinned.</summary>
public sealed class DownloadOp
{
    public string ReleaseId { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public long FileCount { get; set; }
    public long TotalSize { get; set; }

    /// <summary>"queued", "active", or "failed".</summary>
    public string State { get; set; } = string.Empty;

    /// <summary>The active transfer's byte/file progress when <see cref="State"/>
    /// is "active".</summary>
    public DownloadTransferProgress? Progress { get; set; }

    /// <summary>The failure message when <see cref="State"/> is "failed".</summary>
    public string? Error { get; set; }
}

/// <summary>Byte and file progress for the active download.</summary>
public sealed class DownloadTransferProgress
{
    public ulong BytesDone { get; set; }
    public ulong BytesTotal { get; set; }
    public double Fraction { get; set; }
}

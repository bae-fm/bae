using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>The in-memory download (pin) queue snapshot. The storage row reads it
/// to tell a release that's pinning (queued or downloading) from an idle one, so
/// its context menu can offer to cancel the pin. The pin queue has no Windows
/// pane yet, so only the release id is carried.</summary>
public sealed class DownloadSnapshot
{
    public List<DownloadOp> Downloads { get; set; } = new();
}

/// <summary>One queued download — a release being pinned.</summary>
public sealed class DownloadOp
{
    public string ReleaseId { get; set; } = string.Empty;
}

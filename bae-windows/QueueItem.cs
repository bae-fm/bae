namespace Bae.Windows;

/// <summary>One track in the play queue, from the <c>QueueUpdated</c> event JSON.</summary>
public sealed class QueueItem
{
    public string TrackId { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;
    public string Duration { get; set; } = string.Empty;

    /// <summary>The list row; used as the default item text.</summary>
    public override string ToString() => $"{Title} — {Artist} · {Duration}".Trim();
}

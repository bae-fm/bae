namespace Bae.Windows;

/// <summary>One track in the play queue, from the <c>QueueUpdated</c> event JSON.</summary>
public sealed class QueueItem
{
    /// <summary>Per-instance id: the same track queued twice yields two items
    /// with two ids, so rows key on it and remove/reorder/skip target one
    /// instance.</summary>
    public string EntryId { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;

    /// <summary>Raw track length in milliseconds, or null when unknown.</summary>
    public long? DurationMs { get; set; }

    /// <summary>The track length formatted for the locale, e.g. "3:07".</summary>
    public string DurationLabel => Loc.Duration(DurationMs);

    /// <summary>The list row; used as the default item text.</summary>
    public override string ToString() => $"{Title} — {Artist} · {DurationLabel}".Trim();
}

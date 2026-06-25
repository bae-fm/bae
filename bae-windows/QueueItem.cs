using System.Collections.Generic;

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

/// <summary>The context lane (what the queue is playing from), from the
/// <c>QueueUpdated</c> event JSON: its kind (<c>release</c> / <c>library</c>, for
/// the section label), its not-yet-played tail, plus whether it was ordered by
/// shuffle (the queue dialog shows a shuffle indicator when so). Rendered as its
/// own section, distinct from the manual "Up Next" lane.</summary>
public sealed class PlaybackContext
{
    /// <summary>The source kind wire name: <c>"release"</c> or <c>"library"</c>.</summary>
    public string Kind { get; set; } = string.Empty;
    public bool Shuffled { get; set; }
    public List<QueueItem> Upcoming { get; set; } = new();
}

using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// A UI event from the core, deserialized from the FFI's tagged JSON. A single
/// flat shape covers every event: <see cref="Type"/> names the FfiEvent variant
/// and only that variant's fields are set; the handler switches on it.
/// </summary>
public sealed class BaeEvent
{
    public string Type { get; set; } = string.Empty;

    // Identify the playing track and its album, so "go to now playing" can open
    // that album and reveal the track. Carried by PlaybackPlaying / PlaybackPaused.
    public string? TrackId { get; set; }
    public string? AlbumId { get; set; }
    public string? TrackTitle { get; set; }
    public string? Artist { get; set; }
    public string? CoverImageId { get; set; }
    public string? DurationLabel { get; set; }
    public double Progress { get; set; }
    public string? Elapsed { get; set; }
    public string? Remaining { get; set; }

    // Volume / mute / repeat.
    public float Volume { get; set; }
    public bool IsMuted { get; set; }
    public string? Mode { get; set; }
    public List<QueueItem>? Items { get; set; }
    public bool HasNext { get; set; }
    public bool HasPrevious { get; set; }

    // QueueItemsAdded — count of tracks just appended/inserted, for the +N badge.
    public int? Count { get; set; }

    // Scan-candidate events.
    public string? Key { get; set; }
    public string? Name { get; set; }
    public int? TrackCount { get; set; }
    public string? Format { get; set; }
    public List<string>? AudioPaths { get; set; }

    // Auto-identify events (CandidateIdentifyState).
    public string? Status { get; set; }
    public List<Candidate>? Matches { get; set; }
    public string? Message { get; set; }

    // Per-signal badge list, carried by CandidateIdentifyState.
    public List<SignalBadge>? Signals { get; set; }

    // Import events (CandidateImportProgress / Complete / Error).
    public int ProgressPercent { get; set; }

    // Sync status (SyncingChanged / SyncTimeChanged) for the toolbar indicator.
    // SyncTime is Unix epoch milliseconds of the last successful sync, or null
    // when never synced.
    public bool Syncing { get; set; }
    public long? SyncTime { get; set; }
}

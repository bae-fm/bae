using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// A UI event from the core, deserialized from the FFI's tagged JSON. A single
/// flat shape covers every event: <see cref="Type"/> names the FfiEvent variant
/// and only that variant's fields are set; the handler switches on it.
///
/// The locale never crosses the bridge: progress/duration are raw milliseconds
/// formatted here, and failures are structured (<see cref="Reason"/>,
/// <see cref="Error"/>, <see cref="Step"/>) so the UI resolves a localized line.
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

    // Raw track length / playback position in milliseconds; formatted for the
    // locale by the handler. DurationMs is carried by PlaybackPlaying /
    // PlaybackPaused / PlaybackProgress / PreviewPlaying; PositionMs by
    // PlaybackProgress / PreviewProgress.
    public ulong DurationMs { get; set; }
    public ulong PositionMs { get; set; }
    public double Progress { get; set; }

    /// <summary>Now-playing track length formatted for the locale, e.g. "3:07".</summary>
    public string DurationLabel => Loc.Duration((long)DurationMs);

    /// <summary>Elapsed playback position formatted for the locale.</summary>
    public string ElapsedLabel => Loc.Duration((long)PositionMs);

    /// <summary>Remaining playback time (duration − position) formatted for the
    /// locale; the now-playing bar shows it on the trailing side.</summary>
    public string RemainingLabel =>
        Loc.Duration((long)(DurationMs > PositionMs ? DurationMs - PositionMs : 0));

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

    // Per-signal badge list, carried by CandidateIdentifyState.
    public List<SignalBadge>? Signals { get; set; }

    // Import progress (CandidateImportProgress): percent + the structured step.
    public int ProgressPercent { get; set; }

    /// <summary>The structured import step (CandidateImportProgress); null before
    /// the first step is known. The handler resolves its localized verb.</summary>
    public ImportStep? Step { get; set; }

    /// <summary>The structured diagnostic for Error / SyncError /
    /// CandidateImportError; null when SyncError clears a prior failure.</summary>
    public DiagnosticError? Error { get; set; }

    /// <summary>The structured playback-failure reason (PlaybackError).</summary>
    public PlaybackErrorReason? Reason { get; set; }

    // Sync status (SyncingChanged / SyncTimeChanged) for the toolbar indicator.
    // SyncTime is Unix epoch milliseconds of the last successful sync, or null
    // when never synced.
    public bool Syncing { get; set; }
    public long? SyncTime { get; set; }
}

/// <summary>
/// A structured import-progress step, mirroring the FFI's <c>FfiImportStep</c>.
/// <see cref="Kind"/> tags whether it's a prepare step ("preparing") or a
/// running phase ("running"); <see cref="StepTag"/> / <see cref="Phase"/> is the
/// inner wire tag whose catalog key the FFI maps. The locale never crosses the
/// bridge — the verb is resolved here.
/// </summary>
public sealed class ImportStep
{
    /// <summary>"preparing" / "running".</summary>
    public string Kind { get; set; } = string.Empty;

    /// <summary>Prepare-step wire tag for the "preparing" case
    /// (e.g. "parsing_metadata").</summary>
    [System.Text.Json.Serialization.JsonPropertyName("step")]
    public string? StepTag { get; set; }

    /// <summary>Import-phase wire tag for the "running" case ("acquire"/"store").</summary>
    public string? Phase { get; set; }

    /// <summary>The localized progress verb for this step, or empty for an
    /// unknown tag. The key comes from the FFI (one source for the mapping).</summary>
    public string LocalizedLabel
    {
        get
        {
            var key = Kind switch
            {
                "preparing" when StepTag is not null => NativeBae.PrepareStepKey(StepTag),
                "running" when Phase is not null => NativeBae.ImportPhaseKey(Phase),
                _ => null,
            };
            return key is null ? string.Empty : Loc.Core(key);
        }
    }
}

using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// A UI event from the generated bridge, flattened for the existing handler.
/// <see cref="Type"/> names the event variant and only that variant's fields are
/// set; the handler switches on it.
///
/// The locale never crosses the bridge: progress/duration are raw milliseconds
/// formatted here, and failures are structured (<see cref="Reason"/>,
/// <see cref="Error"/>, <see cref="Step"/>) so the UI resolves a localized line.
/// </summary>
public sealed class BaeEvent
{
    public string Type { get; set; } = string.Empty;
    public BaeInvalidation? Invalidation { get; set; }

    // Identify the playing track and its album, so "go to now playing" can open
    // that album and reveal the track. Carried by PlaybackPlaying /
    // PlaybackPaused / PlaybackProgress / PlaybackSeeked.
    public string? TrackId { get; set; }
    public string? AlbumId { get; set; }
    public string? TrackTitle { get; set; }
    public string? Artist { get; set; }
    public string? CoverImageId { get; set; }

    // Raw track length / playback position in milliseconds; formatted for the
    // locale by the handler. DurationMs is carried by PlaybackPlaying /
    // PlaybackPaused / PlaybackProgress / PlaybackSeeked / PreviewPlaying;
    // PositionMs by PlaybackProgress / PlaybackSeeked / PreviewProgress.
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
    // The queue's two lanes (QueueUpdated): the manual lane ("Up Next") and the
    // context (the release being played from), rendered as distinct sections.
    public List<BridgeQueueEntry>? Manual { get; set; }
    public BridgePlaybackContext? Context { get; set; }
    public bool HasNext { get; set; }
    public bool HasPrevious { get; set; }

    // QueueItemsAdded — count of tracks just appended/inserted, for the +N badge.
    public int? Count { get; set; }

    // CandidateImportLoudnessProgress routes the high-frequency import bar.
    public string? Key { get; set; }
    public int TracksDone { get; set; }
    public int TracksTotal { get; set; }

    /// <summary>The structured diagnostic for Error.</summary>
    public DiagnosticError? Error { get; set; }

    /// <summary>The structured playback-failure reason (PlaybackError).</summary>
    public PlaybackErrorReason? Reason { get; set; }

    /// <summary>The structured pause reason (PlaybackPaused).</summary>
    public PlaybackPauseReason? PauseReason { get; set; }

}

public sealed class BaeInvalidation
{
    public string Kind { get; set; } = string.Empty;
    public string? AlbumId { get; set; }
    public string? ReleaseId { get; set; }
    public string? ComposerId { get; set; }
    public string? Key { get; set; }
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

    /// <summary>Import-phase wire tag for the "running" case
    /// ("referencing_files"/"measuring_loudness"/"finalizing").</summary>
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

/// <summary>
/// A structured playback pause reason, mirroring the FFI's
/// <c>FfiPlaybackPauseReason</c>. The UI resolves alert copy from
/// the core catalog.
/// </summary>
public sealed class PlaybackPauseReason
{
    public string Kind { get; set; } = string.Empty;
    public SidePausePrompt? Prompt { get; set; }

    public string? AlertTitle
    {
        get
        {
            if (Prompt is null)
            {
                return null;
            }
            return Loc.Core(Prompt.TitleKey, "letter", Prompt.SideLetter);
        }
    }

    public string? AlertMessage
    {
        get
        {
            return Prompt is null ? null : Loc.Core(Prompt.MessageKey);
        }
    }
}

public sealed class SidePausePrompt
{
    public string TitleKey { get; set; } = string.Empty;
    public string SideLetter { get; set; } = string.Empty;
    public string MessageKey { get; set; } = string.Empty;
}

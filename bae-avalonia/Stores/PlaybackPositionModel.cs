using System;

namespace Bae.Desktop;

// The now-playing bar's seek projection: the position the user dropped the
// slider on, shown until core confirms it. TrackId pins it to the track it was
// made for so a projection for a since-changed track is dropped.
public sealed record SeekProjection(
    string? TrackId,
    ulong TargetPositionMs,
    ulong DurationMs,
    double Progress);

public sealed record PlaybackPositionSnapshot(
    ulong DurationMs,
    long PositionMs,
    double Progress);

public sealed record PlaybackPositionState(
    PlaybackPositionSnapshot Snapshot,
    SeekProjection? Projection);

public sealed record NowPlayingState(
    string? AlbumId,
    string? TrackId,
    PlaybackPositionState? Position);

// Whether a playback-position update targets the current track, and if not, why.
public enum PlaybackPositionRejection
{
    Accepted,
    MissingTrackId,
    StaleTrack,
}

// Pure position math for the now-playing bar: seek projection, remaining time,
// and the state transitions the playback store applies. No Avalonia, no bridge —
// track ids are plain strings, slider bounds are doubles.
public static class PlaybackPositionModel
{
    // The seek the user dropped the slider on, as a projection for the current
    // snapshot: clamp the requested progress into the slider's range, map it to a
    // 0..1 ratio, and round to the target position, never past the duration.
    public static SeekProjection ProjectSeek(
        string? trackId,
        PlaybackPositionSnapshot current,
        double requestedProgress,
        double sliderMinimum,
        double sliderMaximum)
    {
        var progress = Math.Clamp(requestedProgress, sliderMinimum, sliderMaximum);
        var progressRange = sliderMaximum - sliderMinimum;
        if (progressRange <= 0)
        {
            throw new InvalidOperationException("Seek slider range must be positive.");
        }

        var ratio = Math.Clamp((progress - sliderMinimum) / progressRange, 0, 1);
        var targetPositionMs = (ulong)Math.Round(current.DurationMs * ratio);
        if (targetPositionMs > current.DurationMs)
        {
            targetPositionMs = current.DurationMs;
        }

        return new SeekProjection(trackId, targetPositionMs, current.DurationMs, progress);
    }

    // A projection wins over a live progress update only when it was made for the
    // same track; a projection for a track that has since changed is dropped.
    public static bool ProjectionWins(SeekProjection? projection, string trackId) =>
        projection is not null && projection.TrackId == trackId;

    // Whether a playback-position update targets the current track: a missing
    // incoming id or an id for a different track than the one now playing is
    // rejected.
    public static PlaybackPositionRejection ClassifyPlaybackPosition(string? currentTrackId, string? incomingTrackId)
    {
        if (incomingTrackId is null)
        {
            return PlaybackPositionRejection.MissingTrackId;
        }
        if (currentTrackId is not null && currentTrackId != incomingTrackId)
        {
            return PlaybackPositionRejection.StaleTrack;
        }
        return PlaybackPositionRejection.Accepted;
    }

    // The leading label's tooltip names the action a click performs, so it follows
    // the current mode.
    public static string TimeLabelTooltipKey(bool showRemaining) =>
        showRemaining ? "nowplaying.show_elapsed" : "nowplaying.show_remaining";

    // Fold a fresh snapshot into the now-playing state, keeping the album when a
    // state already exists, or creating one for the track when it doesn't.
    public static NowPlayingState WithPosition(
        NowPlayingState? current,
        string trackId,
        PlaybackPositionSnapshot snapshot,
        SeekProjection? projection)
    {
        var position = new PlaybackPositionState(snapshot, projection);
        return current is { } nowPlaying
            ? nowPlaying with { Position = position }
            : new NowPlayingState(null, trackId, position);
    }

    // Drop the seek projection while keeping the underlying position snapshot.
    public static NowPlayingState? ClearProjection(NowPlayingState? state) =>
        state is { Position: { } position } nowPlaying
            ? nowPlaying with { Position = position with { Projection = null } }
            : state;

    // Enter a loading transition: reset the position when the loading track
    // differs from the one on screen, keeping it otherwise.
    public static NowPlayingState? BeginLoading(NowPlayingState? current, string trackId) =>
        current is { } state && trackId != state.TrackId
            ? state with { Position = null }
            : current;
}

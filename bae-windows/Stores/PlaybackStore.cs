using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The now-playing bar's display fields for a playing or paused track. PauseReason
// is null while playing and carries the pause reason while paused (which may
// prompt the side-ended dialog).
internal sealed record NowPlayingBarTrack(
    string Title,
    string Artist,
    string? CoverImageId,
    bool IsPlaying,
    BridgePlaybackPauseReason? PauseReason);

// The effective position to render on the seek bar (already resolved between a
// live progress update and a pending seek projection).
internal sealed record PlaybackPositionRender(
    double Progress,
    ulong PositionMs,
    ulong DurationMs);

// Mirror of core's playback state: the now-playing track/position and the queue
// lanes. The reducer is the sole writer, driven by BridgeUiEvent deliveries; the
// now-playing bar subscribes to the change events and renders. Views read the
// snapshot properties and never write back.
internal sealed class PlaybackStore
{
    private NowPlayingState? _nowPlaying;

    // The manual lane ("Up Next") — explicitly enqueued tracks — and the context
    // (the release being played from), kept separate so the queue dialog renders
    // them as distinct sections. Context is null when nothing plays from a release.
    private List<BridgeQueueEntry> _queueManual = new();
    private BridgePlaybackContext? _queueContext;

    public IReadOnlyList<BridgeQueueEntry> ManualQueue => _queueManual;
    public BridgePlaybackContext? Context => _queueContext;
    public string? NowPlayingAlbumId => _nowPlaying?.AlbumId;
    public string? NowPlayingTrackId => _nowPlaying?.TrackId;

    // The queue revision the current lanes were resolved from. Stamped onto
    // every QueueUpcomingPage fetch so a reply computed under a
    // since-superseded revision is dropped rather than merged.
    public ulong Revision { get; private set; }

    // The current mute state, for the mute button that must compute an absolute
    // command target. Written by ApplyMute from the MuteChanged payload.
    public bool IsMuted { get; private set; }

    public event Action<NowPlayingBarTrack>? NowPlayingChanged;
    public event Action? PlaybackStopped;
    public event Action? LoadingStarted;
    public event Action<PlaybackPositionRender>? PositionChanged;
    public event Action<double>? VolumeChanged;
    public event Action<bool>? MuteChanged;
    public event Action<BridgeRepeatMode>? RepeatChanged;
    public event Action<bool, bool>? TransportChanged;
    public event Action<int>? QueueItemsAdded;

    // The lanes changed (from a mutation on any source). The non-modal queue pane
    // re-renders on this while visible; the now-playing bar reads only transport.
    public event Action? QueueChanged;

    public void ApplyPlaying(string albumId, string trackId, string trackTitle, string artistNames, string? coverImageId)
    {
        _nowPlaying = new NowPlayingState(albumId, trackId, KeptPositionFor(trackId));
        NowPlayingChanged?.Invoke(new NowPlayingBarTrack(trackTitle, artistNames, coverImageId, true, null));
    }

    public void ApplyPaused(string albumId, string trackId, string trackTitle, string artistNames, string? coverImageId, BridgePlaybackPauseReason reason)
    {
        _nowPlaying = new NowPlayingState(albumId, trackId, KeptPositionFor(trackId));
        NowPlayingChanged?.Invoke(new NowPlayingBarTrack(trackTitle, artistNames, coverImageId, false, reason));
    }

    public void ApplyStopped()
    {
        _nowPlaying = null;
        PlaybackStopped?.Invoke();
    }

    public void ApplyProgress(string trackId, ulong positionMs, ulong durationMs, double progress)
    {
        if (!Accept(trackId))
        {
            return;
        }

        var projection = _nowPlaying?.Position?.Projection;
        if (PlaybackPositionModel.ProjectionWins(projection, trackId))
        {
            PositionChanged?.Invoke(new PlaybackPositionRender(
                projection!.Progress,
                projection.TargetPositionMs,
                projection.DurationMs));
            return;
        }

        ApplyPositionSnapshot(trackId, durationMs, positionMs, progress);
    }

    public void ApplySeeked(string trackId, ulong positionMs, ulong durationMs, double progress)
    {
        if (!Accept(trackId))
        {
            return;
        }

        ApplyPositionSnapshot(trackId, durationMs, positionMs, progress);
    }

    public void ApplyLoading(string trackId)
    {
        _nowPlaying = PlaybackPositionModel.BeginLoading(_nowPlaying, trackId);
        LoadingStarted?.Invoke();
    }

    public void ApplyVolume(double volume) => VolumeChanged?.Invoke(volume);

    public void ApplyMute(bool isMuted)
    {
        IsMuted = isMuted;
        MuteChanged?.Invoke(isMuted);
    }

    public void ApplyRepeat(BridgeRepeatMode mode) => RepeatChanged?.Invoke(mode);

    public void ApplyQueueUpdated(BridgeQueueSnapshot snapshot)
    {
        _queueManual = snapshot.Manual.ToList();
        _queueContext = snapshot.Context;
        Revision = snapshot.Revision;
        TransportChanged?.Invoke(snapshot.HasPrevious, snapshot.HasNext);
        QueueChanged?.Invoke();
    }

    public void ApplyQueueItemsAdded(int count)
    {
        if (count > 0)
        {
            QueueItemsAdded?.Invoke(count);
        }
    }

    // Record the seek the user dropped the slider on as a projection, shown until
    // core confirms it. Returns null when there's no known position to project
    // from (nothing playing, or a zero-length track).
    public SeekProjection? ProjectSeek(double requestedProgress, double sliderMinimum, double sliderMaximum)
    {
        var nowPlaying = _nowPlaying;
        var state = nowPlaying?.Position;
        if (nowPlaying is null || state is null || state.Snapshot.DurationMs == 0)
        {
            BaeDiagnostics.Logger.Warning(
                $"Skipping seek projection for progress {requestedProgress} because playback position is unavailable for track {_nowPlaying?.TrackId ?? "<none>"}.");
            return null;
        }

        var projection = PlaybackPositionModel.ProjectSeek(
            nowPlaying.TrackId,
            state.Snapshot,
            requestedProgress,
            sliderMinimum,
            sliderMaximum);
        _nowPlaying = nowPlaying with { Position = state with { Projection = projection } };
        return projection;
    }

    public void ClearSeekProjection() => _nowPlaying = PlaybackPositionModel.ClearProjection(_nowPlaying);

    public void Reset()
    {
        _nowPlaying = null;
        _queueManual = new List<BridgeQueueEntry>();
        _queueContext = null;
        Revision = 0;
        IsMuted = false;
    }

    // Carry the current position forward only when the incoming track is the one
    // already on screen; a genuine track change starts with no position.
    private PlaybackPositionState? KeptPositionFor(string trackId) =>
        _nowPlaying is { } nowPlaying && nowPlaying.TrackId == trackId
            ? nowPlaying.Position
            : null;

    private void ApplyPositionSnapshot(string trackId, ulong durationMs, ulong positionMs, double progress)
    {
        var snapshot = new PlaybackPositionSnapshot(durationMs, positionMs, progress);
        _nowPlaying = PlaybackPositionModel.WithPosition(_nowPlaying, trackId, snapshot, null);
        PositionChanged?.Invoke(new PlaybackPositionRender(progress, positionMs, durationMs));
    }

    private bool Accept(string trackId)
    {
        switch (PlaybackPositionModel.ClassifyPlaybackPosition(_nowPlaying?.TrackId, trackId))
        {
            case PlaybackPositionRejection.MissingTrackId:
                BaeDiagnostics.Logger.Warning("Ignoring playback position with no track id.");
                return false;
            case PlaybackPositionRejection.StaleTrack:
                BaeDiagnostics.Logger.Warning(
                    $"Ignoring playback position for stale track {trackId}; current track is {_nowPlaying?.TrackId}.");
                return false;
            default:
                return true;
        }
    }
}

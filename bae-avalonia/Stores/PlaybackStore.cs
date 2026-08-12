using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The now-playing bar's display fields for a playing or paused track. PauseReason
// is null while playing and carries the pause reason while paused (which may
// prompt the side-ended dialog).
internal sealed record NowPlayingBarTrack(
    string Title,
    string Artist,
    BridgeImageRef? CoverImage,
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
// What the transport currently is, for click handlers that must compute an
// absolute command target. Playing covers loading — the bar shows the spinner
// where the pause control goes, and a press should pause the incoming track,
// matching the other platforms.
internal enum TransportPlayState { Stopped, Playing, Paused }

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

    // The queue revision the current lanes were resolved from. Page subscription
    // values are accepted only while they carry this revision.
    public ulong Revision { get; private set; }

    // What the transport currently is, for the play/pause button that must
    // compute an absolute command target. Written by the Apply* reducers.
    public TransportPlayState PlayState { get; private set; } = TransportPlayState.Stopped;

    // The current mute state, for the mute button that must compute an absolute
    // command target. Written by ApplyMute from the MuteChanged payload.
    public bool IsMuted { get; private set; }

    // The current output volume in [0, 1]. Core reports volume and mute as
    // separate events, so whichever arrives is paired with the other's last value
    // here before it goes to the OS now-playing surface.
    public float Volume { get; private set; } = 1.0f;

    // The current repeat mode, for the repeat button that must compute the next
    // mode as an absolute command target. Written by ApplyRepeat from the
    // RepeatModeChanged payload.
    public BridgeRepeatMode RepeatMode { get; private set; } = BridgeRepeatMode.Off;

    public event Action<NowPlayingBarTrack>? NowPlayingChanged;
    public event Action? PlaybackStopped;
    public event Action? LoadingStarted;
    public event Action<PlaybackPositionRender>? PositionChanged;
    public event Action<float>? VolumeChanged;
    public event Action<bool>? MuteChanged;
    public event Action<BridgeRepeatMode>? RepeatChanged;
    public event Action<bool, bool>? TransportChanged;
    public event Action<int>? QueueItemsAdded;

    // The lanes changed (from a mutation on any source). The non-modal queue pane
    // re-renders on this while visible; the now-playing bar reads only transport.
    public event Action? QueueChanged;

    public void ApplyPlaying(string albumId, string trackId, string trackTitle, string artistNames, BridgeImageRef? coverImage)
    {
        _nowPlaying = new NowPlayingState(albumId, trackId, KeptPositionFor(trackId));
        PlayState = TransportPlayState.Playing;
        NowPlayingChanged?.Invoke(new NowPlayingBarTrack(trackTitle, artistNames, coverImage, true, null));
    }

    public void ApplyPaused(string albumId, string trackId, string trackTitle, string artistNames, BridgeImageRef? coverImage, BridgePlaybackPauseReason reason)
    {
        _nowPlaying = new NowPlayingState(albumId, trackId, KeptPositionFor(trackId));
        PlayState = TransportPlayState.Paused;
        NowPlayingChanged?.Invoke(new NowPlayingBarTrack(trackTitle, artistNames, coverImage, false, reason));
    }

    public void ApplyStopped()
    {
        _nowPlaying = null;
        PlayState = TransportPlayState.Stopped;
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

    // A loading transition. Once core has resolved the target (`track` is set),
    // switch the bar to it — so the in-app bar matches the taskbar and the other
    // platforms while audio still downloads. The first, bare loading event
    // (`track` null, e.g. a seek re-entering loading) keeps the current track on
    // screen behind the spinner. The None-then-Some sequencing is core's; this
    // renders it.
    public void ApplyLoading(string trackId, BridgeLoadingTrackInfo? track)
    {
        if (track is { } target)
        {
            _nowPlaying = new NowPlayingState(target.AlbumId, trackId, KeptPositionFor(trackId));
            NowPlayingChanged?.Invoke(
                new NowPlayingBarTrack(target.TrackTitle, target.ArtistNames, target.CoverImage, true, null));
        }
        else
        {
            _nowPlaying = PlaybackPositionModel.BeginLoading(_nowPlaying, trackId);
        }
        PlayState = TransportPlayState.Playing;
        LoadingStarted?.Invoke();
    }

    public void ApplyVolume(float volume)
    {
        Volume = volume;
        VolumeChanged?.Invoke(volume);
    }

    public void ApplyMute(bool isMuted)
    {
        IsMuted = isMuted;
        MuteChanged?.Invoke(isMuted);
    }

    public void ApplyRepeat(BridgeRepeatMode mode)
    {
        RepeatMode = mode;
        RepeatChanged?.Invoke(mode);
    }

    public void ApplyQueueValue(BridgeQueueSnapshot snapshot)
    {
        if (snapshot.Revision < Revision)
        {
            return;
        }
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
        Volume = 1.0f;
        RepeatMode = BridgeRepeatMode.Off;
        PlayState = TransportPlayState.Stopped;
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

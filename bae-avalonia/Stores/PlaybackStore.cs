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
// lanes. ApplyValues is the sole writer, driven by retained playback values; the
// now-playing bar subscribes to the change events and renders. Views read the
// snapshot properties and never write back.
// What the transport currently is, for click handlers that must compute an
// absolute command target. Playing covers loading — the bar shows the spinner
// where the pause control goes, and a press should pause the incoming track,
// matching the other platforms.
internal enum TransportPlayState { Stopped, Playing, Paused }

internal sealed class PlaybackStore
{
    private const int QueuePageSize = 100;
    private const int MaximumQueuePageSubscriptions = 3;

    private readonly QueueService _queue;
    private readonly Action<Exception> _queueError;
    private NowPlayingState? _nowPlaying;
    private ulong _lastSeekRevision;
    private readonly SortedDictionary<int, BridgeQueueEntry[]> _contextPages = new();
    private readonly Dictionary<QueuePageKey, IDisposable> _contextSubscriptions = new();

    public PlaybackStore(QueueService queue, Action<Exception> queueError)
    {
        _queue = queue;
        _queueError = queueError;
    }

#if DEBUG
    public PlaybackStore() : this(new QueueService(), _ => { }) { }
#endif

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
    // command target. Written by ApplyMute from the retained playback value.
    public bool IsMuted { get; private set; }

    // The current output volume in [0, 1], delivered with mute in one retained
    // playback value before both go to the OS now-playing surface.
    public float Volume { get; private set; } = 1.0f;

    // The current repeat mode, for the repeat button that must compute the next
    // mode as an absolute command target. Written by ApplyRepeat from the retained
    // playback value.
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
    public event Action? ContextPagesChanged;

    public BridgeQueueEntry? ContextItemAt(int index)
    {
        if (_queueContext is null || index < 0 || (ulong)index >= _queueContext.UpcomingTotal)
        {
            return null;
        }
        foreach (var (offset, entries) in _contextPages)
        {
            var relative = index - offset;
            if (relative >= 0 && relative < entries.Length)
            {
                return entries[relative];
            }
        }
        return null;
    }

    public void ReportVisibleContextRange(int first, int last)
    {
        if (_queueContext is not { } context || first > last)
        {
            CancelContextPages();
            return;
        }
        var total = checked((int)context.UpcomingTotal);
        var start = Math.Clamp(first, 0, total);
        var end = Math.Clamp(last + 1, 0, total);
        var wanted = new HashSet<QueuePageKey>();
        for (var offset = start / QueuePageSize * QueuePageSize; offset < end; offset += QueuePageSize)
        {
            wanted.Add(new QueuePageKey(offset, Math.Min(offset + QueuePageSize, total), Revision));
        }

        var removedPage = false;
        foreach (var key in _contextSubscriptions.Keys.Where(key => !wanted.Contains(key)).ToArray())
        {
            _contextSubscriptions[key].Dispose();
            _contextSubscriptions.Remove(key);
            if (key.Offset > 0)
            {
                removedPage |= _contextPages.Remove(key.Offset);
            }
        }
        foreach (var key in wanted.OrderBy(key => key.Offset).Take(MaximumQueuePageSubscriptions))
        {
            SubscribeContextPage(key);
        }
        if (removedPage)
        {
            ContextPagesChanged?.Invoke();
        }
    }

    private void SubscribeContextPage(QueuePageKey key)
    {
        if (_contextSubscriptions.ContainsKey(key) || key.Offset == 0 && _contextPages.ContainsKey(0))
        {
            return;
        }
        var subscription = _queue.SubscribeUpcomingPage(
            checked((uint)key.Offset),
            checked((uint)(key.End - key.Offset)),
            page => Avalonia.Threading.Dispatcher.UIThread.Post(() => ApplyContextPage(key, page)),
            error => Avalonia.Threading.Dispatcher.UIThread.Post(() => _queueError(error)));
        if (subscription is not null)
        {
            _contextSubscriptions[key] = subscription;
        }
    }

    private void ApplyContextPage(QueuePageKey key, BridgeQueueUpcomingPage page)
    {
        if (!_contextSubscriptions.ContainsKey(key) || page.Revision != Revision)
        {
            return;
        }
        _contextPages[key.Offset] = page.Entries;
        ContextPagesChanged?.Invoke();
    }

    public void ApplyValues(BridgePlaybackValues values, IMediaControl mediaControls)
    {
        switch (values.State)
        {
            case BridgePlaybackValueState.Stopped:
                ApplyStopped();
                mediaControls.UpdateNowPlayingStopped();
                break;
            case BridgePlaybackValueState.Loading loading:
                ApplyLoading(loading.TrackId, loading.Track);
                if (loading.Track is { } loadingTrack)
                {
                    mediaControls.UpdateNowPlayingLoading(
                        loadingTrack.TrackTitle, loadingTrack.ArtistNames, loadingTrack.AlbumTitle,
                        loadingTrack.CoverImage, loadingTrack.DurationMs);
                }
                break;
            case BridgePlaybackValueState.Playing playing:
                ApplyPlaying(
                    playing.AlbumId, playing.TrackId, playing.TrackTitle,
                    playing.ArtistNames, playing.CoverImage);
                mediaControls.UpdateNowPlayingPlaying(
                    playing.TrackTitle, playing.ArtistNames, playing.AlbumTitle,
                    playing.CoverImage, playing.DurationMs);
                break;
            case BridgePlaybackValueState.Paused paused:
                ApplyPaused(
                    paused.AlbumId, paused.TrackId, paused.TrackTitle,
                    paused.ArtistNames, paused.CoverImage, paused.Reason);
                mediaControls.UpdateNowPlayingPaused(
                    paused.TrackTitle, paused.ArtistNames, paused.AlbumTitle,
                    paused.CoverImage, paused.DurationMs);
                break;
        }
        if (values.Position is { } position)
        {
            if (values.SeekRevision != _lastSeekRevision)
            {
                ApplySeeked(position.TrackId, position.PositionMs, position.DurationMs, position.Progress);
                mediaControls.UpdateSeekedPosition(position.PositionMs, position.DurationMs);
            }
            else
            {
                ApplyProgress(position.TrackId, position.PositionMs, position.DurationMs, position.Progress);
                mediaControls.UpdatePosition(position.PositionMs, position.DurationMs);
            }
        }
        _lastSeekRevision = values.SeekRevision;
        ApplyVolume(values.Volume);
        ApplyMute(values.IsMuted);
        ApplyRepeat(values.RepeatMode);
        mediaControls.UpdateVolume(values.Volume, values.IsMuted);
    }

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
        _lastSeekRevision = 0;
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
    // platforms while audio still downloads. The first bare loading value
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
        if (snapshot.Revision > Revision)
        {
            CancelContextPages();
        }
        _queueManual = snapshot.Manual.ToList();
        _queueContext = snapshot.Context;
        _contextPages.Clear();
        if (snapshot.Context is { } context)
        {
            _contextPages[0] = context.Upcoming;
        }
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
        CancelContextPages();
        _nowPlaying = null;
        _queueManual = new List<BridgeQueueEntry>();
        _queueContext = null;
        _contextPages.Clear();
        Revision = 0;
        IsMuted = false;
        Volume = 1.0f;
        RepeatMode = BridgeRepeatMode.Off;
        PlayState = TransportPlayState.Stopped;
    }

    private void CancelContextPages()
    {
        foreach (var subscription in _contextSubscriptions.Values)
        {
            subscription.Dispose();
        }
        _contextSubscriptions.Clear();
        _contextPages.Clear();
        if (_queueContext is { } context)
        {
            _contextPages[0] = context.Upcoming;
        }
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

internal readonly record struct QueuePageKey(int Offset, int End, ulong Revision);

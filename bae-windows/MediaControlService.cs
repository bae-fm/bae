using System;
using System.Threading.Tasks;
using Microsoft.UI.Dispatching;
using Windows.Media;
using Windows.Storage.Streams;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// Owns the window's <see cref="SystemMediaTransportControls"/> and applies the
/// decisions of <see cref="MediaControlState"/>: metadata and status pushes,
/// timeline updates, the async artwork fetch, and the button / seek callbacks
/// (marshalled onto the UI thread). All decision logic lives in the state; this
/// shell only translates it into WinRT calls.
/// </summary>
internal sealed class MediaControlService
{
    private readonly SystemMediaTransportControls _smtc;
    private readonly DispatcherQueue _dispatcherQueue;
    private readonly PlaybackService _playback;
    private readonly Func<BridgeImageRef, byte[]?> _fetchLibraryImageBytes;
    private readonly MediaControlState _state = new();

    // Kept alive while the display updater references it as the current thumbnail.
    private InMemoryRandomAccessStream? _thumbnailStream;

    internal MediaControlService(
        IntPtr windowHandle,
        DispatcherQueue dispatcherQueue,
        PlaybackService playback,
        Func<BridgeImageRef, byte[]?> fetchLibraryImageBytes)
    {
        _dispatcherQueue = dispatcherQueue;
        _playback = playback;
        _fetchLibraryImageBytes = fetchLibraryImageBytes;

        _smtc = SystemMediaTransportControlsInterop.GetForWindow(windowHandle);
        // Play/pause stay armed for the whole session; next/previous track the
        // queue's has-next/has-previous. The session is inert until something
        // plays, at which point Apply enables it.
        _smtc.IsEnabled = false;
        _smtc.IsPlayEnabled = true;
        _smtc.IsPauseEnabled = true;
        _smtc.IsNextEnabled = false;
        _smtc.IsPreviousEnabled = false;
        _smtc.ButtonPressed += OnButtonPressed;
        // Registering this handler arms the system seek bar; the timeline pushes
        // tell it the track's extent.
        _smtc.PlaybackPositionChangeRequested += OnPlaybackPositionChangeRequested;
    }

    internal void UpdateNowPlayingPlaying(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs) =>
        Apply(
            _state.UpdateForTrack(
                trackTitle, artistNames, albumTitle, ArtworkToken(coverImage), durationMs,
                MediaControlPlaybackStatus.Playing),
            coverImage);

    internal void UpdateNowPlayingPaused(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs) =>
        Apply(
            _state.UpdateForTrack(
                trackTitle, artistNames, albumTitle, ArtworkToken(coverImage), durationMs,
                MediaControlPlaybackStatus.Paused),
            coverImage);

    internal void UpdateNowPlayingLoading(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs) =>
        Apply(
            _state.UpdateForTrack(
                trackTitle, artistNames, albumTitle, ArtworkToken(coverImage), durationMs,
                MediaControlPlaybackStatus.Changing),
            coverImage);

    internal void UpdateNowPlayingStopped()
    {
        _state.Clear();
        ClearSystemDisplay();
    }

    internal void UpdatePosition(ulong positionMs, ulong durationMs)
    {
        var timeline = _state.UpdatePosition(positionMs, durationMs);
        if (timeline is not null)
        {
            PushTimeline(timeline);
        }
    }

    internal void UpdateCommandAvailability(bool hasNext, bool hasPrevious)
    {
        _smtc.IsNextEnabled = hasNext;
        _smtc.IsPreviousEnabled = hasPrevious;
    }

    internal void UpdateNowPlayingForPreview(string path, ulong durationMs, bool isPlaying) =>
        Apply(_state.UpdateForPreview(path, durationMs, isPlaying));

    internal void UpdatePreviewIdle()
    {
        _state.ClearPreview();
        ClearSystemDisplay();
    }

    internal void UpdatePreviewPosition(ulong positionMs)
    {
        var timeline = _state.UpdatePreviewPosition(positionMs);
        if (timeline is not null)
        {
            PushTimeline(timeline);
        }
    }

    // Called on library teardown and window close. Idempotent, so a close after a
    // library close is fine.
    internal void Deactivate()
    {
        _state.Clear();
        ClearSystemDisplay();
    }

    // Pushes a display's metadata, artwork action, and status to the system, and
    // marks the session live. Null while a preview suppresses library updates.
    private void Apply(MediaControlDisplay? display, BridgeImageRef? coverImage = null)
    {
        if (display is null)
        {
            return;
        }

        var updater = _smtc.DisplayUpdater;
        updater.Type = MediaPlaybackType.Music;
        updater.MusicProperties.Title = display.Title;
        updater.MusicProperties.Artist = display.Artist;
        updater.MusicProperties.AlbumTitle = display.AlbumTitle;
        switch (display.Artwork)
        {
            case MediaControlArtwork.Keep:
                break;
            case MediaControlArtwork.Clear:
                updater.Thumbnail = null;
                _thumbnailStream?.Dispose();
                _thumbnailStream = null;
                break;
            case MediaControlArtwork.Load load:
                updater.Thumbnail = null;
                // A Load is only produced for a cover that is present, so the
                // reference behind the token is in hand.
                StartArtworkLoad(coverImage!, load.Token);
                break;
        }
        updater.Update();
        _smtc.PlaybackStatus = ToPlaybackStatus(display.Status);
        _smtc.IsEnabled = true;
    }

    private void ClearSystemDisplay()
    {
        _smtc.DisplayUpdater.ClearAll();
        _smtc.PlaybackStatus = MediaPlaybackStatus.Closed;
        _smtc.IsEnabled = false;
        _smtc.IsNextEnabled = false;
        _smtc.IsPreviousEnabled = false;
        _thumbnailStream?.Dispose();
        _thumbnailStream = null;
    }

    private void PushTimeline(MediaControlTimeline timeline) =>
        _smtc.UpdateTimelineProperties(new SystemMediaTransportControlsTimelineProperties
        {
            StartTime = TimeSpan.Zero,
            MinSeekTime = TimeSpan.Zero,
            Position = TimeSpan.FromMilliseconds(timeline.PositionMs),
            MaxSeekTime = TimeSpan.FromMilliseconds(timeline.DurationMs),
            EndTime = TimeSpan.FromMilliseconds(timeline.DurationMs),
        });

    private static MediaPlaybackStatus ToPlaybackStatus(MediaControlPlaybackStatus status) =>
        status switch
        {
            MediaControlPlaybackStatus.Playing => MediaPlaybackStatus.Playing,
            MediaControlPlaybackStatus.Paused => MediaPlaybackStatus.Paused,
            MediaControlPlaybackStatus.Changing => MediaPlaybackStatus.Changing,
            _ => throw new ArgumentOutOfRangeException(nameof(status), status, "Unknown media control status"),
        };

    private void OnButtonPressed(
        SystemMediaTransportControls sender, SystemMediaTransportControlsButtonPressedEventArgs args)
    {
        var button = args.Button;
        _dispatcherQueue.TryEnqueue(() => HandleButton(button));
    }

    // Routes a system button to the same core commands the on-screen transport
    // uses, splitting on whether a preview clip is showing.
    private void HandleButton(SystemMediaTransportControlsButton button)
    {
        var previewing = _state.IsShowingPreview;
        var dispatched = button switch
        {
            SystemMediaTransportControlsButton.Play when previewing => _playback.PreviewTogglePause(),
            SystemMediaTransportControlsButton.Pause when previewing => _playback.PreviewTogglePause(),
            SystemMediaTransportControlsButton.Play => _playback.Resume(),
            SystemMediaTransportControlsButton.Pause => _playback.Pause(),
            SystemMediaTransportControlsButton.Next when previewing => _playback.PreviewStop(),
            SystemMediaTransportControlsButton.Previous when previewing => _playback.PreviewStop(),
            SystemMediaTransportControlsButton.Next => _playback.NextTrack(),
            SystemMediaTransportControlsButton.Previous => _playback.PreviousTrack(),
            // Buttons this service never enables (stop, FF, rewind, shuffle,
            // repeat) — nothing to dispatch.
            _ => true,
        };
        if (!dispatched)
        {
            // A button can race library teardown: the session was cleared between
            // the press and this dispatch. Dropping the command is correct.
            BaeDiagnostics.Logger.Debug($"Dropped system media button {button}: no open library handle.");
        }
    }

    private void OnPlaybackPositionChangeRequested(
        SystemMediaTransportControls sender, PlaybackPositionChangeRequestedEventArgs args)
    {
        var requestedMs = args.RequestedPlaybackPosition.TotalMilliseconds;
        _dispatcherQueue.TryEnqueue(() => HandleSeek(requestedMs));
    }

    private void HandleSeek(double requestedMs)
    {
        if (_state.SeekRatio(requestedMs) is not { } ratio)
        {
            BaeDiagnostics.Logger.Warning("Ignoring system seek request: no current duration tracked.");
            return;
        }

        bool dispatched;
        if (_state.IsShowingPreview)
        {
            dispatched = _playback.PreviewSeekByRatio(ratio);
        }
        else
        {
            // Move the system timeline to the seek target now; the PlaybackSeeked
            // event that follows corrects it. Duration is known whenever a ratio is.
            if (_state.CurrentDurationMs is { } durationMs)
            {
                var timeline = _state.UpdatePosition((ulong)(ratio * durationMs), durationMs);
                if (timeline is not null)
                {
                    PushTimeline(timeline);
                }
            }
            dispatched = _playback.SeekByRatio(ratio);
        }
        if (!dispatched)
        {
            // A seek can race library teardown, same as the buttons.
            BaeDiagnostics.Logger.Debug("Dropped system seek request: no open library handle.");
        }
    }

    // The state layer names artwork by token (it compiles without the generated
    // bridge bindings), so the reference the fetch needs travels alongside it.
    private static string? ArtworkToken(BridgeImageRef? image) =>
        image is null ? null : $"{image.ImageType}:{image.Id}:{image.Version}";

    private void StartArtworkLoad(BridgeImageRef image, string token)
    {
        _ = Task.Run(() => _fetchLibraryImageBytes(image)).ContinueWith(task =>
        {
            var bytes = task.Status == TaskStatus.RanToCompletion ? task.Result : null;
            if (task.Exception is not null)
            {
                BaeDiagnostics.Logger.Warning("Failed to read system media thumbnail.", task.Exception);
            }
            _dispatcherQueue.TryEnqueue(() => ApplyArtwork(token, bytes));
        }, TaskScheduler.Default);
    }

    // Writes the fetched cover into the display's thumbnail, unless the track
    // changed while the fetch was in flight (stale load) or the fetch produced
    // nothing.
    private async void ApplyArtwork(string token, byte[]? bytes)
    {
        if (!_state.ArtworkLoadIsCurrent(token))
        {
            // A newer track's load owns the thumbnail slot now.
            BaeDiagnostics.Logger.Debug("Skipping stale system media thumbnail load.");
            return;
        }
        if (bytes is null)
        {
            // The fetch threw (logged at the fetch site) or the library handle
            // closed mid-fetch. Forget the id so the next update for this cover
            // retries the load instead of keeping a thumbnail that never arrived.
            BaeDiagnostics.Logger.Debug("System media thumbnail fetch produced no bytes; will retry on the next update.");
            _state.ArtworkLoadFailed(token);
            return;
        }

        var stream = new InMemoryRandomAccessStream();
        using (var writer = new DataWriter(stream))
        {
            writer.WriteBytes(bytes);
            await writer.StoreAsync();
            writer.DetachStream();
        }
        stream.Seek(0);

        // The write yielded the UI thread; re-check the load is still current so a
        // track change during it doesn't leave a stale thumbnail.
        if (!_state.ArtworkLoadIsCurrent(token))
        {
            BaeDiagnostics.Logger.Debug("Skipping stale system media thumbnail load.");
            stream.Dispose();
            return;
        }

        _thumbnailStream?.Dispose();
        _thumbnailStream = stream;
        _smtc.DisplayUpdater.Thumbnail = RandomAccessStreamReference.CreateFromStream(stream);
        _smtc.DisplayUpdater.Update();
    }
}

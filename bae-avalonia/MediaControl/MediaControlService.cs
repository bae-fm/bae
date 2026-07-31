using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Applies the decisions of <see cref="MediaControlState"/> to the platform's
/// now-playing surface: metadata and status pushes, timeline updates, the async
/// artwork fetch, and the command / seek / volume callbacks (marshalled onto the
/// UI thread). All decision logic lives in the state and every platform call
/// lives behind <see cref="IMediaSession"/>; this shell only translates between
/// the two.
/// </summary>
internal sealed class MediaControlService : IMediaControl
{
    private readonly IMediaSession _session;
    private readonly Dispatcher _dispatcher;
    private readonly PlaybackService _playback;
    private readonly Func<BridgeImageRef, byte[]?> _fetchLibraryImageBytes;
    private readonly MediaControlState _state = new();

    // The last mute state core reported, so a volume written by an OS client knows
    // whether it has to unmute first.
    private bool _isMuted;

    /// <summary>The now-playing surface for the platform this build runs on: the
    /// system transport controls on Windows, MPRIS on Linux, and nothing
    /// elsewhere — those two are the shipped desktop targets, and a host without
    /// either runs with no OS surface rather than a broken one.</summary>
    internal static IMediaControl ForCurrentPlatform(
        Dispatcher dispatcher,
        PlaybackService playback,
        MediaPathsService mediaPaths,
        string edition,
        Action raise,
        Action quit)
    {
#if WINDOWS10_0_19041_0_OR_GREATER
        return new MediaControlService(
            new SmtcMediaSession(), dispatcher, playback, mediaPaths.FetchLibraryImageBytes);
#else
        if (!OperatingSystem.IsLinux())
        {
            return new NoopMediaControl();
        }

        var mpris = new MprisMediaSession(edition, raise, quit);
        var control = new MediaControlService(mpris, dispatcher, playback, mediaPaths.FetchLibraryImageBytes);
        // The bus name goes up only after the service is listening, so a client
        // command cannot land while there is nothing to route it to.
        mpris.Serve();
        return control;
#endif
    }

    internal MediaControlService(
        IMediaSession session,
        Dispatcher dispatcher,
        PlaybackService playback,
        Func<BridgeImageRef, byte[]?> fetchLibraryImageBytes)
    {
        _session = session;
        _dispatcher = dispatcher;
        _playback = playback;
        _fetchLibraryImageBytes = fetchLibraryImageBytes;

        _session.CommandRequested += OnCommandRequested;
        _session.SeekRequestedMs += OnSeekRequested;
        _session.VolumeRequested += OnVolumeRequested;
    }

    public void SetWindow(Window? window) => _session.SetWindow(window);

    public void UpdateNowPlayingPlaying(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs) =>
        Apply(
            _state.UpdateForTrack(
                trackTitle, artistNames, albumTitle, ArtworkToken(coverImage), durationMs,
                MediaControlPlaybackStatus.Playing),
            coverImage);

    public void UpdateNowPlayingPaused(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs) =>
        Apply(
            _state.UpdateForTrack(
                trackTitle, artistNames, albumTitle, ArtworkToken(coverImage), durationMs,
                MediaControlPlaybackStatus.Paused),
            coverImage);

    public void UpdateNowPlayingLoading(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs) =>
        Apply(
            _state.UpdateForTrack(
                trackTitle, artistNames, albumTitle, ArtworkToken(coverImage), durationMs,
                MediaControlPlaybackStatus.Changing),
            coverImage);

    public void UpdateNowPlayingStopped()
    {
        _state.Clear();
        _session.Clear();
    }

    public void UpdatePosition(ulong positionMs, ulong durationMs) =>
        PushPosition(positionMs, durationMs, seeked: false);

    public void UpdateSeekedPosition(ulong positionMs, ulong durationMs) =>
        PushPosition(positionMs, durationMs, seeked: true);

    public void UpdateCommandAvailability(bool hasNext, bool hasPrevious) =>
        _session.SetCommandAvailability(hasNext, hasPrevious);

    public void UpdateVolume(float volume, bool isMuted)
    {
        _isMuted = isMuted;
        // Muted output reads as silence on the surface rather than as a volume the
        // user set: a client showing the slider shows what is actually audible.
        _session.SetVolume(isMuted ? 0.0 : volume);
    }

    public void UpdateNowPlayingForPreview(string path, ulong durationMs, bool isPlaying) =>
        Apply(_state.UpdateForPreview(path, durationMs, isPlaying));

    public void UpdatePreviewIdle()
    {
        _state.ClearPreview();
        _session.Clear();
    }

    public void UpdatePreviewPosition(ulong positionMs)
    {
        if (_state.UpdatePreviewPosition(positionMs) is { } timeline)
        {
            _session.SetTimeline(timeline, seeked: false);
        }
    }

    // Called on library teardown and window close. Idempotent, so a close after a
    // library close is fine.
    public void Deactivate()
    {
        _state.Clear();
        _session.Clear();
    }

    public void Dispose()
    {
        _session.CommandRequested -= OnCommandRequested;
        _session.SeekRequestedMs -= OnSeekRequested;
        _session.VolumeRequested -= OnVolumeRequested;
        _session.Dispose();
    }

    // Pushes a display's metadata, artwork action, and status to the surface, and
    // marks the session live. Null while a preview suppresses library updates.
    private void Apply(MediaControlDisplay? display, BridgeImageRef? coverImage = null)
    {
        if (display is null)
        {
            return;
        }

        _session.Apply(display);
        if (display.Artwork is MediaControlArtwork.Load load)
        {
            // A Load is only produced for a cover that is present, so the
            // reference behind the token is in hand.
            StartArtworkLoad(coverImage!, load.Token);
        }
    }

    private void PushPosition(ulong positionMs, ulong durationMs, bool seeked)
    {
        if (_state.UpdatePosition(positionMs, durationMs) is { } timeline)
        {
            _session.SetTimeline(timeline, seeked);
        }
    }

    private void OnCommandRequested(MediaSessionCommand command) =>
        _dispatcher.Post(() => HandleCommand(command));

    // Routes a surface command to the same core commands the on-screen transport
    // uses, splitting on whether a preview clip is showing.
    private void HandleCommand(MediaSessionCommand command)
    {
        var previewing = _state.IsShowingPreview;
        var dispatched = command switch
        {
            MediaSessionCommand.Play when previewing => _playback.PreviewTogglePause(),
            MediaSessionCommand.Pause when previewing => _playback.PreviewTogglePause(),
            MediaSessionCommand.Play => _playback.Resume(),
            MediaSessionCommand.Pause => _playback.Pause(),
            MediaSessionCommand.Next when previewing => _playback.PreviewStop(),
            MediaSessionCommand.Previous when previewing => _playback.PreviewStop(),
            MediaSessionCommand.Stop when previewing => _playback.PreviewStop(),
            MediaSessionCommand.Next => _playback.NextTrack(),
            MediaSessionCommand.Previous => _playback.PreviousTrack(),
            MediaSessionCommand.Stop => _playback.Stop(),
            _ => throw new ArgumentOutOfRangeException(nameof(command), command, "Unrouted media session command"),
        };
        if (!dispatched)
        {
            // A command can race library teardown: the session was cleared between
            // the press and this dispatch. Dropping the command is correct.
            BaeDiagnostics.Logger.Debug($"Dropped system media command {command}: no open library handle.");
        }
    }

    private void OnSeekRequested(double requestedMs) =>
        _dispatcher.Post(() => HandleSeek(requestedMs));

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
            // Move the surface's timeline to the seek target now; the
            // PlaybackSeeked event that follows corrects it. Duration is known
            // whenever a ratio is.
            if (_state.CurrentDurationMs is { } durationMs)
            {
                PushPosition((ulong)(ratio * durationMs), durationMs, seeked: true);
            }
            dispatched = _playback.SeekByRatio(ratio);
        }
        if (!dispatched)
        {
            // A seek can race library teardown, same as the commands.
            BaeDiagnostics.Logger.Debug("Dropped system seek request: no open library handle.");
        }
    }

    private void OnVolumeRequested(double volume) =>
        _dispatcher.Post(() => HandleVolume(volume));

    private void HandleVolume(double volume)
    {
        var requested = (float)Math.Clamp(volume, 0.0, 1.0);
        // A client that raises the slider off zero means "make this audible", so
        // a muted app unmutes first; the volume write alone would stay silent.
        if (_isMuted && requested > 0 && !_playback.SetMuted(false))
        {
            BaeDiagnostics.Logger.Debug("Dropped system unmute: no open library handle.");
            return;
        }
        if (!_playback.SetVolume(requested))
        {
            BaeDiagnostics.Logger.Debug("Dropped system volume write: no open library handle.");
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
                BaeDiagnostics.Logger.Warning("Failed to read the system now-playing cover.", task.Exception);
            }
            _dispatcher.Post(() => ApplyArtwork(token, bytes));
        }, TaskScheduler.Default);
    }

    // Hands the fetched cover to the surface, unless the track changed while the
    // fetch was in flight (stale load) or the fetch produced nothing.
    private void ApplyArtwork(string token, byte[]? bytes)
    {
        if (!_state.ArtworkLoadIsCurrent(token))
        {
            // A newer track's load owns the artwork slot now.
            BaeDiagnostics.Logger.Debug("Skipping stale system now-playing cover load.");
            return;
        }
        if (bytes is null)
        {
            // The fetch threw (logged at the fetch site) or the library handle
            // closed mid-fetch. Forget the id so the next update for this cover
            // retries the load instead of keeping an image that never arrived.
            BaeDiagnostics.Logger.Debug("System now-playing cover fetch produced no bytes; will retry on the next update.");
            _state.ArtworkLoadFailed(token);
            return;
        }

        _session.SetArtwork(bytes);
    }
}

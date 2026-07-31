using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Windows.Media;
using Windows.Storage.Streams;

namespace Bae.Desktop;

/// <summary>
/// The Windows now-playing surface: the system media transport controls bound to
/// the library window's handle. Publishes metadata, status, artwork and the
/// timeline through the display updater, and raises the button presses and seek
/// requests the system sends back.
/// </summary>
internal sealed class SmtcMediaSession : IMediaSession
{
    private SystemMediaTransportControls? _smtc;

    // Kept alive while the display updater references it as the current thumbnail.
    private InMemoryRandomAccessStream? _thumbnailStream;

    // Bumped every time the thumbnail slot is dropped or replaced, so a copy that
    // yielded mid-flight can tell whether it still owns the slot.
    private int _artworkGeneration;

    public event Action<MediaSessionCommand>? CommandRequested;
    public event Action<double>? SeekRequestedMs;

    // The system transport controls carry no volume slider, so nothing here ever
    // asks the app to change it. The member is the transport contract's, which
    // MPRIS does use, hence the never-raised suppression.
#pragma warning disable CS0067
    public event Action<double>? VolumeRequested;
#pragma warning restore CS0067

    // Binds to the given window's handle, releasing whichever window was bound
    // before. A null window (or a window with no handle yet) leaves no session at
    // all and drops every push — playback cannot run without a library window, so
    // nothing is lost.
    public void SetWindow(Window? window)
    {
        Unbind();
        if (window?.TryGetPlatformHandle()?.Handle is not { } handle || handle == IntPtr.Zero)
        {
            return;
        }

        var smtc = SystemMediaTransportControlsInterop.GetForWindow(handle);
        // Play/pause stay armed for the whole session; next/previous track the
        // queue's has-next/has-previous. The session is inert until something
        // plays, at which point Apply enables it.
        smtc.IsEnabled = false;
        smtc.IsPlayEnabled = true;
        smtc.IsPauseEnabled = true;
        smtc.IsNextEnabled = false;
        smtc.IsPreviousEnabled = false;
        smtc.ButtonPressed += OnButtonPressed;
        // Registering this handler arms the system seek bar; the timeline pushes
        // tell it the track's extent.
        smtc.PlaybackPositionChangeRequested += OnPlaybackPositionChangeRequested;
        _smtc = smtc;
    }

    public void Apply(MediaControlDisplay display)
    {
        if (_smtc is not { } smtc)
        {
            return;
        }

        var updater = smtc.DisplayUpdater;
        updater.Type = MediaPlaybackType.Music;
        updater.MusicProperties.Title = display.Title;
        updater.MusicProperties.Artist = display.Artist;
        updater.MusicProperties.AlbumTitle = display.AlbumTitle;
        if (display.Artwork is not MediaControlArtwork.Keep)
        {
            DropThumbnail(updater);
        }
        updater.Update();
        smtc.PlaybackStatus = ToPlaybackStatus(display.Status);
        smtc.IsEnabled = true;
    }

    public void SetArtwork(byte[] bytes)
    {
        var generation = ++_artworkGeneration;
        _ = WriteThumbnailAsync(bytes, generation);
    }

    public void SetTimeline(MediaControlTimeline timeline, bool seeked) =>
        // Windows draws one timeline either way: a jump and a tick are the same
        // push, so the seek flag has nothing to add here.
        _smtc?.UpdateTimelineProperties(new SystemMediaTransportControlsTimelineProperties
        {
            StartTime = TimeSpan.Zero,
            MinSeekTime = TimeSpan.Zero,
            Position = TimeSpan.FromMilliseconds(timeline.PositionMs),
            MaxSeekTime = TimeSpan.FromMilliseconds(timeline.DurationMs),
            EndTime = TimeSpan.FromMilliseconds(timeline.DurationMs),
        });

    public void SetCommandAvailability(bool hasNext, bool hasPrevious)
    {
        if (_smtc is not { } smtc)
        {
            return;
        }

        smtc.IsNextEnabled = hasNext;
        smtc.IsPreviousEnabled = hasPrevious;
    }

    public void SetVolume(double volume)
    {
        // The system transport controls read the app's volume from the audio
        // session, not from the now-playing metadata.
    }

    public void Clear()
    {
        if (_smtc is { } smtc)
        {
            ClearDisplay(smtc);
        }
    }

    public void Dispose() => Unbind();

    private void Unbind()
    {
        if (_smtc is not { } smtc)
        {
            return;
        }

        ClearDisplay(smtc);
        smtc.ButtonPressed -= OnButtonPressed;
        smtc.PlaybackPositionChangeRequested -= OnPlaybackPositionChangeRequested;
        _smtc = null;
    }

    private void ClearDisplay(SystemMediaTransportControls smtc)
    {
        smtc.DisplayUpdater.ClearAll();
        smtc.PlaybackStatus = MediaPlaybackStatus.Closed;
        smtc.IsEnabled = false;
        smtc.IsNextEnabled = false;
        smtc.IsPreviousEnabled = false;
        DropThumbnail(smtc.DisplayUpdater);
    }

    private void DropThumbnail(SystemMediaTransportControlsDisplayUpdater updater)
    {
        _artworkGeneration++;
        updater.Thumbnail = null;
        _thumbnailStream?.Dispose();
        _thumbnailStream = null;
    }

    // Copying the bytes into a WinRT stream yields, so the generation stamped at
    // the call is re-checked before the result reaches the display: a track that
    // changed in between owns the slot now.
    private async Task WriteThumbnailAsync(byte[] bytes, int generation)
    {
        try
        {
            var stream = new InMemoryRandomAccessStream();
            using (var writer = new DataWriter(stream))
            {
                writer.WriteBytes(bytes);
                await writer.StoreAsync();
                writer.DetachStream();
            }
            stream.Seek(0);

            if (_smtc is not { } smtc || generation != _artworkGeneration)
            {
                stream.Dispose();
                return;
            }

            _thumbnailStream?.Dispose();
            _thumbnailStream = stream;
            smtc.DisplayUpdater.Thumbnail = RandomAccessStreamReference.CreateFromStream(stream);
            smtc.DisplayUpdater.Update();
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Failed to publish the system now-playing cover.", exception);
        }
    }

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
        if (ToCommand(args.Button) is { } command)
        {
            CommandRequested?.Invoke(command);
        }
    }

    // The buttons this session arms. Stop, fast-forward, rewind, record, channel
    // and the shuffle / repeat toggles are never enabled, so no press arrives for
    // them and there is nothing to route.
    private static MediaSessionCommand? ToCommand(SystemMediaTransportControlsButton button) =>
        button switch
        {
            SystemMediaTransportControlsButton.Play => MediaSessionCommand.Play,
            SystemMediaTransportControlsButton.Pause => MediaSessionCommand.Pause,
            SystemMediaTransportControlsButton.Next => MediaSessionCommand.Next,
            SystemMediaTransportControlsButton.Previous => MediaSessionCommand.Previous,
            _ => null,
        };

    private void OnPlaybackPositionChangeRequested(
        SystemMediaTransportControls sender, PlaybackPositionChangeRequestedEventArgs args) =>
        SeekRequestedMs?.Invoke(args.RequestedPlaybackPosition.TotalMilliseconds);
}

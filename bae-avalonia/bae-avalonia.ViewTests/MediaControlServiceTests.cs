using Avalonia.Controls;
using Avalonia.Threading;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class MediaControlServiceTests
{
    [Fact]
    public void ResolvedLibraryPlaybackReplacesPreview()
    {
        var session = new RecordingMediaSession();
        using var service = new MediaControlService(
            session,
            Dispatcher.UIThread,
            new PlaybackService(),
            _ => null);
        service.ApplyMediaControlValues(new BridgeMediaControlValues(
            new BridgeMediaControlPlayback.Preview(
                new BridgePreviewTarget("/tmp/Preview Clip.flac", 0, null),
                120_000,
                10_000,
                true),
            1,
            false));

        service.ApplyMediaControlValues(new BridgeMediaControlValues(
            new BridgeMediaControlPlayback.Library(
                new BridgePlaybackValueState.Playing(
                    "track-1",
                    "Track Title",
                    "Artist Name",
                    "artist-1",
                    "album-1",
                    "Album Title",
                    null,
                    200_000),
                new BridgePlaybackPosition("track-1", 30_000, 200_000, 0.15),
                0),
            1,
            false));

        Assert.Equal("Track Title", session.Display?.Title);
        Assert.Equal(MediaControlPlaybackStatus.Playing, session.Display?.Status);
    }

    private sealed class RecordingMediaSession : IMediaSession
    {
        public event Action<MediaSessionCommand>? CommandRequested
        {
            add { }
            remove { }
        }

        public event Action<double>? SeekRequestedMs
        {
            add { }
            remove { }
        }

        public event Action<double>? VolumeRequested
        {
            add { }
            remove { }
        }

        internal MediaControlDisplay? Display { get; private set; }

        public void Apply(MediaControlDisplay display) => Display = display;
        public void Clear() => Display = null;
        public void Dispose() { }
        public void SetArtwork(byte[] bytes) { }
        public void SetCommandAvailability(bool hasNext, bool hasPrevious) { }
        public void SetTimeline(MediaControlTimeline timeline, bool seeked) { }
        public void SetVolume(double volume) { }
        public void SetWindow(Window? window) { }
    }
}

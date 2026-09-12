using System.Collections.Generic;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The playback mirror's snapshot state: what a control reads off the store
/// between the events that change it.
/// </summary>
public sealed class PlaybackStoreTests
{
    /// <summary>Loading runs from the transition until core reports the track
    /// playing or paused. A control that attaches mid-transition reads it rather
    /// than inferring it from the event it happened to catch.</summary>
    [Fact]
    public void LoadingRunsFromTheTransitionUntilTheTrackPlays()
    {
        var store = new PlaybackStore(new QueueService(), _ => { });

        Assert.False(store.IsLoading);

        store.ApplyLoading("track", null);
        Assert.True(store.IsLoading);

        store.ApplyPlaying("album", "track", "Title", "Artist", null);
        Assert.False(store.IsLoading);

        store.ApplyLoading("track", null);
        store.ApplyPaused("album", "track", "Title", "Artist", null, new BridgePlaybackPauseReason.Manual());
        Assert.False(store.IsLoading);

        store.ApplyLoading("track", null);
        store.ApplyStopped();
        Assert.False(store.IsLoading);
    }

    /// <summary>The resolved target arrives as a loading value too, so the track
    /// the bar switches to is already the loading one when it is handed over.</summary>
    [Fact]
    public void AResolvedLoadingTargetIsHandedOverAsLoading()
    {
        var store = new PlaybackStore(new QueueService(), _ => { });
        var loading = new List<bool>();
        store.NowPlayingChanged += _ => loading.Add(store.IsLoading);

        store.ApplyLoading("track", new BridgeLoadingTrackInfo(
            TrackTitle: "Title",
            ArtistNames: "Artist",
            AlbumId: "album",
            AlbumTitle: "Album",
            CoverImage: null,
            DurationMs: 200_000));
        store.ApplyPlaying("album", "track", "Title", "Artist", null);

        Assert.Equal(new[] { true, false }, loading);
    }

    /// <summary>A pause at a side boundary carries core's prompt, which is what
    /// the bar shows where the artist names go.</summary>
    [Fact]
    public void APauseAtASideBoundaryCarriesItsPrompt()
    {
        var store = new PlaybackStore(new QueueService(), _ => { });
        NowPlayingBarTrack? handed = null;
        store.NowPlayingChanged += track => handed = track;

        store.ApplyPaused(
            "album",
            "track",
            "Title",
            "Artist",
            null,
            new BridgePlaybackPauseReason.SideEnded(new BridgeSidePausePrompt(
                Id: "side-A",
                TitleKey: "core.playback.pause.side_ended.title",
                SideLabel: "A",
                MessageKey: "core.playback.pause.side_ended.message.vinyl")));

        Assert.Equal("A", handed?.SidePausePrompt?.SideLabel);

        store.ApplyPaused("album", "track", "Title", "Artist", null, new BridgePlaybackPauseReason.Manual());

        Assert.Null(handed?.SidePausePrompt);
    }
}

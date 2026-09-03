using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the decision logic in <see cref="MediaControlState"/> — the pure state
/// machine that turns core's selected playback into the system display and
/// remembers whether commands act on the library queue or a preview clip. The
/// WinRT shell that applies these decisions is verified by compilation, not here.
/// </summary>
public sealed class MediaControlStateTests
{
    private const string TrackTitle = "Track Title";
    private const string ArtistName = "Artist Name";
    private const string AlbumTitle = "Album Title";
    private const ulong DurationMs = 200_000;

    private static MediaControlDisplay Track(
        MediaControlState state,
        string? coverToken = "img-1",
        ulong durationMs = DurationMs,
        MediaControlPlaybackStatus status = MediaControlPlaybackStatus.Playing) =>
        state.UpdateForTrack(TrackTitle, ArtistName, AlbumTitle, coverToken, durationMs, status);

    // ── Track metadata and status ──────────────────────────────────────────────

    // The status enum is internal, so it can't appear in a public [Theory]
    // signature; drive the three cases from the body instead.
    [Fact]
    public void UpdateForTrack_PassesMetadataAndStatus()
    {
        AssertTrackStatus(MediaControlPlaybackStatus.Playing);
        AssertTrackStatus(MediaControlPlaybackStatus.Paused);
        AssertTrackStatus(MediaControlPlaybackStatus.Changing);
    }

    private static void AssertTrackStatus(MediaControlPlaybackStatus status)
    {
        var state = new MediaControlState();

        var display = Track(state, status: status);

        Assert.Equal(status, display.Status);
        Assert.Equal(TrackTitle, display.Title);
        Assert.Equal(ArtistName, display.Artist);
        Assert.Equal(AlbumTitle, display.AlbumTitle);
    }

    // ── Artwork: load / keep / clear ───────────────────────────────────────────

    [Fact]
    public void Artwork_LoadsNewIdKeepsSameIdClearsNull()
    {
        var state = new MediaControlState();

        Assert.Equal("img-1", Assert.IsType<MediaControlArtwork.Load>(Track(state, "img-1").Artwork).Token);
        Assert.IsType<MediaControlArtwork.Keep>(Track(state, "img-1").Artwork);
        Assert.Equal("img-2", Assert.IsType<MediaControlArtwork.Load>(Track(state, "img-2").Artwork).Token);
        Assert.IsType<MediaControlArtwork.Clear>(Track(state, null).Artwork);
    }

    [Fact]
    public void Artwork_SameIdLoadsAgainAfterClear()
    {
        var state = new MediaControlState();

        Assert.IsType<MediaControlArtwork.Load>(Track(state, "img-1").Artwork);
        state.Clear();
        Assert.IsType<MediaControlArtwork.Load>(Track(state, "img-1").Artwork);
    }

    // ── Stale-artwork guard ────────────────────────────────────────────────────

    [Fact]
    public void ArtworkLoadFailed_SameIdLoadsAgainStaleFailureIgnored()
    {
        var state = new MediaControlState();

        Assert.IsType<MediaControlArtwork.Load>(Track(state, "img-1").Artwork);
        state.ArtworkLoadFailed("img-1");
        Assert.IsType<MediaControlArtwork.Load>(Track(state, "img-1").Artwork);

        Track(state, "img-2");
        state.ArtworkLoadFailed("img-1");
        Assert.True(state.ArtworkLoadIsCurrent("img-2"));
        Assert.IsType<MediaControlArtwork.Keep>(Track(state, "img-2").Artwork);
    }

    [Fact]
    public void ArtworkLoadIsCurrent_TrueForCurrentIdFalseAfterChange()
    {
        var state = new MediaControlState();

        Track(state, "img-1");
        Assert.True(state.ArtworkLoadIsCurrent("img-1"));

        Track(state, "img-2");
        Assert.False(state.ArtworkLoadIsCurrent("img-1"));
        Assert.True(state.ArtworkLoadIsCurrent("img-2"));

        state.Clear();
        Assert.False(state.ArtworkLoadIsCurrent("img-2"));
    }

    // ── Position ───────────────────────────────────────────────────────────────

    [Fact]
    public void UpdatePosition_NullBeforeDisplayNonNullAfterTrack()
    {
        var state = new MediaControlState();

        Assert.Null(state.UpdatePosition(1_000, DurationMs));

        Track(state);
        var timeline = state.UpdatePosition(1_000, DurationMs);

        Assert.NotNull(timeline);
        Assert.Equal(1_000ul, timeline!.PositionMs);
        Assert.Equal(DurationMs, timeline.DurationMs);
    }

    [Fact]
    public void UpdatePosition_RefreshesDurationUsedBySeek()
    {
        var state = new MediaControlState();

        Track(state, durationMs: DurationMs);
        state.UpdatePosition(10_000, 100_000);

        // Seek reads the position event's duration, not the track's original one.
        Assert.Equal(0.5, state.SeekRatio(50_000));
    }

    // ── Seek ratio ─────────────────────────────────────────────────────────────

    [Fact]
    public void SeekRatio_NullWithoutDuration()
    {
        var state = new MediaControlState();

        Assert.Null(state.SeekRatio(1_000));
    }

    [Theory]
    [InlineData(0.0, 0.0)]
    [InlineData(100_000.0, 0.5)]
    [InlineData(200_000.0, 1.0)]
    [InlineData(300_000.0, 1.0)]  // above duration clamps to 1
    [InlineData(-5_000.0, 0.0)]   // negative clamps to 0
    public void SeekRatio_ClampsToUnitInterval(double requestedMs, double expected)
    {
        var state = new MediaControlState();

        Track(state, durationMs: DurationMs);

        Assert.Equal(expected, state.SeekRatio(requestedMs));
    }

    // ── Preview takeover ───────────────────────────────────────────────────────

    [Fact]
    public void Preview_TracksCommandRoutingAndTimeline()
    {
        var state = new MediaControlState();

        state.UpdateForPreview("C:\\clips\\preview-clip.flac", 120_000, isPlaying: true);

        Assert.True(state.IsShowingPreview);
        Assert.Equal(120_000ul, state.CurrentDurationMs);
        Assert.Null(state.UpdatePosition(1_000, DurationMs));
    }

    [Fact]
    public void Preview_TitleIsFileName()
    {
        var state = new MediaControlState();

        var display = state.UpdateForPreview("C:\\clips\\preview-clip.flac", 120_000, isPlaying: false);

        Assert.Equal(MediaControlPlaybackStatus.Paused, display.Status);
        Assert.Equal("preview-clip.flac", display.Title);
        Assert.Equal(string.Empty, display.Artist);
        Assert.Equal(string.Empty, display.AlbumTitle);
        Assert.IsType<MediaControlArtwork.Clear>(display.Artwork);
    }

    [Fact]
    public void UpdatePreviewPosition_UsesPreviewDuration()
    {
        var state = new MediaControlState();

        state.UpdateForPreview("C:\\clips\\preview-clip.flac", 120_000, isPlaying: true);
        var timeline = state.UpdatePreviewPosition(30_000);

        Assert.NotNull(timeline);
        Assert.Equal(30_000ul, timeline!.PositionMs);
        Assert.Equal(120_000ul, timeline.DurationMs);
    }

    [Fact]
    public void UpdatePreviewPosition_NullWhenNotPreviewing()
    {
        var state = new MediaControlState();

        Track(state);

        Assert.Null(state.UpdatePreviewPosition(30_000));
    }

    // ── Leaving preview ────────────────────────────────────────────────────────

    [Fact]
    public void Clear_RestoresLibraryUpdates()
    {
        var state = new MediaControlState();

        state.UpdateForPreview("C:\\clips\\preview-clip.flac", 120_000, isPlaying: true);
        state.Clear();

        Assert.False(state.IsShowingPreview);
        Assert.Equal(MediaControlPlaybackStatus.Playing, Track(state).Status);
    }

    [Fact]
    public void StopMidPreview_ClearsDisplayState()
    {
        var state = new MediaControlState();

        state.UpdateForPreview("C:\\clips\\preview-clip.flac", 120_000, isPlaying: true);
        state.Clear();

        // The tracked duration is gone, so a scrub can't be honored and the
        // preview timeline no longer has a duration to report.
        Assert.Null(state.SeekRatio(30_000));
        Assert.Null(state.UpdatePreviewPosition(30_000));
        Assert.False(state.IsShowingPreview);
    }
}

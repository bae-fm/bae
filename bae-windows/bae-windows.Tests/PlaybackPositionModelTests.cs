using System;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the pure position math behind the now-playing bar: seek projection
/// (ratio clamping, target rounding, duration cap), the projection-wins rule,
/// stale-track rejection, the time-label mode token, and the now-playing state
/// transitions. The elapsed / remaining clock labels are not here: core decides
/// their fields (bae-core's `util::duration`) and <see cref="Loc.Clock"/> renders
/// them (LocFormattersTests).
/// </summary>
public sealed class PlaybackPositionModelTests
{
    private static PlaybackPositionSnapshot Snapshot(
        ulong durationMs = 200_000,
        ulong positionMs = 50_000,
        double progress = 0.25) =>
        new(durationMs, positionMs, progress);

    // ── ProjectSeek: clamp into the slider range, round to the target ──

    [Fact]
    public void ProjectSeek_ClampsBelowMinimum()
    {
        var projection = PlaybackPositionModel.ProjectSeek("track-1", Snapshot(durationMs: 100_000), -0.5, 0.0, 1.0);
        Assert.Equal(0.0, projection.Progress);
        Assert.Equal(0UL, projection.TargetPositionMs);
    }

    [Fact]
    public void ProjectSeek_ClampsAboveMaximum()
    {
        var projection = PlaybackPositionModel.ProjectSeek("track-1", Snapshot(durationMs: 100_000), 1.5, 0.0, 1.0);
        Assert.Equal(1.0, projection.Progress);
        Assert.Equal(100_000UL, projection.TargetPositionMs);
    }

    [Fact]
    public void ProjectSeek_RoundsTargetPosition()
    {
        var projection = PlaybackPositionModel.ProjectSeek("track-1", Snapshot(durationMs: 90_001), 0.5, 0.0, 1.0);
        Assert.Equal((ulong)Math.Round(90_001 * 0.5), projection.TargetPositionMs);
    }

    [Fact]
    public void ProjectSeek_CapsTargetAtDuration()
    {
        var projection = PlaybackPositionModel.ProjectSeek("track-1", Snapshot(durationMs: 12_345), 1.0, 0.0, 1.0);
        Assert.Equal(12_345UL, projection.TargetPositionMs);
    }

    [Fact]
    public void ProjectSeek_MapsWithinNonUnitSliderRange()
    {
        var projection = PlaybackPositionModel.ProjectSeek("track-1", Snapshot(durationMs: 100_000), 5.0, 0.0, 10.0);
        Assert.Equal(50_000UL, projection.TargetPositionMs);
        Assert.Equal(5.0, projection.Progress);
    }

    [Fact]
    public void ProjectSeek_ZeroRangeThrows() =>
        Assert.Throws<InvalidOperationException>(() =>
            PlaybackPositionModel.ProjectSeek("track-1", Snapshot(), 0.5, 1.0, 1.0));

    [Fact]
    public void ProjectSeek_CarriesTrackIdAndDuration()
    {
        var projection = PlaybackPositionModel.ProjectSeek("track-9", Snapshot(durationMs: 42_000), 0.25, 0.0, 1.0);
        Assert.Equal("track-9", projection.TrackId);
        Assert.Equal(42_000UL, projection.DurationMs);
    }

    // ── ProjectionWins: a projection only wins for the track it was made for ──

    [Fact]
    public void ProjectionWins_ForSameTrack() =>
        Assert.True(PlaybackPositionModel.ProjectionWins(new SeekProjection("track-1", 1_000, 2_000, 0.5), "track-1"));

    [Fact]
    public void ProjectionWins_DroppedForDifferentTrack() =>
        Assert.False(PlaybackPositionModel.ProjectionWins(new SeekProjection("track-1", 1_000, 2_000, 0.5), "track-2"));

    [Fact]
    public void ProjectionWins_NoProjection() =>
        Assert.False(PlaybackPositionModel.ProjectionWins(null, "track-1"));

    // ── ClassifyPlaybackPosition: accept the current track, reject stale/absent ──

    [Fact]
    public void Classify_AcceptsMatchingTrack() =>
        Assert.Equal(PlaybackPositionRejection.Accepted, PlaybackPositionModel.ClassifyPlaybackPosition("track-1", "track-1"));

    [Fact]
    public void Classify_AcceptsWhenNoCurrentTrack() =>
        Assert.Equal(PlaybackPositionRejection.Accepted, PlaybackPositionModel.ClassifyPlaybackPosition(null, "track-1"));

    [Fact]
    public void Classify_RejectsMissingIncomingTrackId() =>
        Assert.Equal(PlaybackPositionRejection.MissingTrackId, PlaybackPositionModel.ClassifyPlaybackPosition("track-1", null));

    [Fact]
    public void Classify_RejectsStaleTrack() =>
        Assert.Equal(PlaybackPositionRejection.StaleTrack, PlaybackPositionModel.ClassifyPlaybackPosition("track-1", "track-2"));

    // ── TimeLabelToken / ShowRemainingFromToken: round-trip and fallback ──

    [Fact]
    public void TimeLabelToken_RoundTripsRemaining() =>
        Assert.True(PlaybackPositionModel.ShowRemainingFromToken(PlaybackPositionModel.TimeLabelToken(true)));

    [Fact]
    public void TimeLabelToken_RoundTripsElapsed() =>
        Assert.False(PlaybackPositionModel.ShowRemainingFromToken(PlaybackPositionModel.TimeLabelToken(false)));

    [Fact]
    public void ShowRemainingFromToken_RemainingIsTrue() =>
        Assert.True(PlaybackPositionModel.ShowRemainingFromToken("remaining"));

    [Fact]
    public void ShowRemainingFromToken_ElapsedIsFalse() =>
        Assert.False(PlaybackPositionModel.ShowRemainingFromToken("elapsed"));

    [Fact]
    public void ShowRemainingFromToken_NullIsFalse() =>
        Assert.False(PlaybackPositionModel.ShowRemainingFromToken(null));

    [Fact]
    public void ShowRemainingFromToken_EmptyIsFalse() =>
        Assert.False(PlaybackPositionModel.ShowRemainingFromToken(""));

    [Fact]
    public void ShowRemainingFromToken_GarbageIsFalse() =>
        Assert.False(PlaybackPositionModel.ShowRemainingFromToken("xyzzy"));

    [Fact]
    public void ShowRemainingFromToken_TrimsWhitespace() =>
        Assert.True(PlaybackPositionModel.ShowRemainingFromToken(" remaining\n"));

    // ── TimeLabelTooltipKey: names the mode a click switches to ──

    [Fact]
    public void TimeLabelTooltipKey_ElapsedOffersRemaining() =>
        Assert.Equal("nowplaying.show_remaining", PlaybackPositionModel.TimeLabelTooltipKey(false));

    [Fact]
    public void TimeLabelTooltipKey_RemainingOffersElapsed() =>
        Assert.Equal("nowplaying.show_elapsed", PlaybackPositionModel.TimeLabelTooltipKey(true));

    // ── ClearProjection: drop the projection, keep the snapshot ──

    [Fact]
    public void ClearProjection_PreservesSnapshot()
    {
        var snapshot = Snapshot();
        var state = new NowPlayingState(
            "album-1",
            "track-1",
            new PlaybackPositionState(snapshot, new SeekProjection("track-1", 1, 2, 0.5)));
        var cleared = PlaybackPositionModel.ClearProjection(state);
        Assert.NotNull(cleared);
        Assert.Null(cleared!.Position!.Projection);
        Assert.Equal(snapshot, cleared.Position.Snapshot);
    }

    [Fact]
    public void ClearProjection_NoPositionIsUnchanged()
    {
        var state = new NowPlayingState("album-1", "track-1", null);
        Assert.Same(state, PlaybackPositionModel.ClearProjection(state));
    }

    [Fact]
    public void ClearProjection_NullIsNull() =>
        Assert.Null(PlaybackPositionModel.ClearProjection(null));

    // ── BeginLoading: a new track resets position, the same track keeps it ──

    [Fact]
    public void BeginLoading_NewTrackResetsPosition()
    {
        var state = new NowPlayingState("album-1", "track-1", new PlaybackPositionState(Snapshot(), null));
        var loading = PlaybackPositionModel.BeginLoading(state, "track-2");
        Assert.NotNull(loading);
        Assert.Null(loading!.Position);
        Assert.Equal("track-1", loading.TrackId);
    }

    [Fact]
    public void BeginLoading_SameTrackKeepsPosition()
    {
        var position = new PlaybackPositionState(Snapshot(), null);
        var state = new NowPlayingState("album-1", "track-1", position);
        var loading = PlaybackPositionModel.BeginLoading(state, "track-1");
        Assert.Same(position, loading!.Position);
    }

    [Fact]
    public void BeginLoading_NoCurrentIsNull() =>
        Assert.Null(PlaybackPositionModel.BeginLoading(null, "track-1"));

    // ── WithPosition: create or fold in a snapshot ──

    [Fact]
    public void WithPosition_CreatesStateWhenNoneExists()
    {
        var snapshot = Snapshot();
        var state = PlaybackPositionModel.WithPosition(null, "track-1", snapshot, null);
        Assert.Null(state.AlbumId);
        Assert.Equal("track-1", state.TrackId);
        Assert.Equal(snapshot, state.Position!.Snapshot);
    }

    [Fact]
    public void WithPosition_PreservesAlbumWhenStateExists()
    {
        var existing = new NowPlayingState("album-1", "track-1", null);
        var snapshot = Snapshot();
        var state = PlaybackPositionModel.WithPosition(existing, "track-1", snapshot, null);
        Assert.Equal("album-1", state.AlbumId);
        Assert.Equal(snapshot, state.Position!.Snapshot);
    }
}

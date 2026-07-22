using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

// The library header's collapse-on-scroll math: progress tracks the active
// scrollable's offset over the first TrackDistance px, and a settle mid-scrub snaps
// to the nearer end. Only the most-recently-scrolled surface drives it.
public sealed class HeaderCollapseModelTests
{
    [Theory]
    [InlineData(0, 0)]
    [InlineData(16, 0.25)]
    [InlineData(32, 0.5)]
    [InlineData(64, 1)]
    public void ProgressTracksOffsetOverTheTrackDistance(double offset, double expected)
    {
        var model = new HeaderCollapseModel();
        Assert.Equal(expected, model.ReportScroll("albums", offset), 3);
        Assert.Equal(expected, model.Progress, 3);
    }

    [Fact]
    public void ProgressClampsBelowZeroAndAboveOne()
    {
        var model = new HeaderCollapseModel();
        Assert.Equal(0, model.ReportScroll("albums", -40));
        Assert.Equal(1, model.ReportScroll("albums", 400));
    }

    [Theory]
    [InlineData(0.3, 0)]
    [InlineData(0.49, 0)]
    [InlineData(0.5, 1)]
    [InlineData(0.8, 1)]
    public void SettlingMidScrubSnapsToTheNearerEnd(double startProgress, double expected)
    {
        var model = new HeaderCollapseModel();
        model.ReportScroll("albums", startProgress * HeaderCollapseModel.TrackDistance);

        Assert.True(model.ReportSettled("albums"));
        Assert.Equal(expected, model.Progress, 3);
    }

    [Fact]
    public void SettlingAtAnEndIsANoOp()
    {
        var model = new HeaderCollapseModel();
        model.ReportScroll("albums", 0);
        Assert.False(model.ReportSettled("albums"));

        model.ReportScroll("albums", HeaderCollapseModel.TrackDistance);
        Assert.False(model.ReportSettled("albums"));
    }

    [Fact]
    public void OnlyTheActiveScrollerCanSettleTheHeading()
    {
        var model = new HeaderCollapseModel();
        model.ReportScroll("albums", 0.3 * HeaderCollapseModel.TrackDistance);

        // A different surface settling does not snap the heading the active one is
        // scrubbing.
        Assert.False(model.ReportSettled("search"));
        Assert.Equal(0.3, model.Progress, 3);

        // The most recent scroll re-claims the active role.
        model.ReportScroll("search", 0.7 * HeaderCollapseModel.TrackDistance);
        Assert.True(model.ReportSettled("search"));
        Assert.Equal(1, model.Progress, 3);
    }
}

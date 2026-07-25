using System.Collections.Generic;
using System.Linq;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// The export queue's pure decision layer: the band/state catalog keys, the
// count summary, retry/pause gating, and percent clamping.
public sealed class OutputQueueModelTests
{
    [Fact]
    public void BandTitle_SplitsOnPaused()
    {
        Assert.Equal("output.title", OutputQueueModel.BandTitleKey(paused: false));
        Assert.Equal("download.paused", OutputQueueModel.BandTitleKey(paused: true));
    }

    [Fact]
    public void PauseToggle_SplitsOnPaused()
    {
        Assert.Equal("outbox.pause", OutputQueueModel.PauseToggleKey(paused: false));
        Assert.Equal("outbox.resume", OutputQueueModel.PauseToggleKey(paused: true));
    }

    [Fact]
    public void Retry_GatedOnFailures()
    {
        Assert.False(OutputQueueModel.RetryEnabled(0));
        Assert.True(OutputQueueModel.RetryEnabled(1));
    }

    [Theory]
    // Queued and failed reuse the generic download-state keys regardless of
    // output kind; only the active key differs by kind.
    [InlineData(OutputRowKind.Queued, OutputKind.Export, "download.state.queued")]
    [InlineData(OutputRowKind.Queued, OutputKind.Save, "download.state.queued")]
    [InlineData(OutputRowKind.Failed, OutputKind.Export, "download.state.failed")]
    [InlineData(OutputRowKind.Failed, OutputKind.Save, "download.state.failed")]
    [InlineData(OutputRowKind.Active, OutputKind.Export, "export.state.exporting")]
    [InlineData(OutputRowKind.Active, OutputKind.Save, "export.state.saving")]
    public void StateKey_MapsEachKind(OutputRowKind kind, OutputKind outputKind, string expected)
    {
        Assert.Equal(expected, OutputQueueModel.StateKey(kind, outputKind));
    }

    [Theory]
    [InlineData(-5, 0)]
    [InlineData(0, 0)]
    [InlineData(47, 47)]
    [InlineData(100, 100)]
    [InlineData(150, 100)]
    public void ClampPercent_ClampsToRange(int input, int expected)
    {
        Assert.Equal(expected, OutputQueueModel.ClampPercent(input));
    }
}

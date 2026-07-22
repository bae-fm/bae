using System.Collections.Generic;
using System.Linq;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

// The export queue's pure decision layer: the band/state catalog keys, the
// count summary, retry/pause gating, and percent clamping.
public sealed class ExportQueueModelTests
{
    [Fact]
    public void BandTitle_SplitsOnPaused()
    {
        Assert.Equal("export.title", ExportQueueModel.BandTitleKey(paused: false));
        Assert.Equal("download.paused", ExportQueueModel.BandTitleKey(paused: true));
    }

    [Fact]
    public void PauseToggle_SplitsOnPaused()
    {
        Assert.Equal("outbox.pause", ExportQueueModel.PauseToggleKey(paused: false));
        Assert.Equal("outbox.resume", ExportQueueModel.PauseToggleKey(paused: true));
    }

    [Fact]
    public void Retry_GatedOnFailures()
    {
        Assert.False(ExportQueueModel.RetryEnabled(0));
        Assert.True(ExportQueueModel.RetryEnabled(1));
    }

    [Theory]
    // Queued and failed reuse the generic download-state keys regardless of
    // output kind; only the active key differs by kind.
    [InlineData(ExportRowKind.Queued, OutputKind.Export, "download.state.queued")]
    [InlineData(ExportRowKind.Queued, OutputKind.Save, "download.state.queued")]
    [InlineData(ExportRowKind.Failed, OutputKind.Export, "download.state.failed")]
    [InlineData(ExportRowKind.Failed, OutputKind.Save, "download.state.failed")]
    [InlineData(ExportRowKind.Active, OutputKind.Export, "export.state.exporting")]
    [InlineData(ExportRowKind.Active, OutputKind.Save, "export.state.saving")]
    public void StateKey_MapsEachKind(ExportRowKind kind, OutputKind outputKind, string expected)
    {
        Assert.Equal(expected, ExportQueueModel.StateKey(kind, outputKind));
    }

    [Theory]
    [InlineData(-5, 0)]
    [InlineData(0, 0)]
    [InlineData(47, 47)]
    [InlineData(100, 100)]
    [InlineData(150, 100)]
    public void ClampPercent_ClampsToRange(int input, int expected)
    {
        Assert.Equal(expected, ExportQueueModel.ClampPercent(input));
    }
}

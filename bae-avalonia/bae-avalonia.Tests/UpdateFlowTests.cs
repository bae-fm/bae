using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the pure update-flow decisions in <see cref="UpdateFlowDisplay"/> and
/// <see cref="UpdateFlowState"/>: which status key each state renders, download
/// percent clamping, the check/restart button gating, and the version-string
/// normalization. The Velopack shell that produces these states is verified by
/// compilation, not here.
/// </summary>
public sealed class UpdateFlowTests
{
    [Fact]
    public void StatusFor_Idle_IsNull()
    {
        Assert.Null(UpdateFlowDisplay.StatusFor(new UpdateFlowState.Idle()));
    }

    [Fact]
    public void StatusFor_Checking_MapsKeyWithoutArgs()
    {
        var status = UpdateFlowDisplay.StatusFor(new UpdateFlowState.Checking());
        Assert.NotNull(status);
        Assert.Equal("settings.updates.checking", status!.Value.Key);
        Assert.Null(status.Value.Args);
    }

    [Fact]
    public void StatusFor_UpToDate_MapsKeyWithoutArgs()
    {
        var status = UpdateFlowDisplay.StatusFor(new UpdateFlowState.UpToDate());
        Assert.NotNull(status);
        Assert.Equal("settings.updates.up_to_date", status!.Value.Key);
        Assert.Null(status.Value.Args);
    }

    [Fact]
    public void StatusFor_Failed_MapsKeyWithoutArgs()
    {
        var status = UpdateFlowDisplay.StatusFor(new UpdateFlowState.Failed());
        Assert.NotNull(status);
        Assert.Equal("settings.updates.failed", status!.Value.Key);
        Assert.Null(status.Value.Args);
    }

    [Fact]
    public void StatusFor_Downloading_CarriesClampedPercent()
    {
        var status = UpdateFlowDisplay.StatusFor(new UpdateFlowState.Downloading(42));
        Assert.NotNull(status);
        Assert.Equal("settings.updates.downloading", status!.Value.Key);
        Assert.NotNull(status.Value.Args);
        Assert.Equal(42, status.Value.Args!["percent"]);
    }

    [Fact]
    public void StatusFor_Ready_CarriesNormalizedVersion()
    {
        var status = UpdateFlowDisplay.StatusFor(new UpdateFlowState.Ready("0.5.0"));
        Assert.NotNull(status);
        Assert.Equal("settings.updates.ready", status!.Value.Key);
        Assert.NotNull(status.Value.Args);
        Assert.Equal("0.5", status.Value.Args!["version"]);
    }

    [Theory]
    [InlineData(-1, 0)]
    [InlineData(0, 0)]
    [InlineData(37, 37)]
    [InlineData(100, 100)]
    [InlineData(250, 100)]
    public void Downloading_ClampsPercentToRange(int input, int expected)
    {
        Assert.Equal(expected, new UpdateFlowState.Downloading(input).Percent);
    }

    [Fact]
    public void CheckEnabled_TrueExceptWhileCheckingOrDownloading()
    {
        Assert.True(UpdateFlowDisplay.CheckEnabled(new UpdateFlowState.Idle()));
        Assert.True(UpdateFlowDisplay.CheckEnabled(new UpdateFlowState.UpToDate()));
        Assert.True(UpdateFlowDisplay.CheckEnabled(new UpdateFlowState.Ready("0.5")));
        Assert.True(UpdateFlowDisplay.CheckEnabled(new UpdateFlowState.Failed()));
        Assert.False(UpdateFlowDisplay.CheckEnabled(new UpdateFlowState.Checking()));
        Assert.False(UpdateFlowDisplay.CheckEnabled(new UpdateFlowState.Downloading(50)));
    }

    [Fact]
    public void RestartVisible_OnlyForReady()
    {
        Assert.True(UpdateFlowDisplay.RestartVisible(new UpdateFlowState.Ready("0.5")));
        Assert.False(UpdateFlowDisplay.RestartVisible(new UpdateFlowState.Idle()));
        Assert.False(UpdateFlowDisplay.RestartVisible(new UpdateFlowState.Checking()));
        Assert.False(UpdateFlowDisplay.RestartVisible(new UpdateFlowState.UpToDate()));
        Assert.False(UpdateFlowDisplay.RestartVisible(new UpdateFlowState.Downloading(50)));
        Assert.False(UpdateFlowDisplay.RestartVisible(new UpdateFlowState.Failed()));
    }

    [Theory]
    [InlineData("0.5.0", "0.5")]
    [InlineData("0.5.1", "0.5.1")]
    [InlineData("0.10.0", "0.10")]
    [InlineData("0.5", "0.5")]
    public void VersionDisplay_DropsTrailingZeroPatch(string semver, string expected)
    {
        Assert.Equal(expected, UpdateFlowDisplay.VersionDisplay(semver));
    }
}

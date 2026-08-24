using System.Linq;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// One broken watched folder raises one dialog, however many times the import
/// list re-delivers its summary.
/// </summary>
public sealed class ScanFailureAlertsTests
{
    private static BridgeWatchedFolderScanStatus Failed(string path, string error) =>
        new(path, "Rips", new BridgeFolderScanStatus.Failed(error), OnNetworkVolume: false);

    private static BridgeWatchedFolderScanStatus Complete(string path) =>
        new(path, "Rips", new BridgeFolderScanStatus.Complete(), OnNetworkVolume: false);

    /// The launch case: the startup scan fails before the UI is up, so the
    /// failure is already standing in the first summary the UI is given.
    [Fact]
    public void AFailureStandingInTheFirstDeliveryIsRaised()
    {
        var alerts = new ScanFailureAlerts();

        var raised = alerts.NewFailures(new[] { Failed("/Media", "no such column") });

        Assert.Equal(new[] { ("/Media", "no such column") }, raised.ToArray());
    }

    [Fact]
    public void TheSameFailureIsRaisedOnceAndADifferentOneAgain()
    {
        var alerts = new ScanFailureAlerts();

        alerts.NewFailures(new[] { Failed("/Media", "offline") });
        var repeat = alerts.NewFailures(new[] { Failed("/Media", "offline") });
        var alongside = alerts.NewFailures(
            new[] { Failed("/Media", "offline"), Complete("/Other") });
        var different = alerts.NewFailures(new[] { Failed("/Media", "no such column") });

        Assert.Empty(repeat);
        Assert.Empty(alongside);
        Assert.Equal(new[] { ("/Media", "no such column") }, different.ToArray());
    }

    [Fact]
    public void ARootThatRecoversRaisesItsNextBreakAgain()
    {
        var alerts = new ScanFailureAlerts();

        alerts.NewFailures(new[] { Failed("/Media", "offline") });
        alerts.NewFailures(new[] { Complete("/Media") });
        var again = alerts.NewFailures(new[] { Failed("/Media", "offline") });

        Assert.Equal(new[] { ("/Media", "offline") }, again.ToArray());
    }

    /// A root that stops being watched leaves with everything else; the alerts
    /// must not keep a row for a folder no delivery mentions.
    [Fact]
    public void ARootThatStopsBeingWatchedIsForgotten()
    {
        var alerts = new ScanFailureAlerts();

        alerts.NewFailures(new[] { Failed("/Media", "offline") });
        alerts.NewFailures(System.Array.Empty<BridgeWatchedFolderScanStatus>());
        var readded = alerts.NewFailures(new[] { Failed("/Media", "offline") });

        Assert.Equal(new[] { ("/Media", "offline") }, readded.ToArray());
    }

    [Fact]
    public void ClearForgetsWhatWasShown()
    {
        var alerts = new ScanFailureAlerts();

        alerts.NewFailures(new[] { Failed("/Media", "offline") });
        alerts.Clear();
        var afterClear = alerts.NewFailures(new[] { Failed("/Media", "offline") });

        Assert.Equal(new[] { ("/Media", "offline") }, afterClear.ToArray());
    }
}

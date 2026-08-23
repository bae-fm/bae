using System.Collections.Generic;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class UploadProgressPresentationTests
{
    [Fact]
    public void RunningQueueWithoutWorkHasNoPauseOrQueueSummary()
    {
        Assert.Equal(
            string.Empty,
            UploadProgressPresentation.QueueSummary(
                BridgeOutboxPauseState.Running,
                new List<BridgeCountLabel>()));
    }

    // The outbox is the authority on a release's cloud work: it holds the
    // release while there is any, and drops it when there is none.
    [Fact]
    public void AnImportedReleaseReadsItsCloudWorkOffTheOutbox()
    {
        var status = new BridgeTriageImportStatus.Complete("release-a", "album-a");
        var progress = Progress();

        var active = Assert.IsType<ImportUploadObservation.Active>(
            UploadProgressPresentation.ResolveImport(status, Snapshot(7, progress)));
        Assert.Same(progress, active.Progress);
        Assert.IsType<ImportUploadObservation.Finished>(
            UploadProgressPresentation.ResolveImport(status, Snapshot(7)));
    }

    // A running import has no cloud state to read yet, and neither has a
    // failed one.
    [Fact]
    public void AnUnfinishedImportHasNoCloudObservation()
    {
        Assert.Null(UploadProgressPresentation.ResolveImport(
            new BridgeTriageImportStatus.Importing(),
            Snapshot(7, Progress())));
        Assert.Null(UploadProgressPresentation.ResolveImport(
            null,
            Snapshot(7, Progress())));
    }

    // The bar and its label read the same two numbers off the same phase, so a
    // queue counting source bytes cannot be captioned with provider bytes.
    [Fact]
    public void TheBarLabelCountsTheBarsOwnPhase()
    {
        var preparing = new BridgeUploadBar(
            BridgeUploadPhase.Preparing,
            1_000,
            1_100);
        var uploading = new BridgeUploadBar(
            BridgeUploadPhase.Uploading,
            1_000,
            1_100);

        Assert.Equal(
            Loc.Core(
                BaeBridgeMethods.BridgeUploadPhaseBytesKey(
                    BridgeUploadPhase.Preparing),
                new Dictionary<string, object?>
                {
                    ["done"] = Loc.Bytes(1_000),
                    ["total"] = Loc.Bytes(1_100),
                }),
            UploadProgressPresentation.BarLabel(preparing));
        Assert.NotEqual(
            UploadProgressPresentation.BarLabel(preparing),
            UploadProgressPresentation.BarLabel(uploading));
        Assert.Equal(
            1_000d / 1_100d,
            UploadProgressPresentation.BarFraction(preparing));
    }

    // A release down to its make-Remote transition, or one being cancelled, has
    // no bytes to count: no bar, no byte caption.
    [Fact]
    public void ASliceWithoutBytesDrawsNoBar()
    {
        Assert.Equal(string.Empty, UploadProgressPresentation.BarLabel(null));
        Assert.Null(UploadProgressPresentation.BarFraction(null));
    }

    [Fact]
    public void FailedAttemptIsPresentedAsRetrying()
    {
        var progress = Progress(
            queued: 0,
            retrying: 1,
            activity: BridgeUploadActivity.Retrying);

        Assert.Equal(
            Loc.Core("core.outbox.retrying", "count", 1),
            UploadProgressPresentation.ActivityLabel(progress));
    }

    private static BridgeOutboxSnapshot Snapshot(
        ulong revision,
        BridgeUploadProgress? progress = null) =>
        new(
            revision,
            [],
            [],
            progress is null
                ? new Dictionary<string, BridgeUploadProgress>()
                : new Dictionary<string, BridgeUploadProgress>
                {
                    ["release-a"] = progress,
                },
            Progress(),
            0,
            [],
            BridgeOutboxPauseState.Running,
            0,
            null);

    private static BridgeUploadProgress Progress(
        uint queued = 1,
        uint retrying = 0,
        BridgeUploadActivity? activity = null,
        BridgeUploadBar? bar = null) =>
        new(
            queued,
            0,
            0,
            0,
            retrying,
            0,
            0,
            0,
            bar,
            activity ?? BridgeUploadActivity.Queued,
            true);
}

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

    [Fact]
    public void CloudImportWaitsForItsPublishedOutboxRevision()
    {
        var status = new BridgeCandidateImportStatus.CloudUploadQueued(
            "release-a",
            "album-a",
            7);

        Assert.IsType<ImportUploadObservation.Awaiting>(
            UploadProgressPresentation.ResolveImport(status, Snapshot(6)));
        Assert.IsType<ImportUploadObservation.Finished>(
            UploadProgressPresentation.ResolveImport(status, Snapshot(7)));
    }

    [Fact]
    public void ActiveOutboxProgressWinsForQueuedAndCompletedImports()
    {
        var progress = Progress();
        var snapshot = Snapshot(7, progress);

        var queued = Assert.IsType<ImportUploadObservation.Active>(
            UploadProgressPresentation.ResolveImport(
                new BridgeCandidateImportStatus.CloudUploadQueued(
                    "release-a",
                    "album-a",
                    7),
                snapshot));
        Assert.Same(progress, queued.Progress);

        Assert.IsType<ImportUploadObservation.Active>(
            UploadProgressPresentation.ResolveImport(
                new BridgeCandidateImportStatus.Complete(
                    "release-a",
                    "album-a"),
                snapshot));
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

    private static BridgeUploadProgress Progress() =>
        new(
            1,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            10,
            0,
            0,
            false,
            0,
            20,
            BridgeUploadActivity.Queued);
}

using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class StorageStoreTests
{
    private const string FailureLine = "Cloud operation failed";

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task MoveToCloudUsesTheRememberedPinChoice(bool rememberedPin)
    {
        bool? requestedPin = null;
        var downloads = new DownloadsService
        {
            MakeReleasesRemote = (_, pin) =>
            {
                requestedPin = pin;
                return Task.FromResult((
                    true,
                    (Outcome: (BridgeMakeReleasesRemoteOutcome?)Complete(2, "release-a"),
                        Error: (string?)null)));
            },
        };
        var store = Store(downloads, () => rememberedPin);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var error = await store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));

        Assert.Null(error);
        Assert.Equal(rememberedPin, requestedPin);
    }

    [Fact]
    public async Task MoveToCloudRemainsATransitionUntilTheOutboxSubscriptionArrives()
    {
        var downloads = new DownloadsService
        {
            MakeReleasesRemote = (_, _) =>
                Task.FromResult((true,
                    (Outcome: (BridgeMakeReleasesRemoteOutcome?)Complete(2, "release-a"),
                        Error: (string?)null))),
        };
        var store = Store(downloads, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var error = await store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));

        Assert.Null(error);
        var (transitioning, readError) = await store.TransitioningReleases(
            ["release-a"],
            new Dictionary<string, BridgeStorageRow>());
        Assert.Null(readError);
        Assert.Contains("release-a", transitioning);
        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-a"));
        Assert.False(store.CanCancelUpload("release-a"));
    }

    [Fact]
    public async Task ForegroundCommandIsVisibleBeforeItReturns()
    {
        var receipt = new TaskCompletionSource<(
            bool Current,
            (BridgeMakeReleasesRemoteOutcome? Outcome, string? Error) Result)>();
        var downloads = new DownloadsService
        {
            MakeReleasesRemote = (_, _) => receipt.Task,
        };
        var store = Store(downloads, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var command = store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));

        Assert.IsType<CloudUploadHandoff.Queueing>(
            store.UploadHandoff("release-a"));
        receipt.SetResult((
            true,
            (Outcome: Complete(2, "release-a"), Error: (string?)null)));
        Assert.Null(await command);
        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-a"));
    }

    [Fact]
    public async Task SelectionIsSubmittedOnceAndEveryHandoffMovesTogether()
    {
        var calls = new List<IReadOnlyList<string>>();
        var store = Store(new DownloadsService
        {
            MakeReleasesRemote = (releaseIds, _) =>
            {
                calls.Add(releaseIds.ToArray());
                return Task.FromResult((
                    true,
                    (Outcome: (BridgeMakeReleasesRemoteOutcome?)Complete(
                        2, "release-a", "release-b"),
                        Error: (string?)null)));
            },
        }, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var error = await store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a", "release-b"],
            () => Task.FromResult<string?>(null));

        Assert.Null(error);
        Assert.Single(calls);
        Assert.Equal(["release-a", "release-b"], calls[0]);
        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-a"));
        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-b"));
    }

    [Fact]
    public async Task PartialAdmissionHandsOffAcceptedReleasesAndReleasesRefusedOnes()
    {
        var failure = Failure("release-b");
        var store = Store(new DownloadsService
        {
            MakeReleasesRemote = (_, _) => Task.FromResult((
                true,
                (Outcome: (BridgeMakeReleasesRemoteOutcome?)new BridgeMakeReleasesRemoteOutcome.Partial(
                    new BridgeMakeRemoteReceipt(2, ["release-a"]),
                    failure),
                    Error: (string?)null))),
        }, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var error = await store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a", "release-b"],
            () => Task.FromResult<string?>(null));

        Assert.Equal(FailureLine, error);
        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-a"));
        Assert.Null(store.UploadHandoff("release-b"));
    }

    [Fact]
    public async Task ActiveTargetRefusesEveryForegroundHandoff()
    {
        var failure = Failure("release-a", "release-b");
        var store = Store(new DownloadsService
        {
            MakeReleasesRemote = (_, _) => Task.FromResult((
                true,
                (Outcome: (BridgeMakeReleasesRemoteOutcome?)new BridgeMakeReleasesRemoteOutcome.Partial(
                    null,
                    failure),
                    Error: (string?)null))),
        }, () => true);
        store.ApplyOutbox(OutboxWithRelease(revision: 1, releaseId: "release-b"));

        var error = await store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a", "release-b"],
            () => Task.FromResult<string?>(null));

        Assert.Equal(FailureLine, error);
        Assert.Null(store.UploadHandoff("release-a"));
        Assert.Null(store.UploadHandoff("release-b"));
        var (transitioning, readError) = await store.TransitioningReleases(
            ["release-a", "release-b"],
            new Dictionary<string, BridgeStorageRow>());
        Assert.Null(readError);
        Assert.DoesNotContain("release-a", transitioning);
        Assert.Contains("release-b", transitioning);
    }

    [Fact]
    public async Task OverlappingLoserCannotClearWinningCommandHandoff()
    {
        var firstReceipt = new TaskCompletionSource<(
            bool Current,
            (BridgeMakeReleasesRemoteOutcome? Outcome, string? Error) Result)>();
        var secondReceipt = new TaskCompletionSource<(
            bool Current,
            (BridgeMakeReleasesRemoteOutcome? Outcome, string? Error) Result)>();
        var invocation = 0;
        var store = Store(new DownloadsService
        {
            MakeReleasesRemote = (_, _) =>
                ++invocation == 1 ? firstReceipt.Task : secondReceipt.Task,
        }, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var first = store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));
        var second = store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));
        var failure = Failure("release-a");
        firstReceipt.SetResult((true,
            (Outcome: new BridgeMakeReleasesRemoteOutcome.Partial(null, failure),
                Error: (string?)null)));
        Assert.Equal(FailureLine, await first);

        Assert.IsType<CloudUploadHandoff.Queueing>(
            store.UploadHandoff("release-a"));
        secondReceipt.SetResult((
            true,
            (Outcome: Complete(2, "release-a"), Error: (string?)null)));
        Assert.Null(await second);
        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-a"));
    }

    [Fact]
    public async Task RetainedOutboxTakesOwnershipOfTheHandoff()
    {
        var store = StoreReturningRevision(2);
        await MoveToCloud(store);

        store.ApplyOutbox(OutboxWithRelease(revision: 2));

        Assert.Null(store.UploadHandoff("release-a"));
        Assert.True(store.CanCancelUpload("release-a"));
        var (transitioning, error) = await store.TransitioningReleases(
            ["release-a"],
            new Dictionary<string, BridgeStorageRow>());
        Assert.Null(error);
        Assert.Contains("release-a", transitioning);
    }

    [Fact]
    public async Task TerminalRevisionProvesFastCompletion()
    {
        var store = StoreReturningRevision(2);
        await MoveToCloud(store);

        store.ApplyOutbox(EmptyOutbox(revision: 2));

        Assert.Null(store.UploadHandoff("release-a"));
        var (transitioning, error) = await store.TransitioningReleases(
            ["release-a"],
            new Dictionary<string, BridgeStorageRow>());
        Assert.Null(error);
        Assert.DoesNotContain("release-a", transitioning);
    }

    [Fact]
    public async Task OlderSnapshotCannotClearTheHandoff()
    {
        var store = StoreReturningRevision(3);
        await MoveToCloud(store);

        store.ApplyOutbox(EmptyOutbox(revision: 2));

        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-a"));
    }

    [Fact]
    public async Task FailedCommandReturnsTheReleaseToRest()
    {
        var failure = Failure("release-a");
        var downloads = new DownloadsService
        {
            MakeReleasesRemote = (_, _) => Task.FromResult((
                true,
                (Outcome: (BridgeMakeReleasesRemoteOutcome?)new BridgeMakeReleasesRemoteOutcome.Partial(
                    null,
                    failure),
                    Error: (string?)null))),
        };
        var store = Store(downloads, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var error = await MoveToCloud(store);

        Assert.Equal(FailureLine, error);
        Assert.Null(store.UploadHandoff("release-a"));
    }

    [Fact]
    public async Task OutboxOwnershipCanArriveBeforeTheCommandFinishes()
    {
        var receipt = new TaskCompletionSource<(
            bool Current,
            (BridgeMakeReleasesRemoteOutcome? Outcome, string? Error) Result)>();
        var store = Store(new DownloadsService
        {
            MakeReleasesRemote = (_, _) => receipt.Task,
        }, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var command = MoveToCloud(store);
        store.ApplyOutbox(OutboxWithRelease(revision: 2));
        receipt.SetResult((false, (Outcome: null, Error: null)));

        Assert.Null(await command);
        Assert.Null(store.UploadHandoff("release-a"));
        Assert.True(store.CanCancelUpload("release-a"));
    }

    [Fact]
    public void PublishingUploadCannotBeCancelled()
    {
        var store = StoreReturningRevision(2);
        store.ApplyOutbox(OutboxWithRelease(
            revision: 2,
            activity: BridgeUploadActivity.Publishing,
            canCancel: false));

        Assert.False(store.CanCancelUpload("release-a"));
    }

    private static StorageStore StoreReturningRevision(ulong revision)
    {
        var store = Store(new DownloadsService
        {
            MakeReleasesRemote = (_, _) => Task.FromResult((
                true,
                (Outcome: (BridgeMakeReleasesRemoteOutcome?)Complete(
                    revision, "release-a"),
                    Error: (string?)null))),
        }, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));
        return store;
    }

    private static StorageStore Store(
        DownloadsService downloads,
        Func<bool> loadPinPreference) =>
        new(downloads, loadPinPreference, _ => FailureLine);

    private static Task<string?> MoveToCloud(StorageStore store) =>
        store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));

    private static BridgeMakeReleasesRemoteOutcome Complete(
        ulong revision,
        params string[] releaseIds) =>
        new BridgeMakeReleasesRemoteOutcome.Complete(
            new BridgeMakeRemoteReceipt(revision, releaseIds));

    private static BridgeMakeRemoteBatchFailure Failure(params string[] releaseIds) =>
        new(
            releaseIds,
            new BridgeException.Diagnostic(
                new BridgeErrorCategory.Network(),
                "provider refused"));

    private static BridgeOutboxSnapshot OutboxWithRelease(
        ulong revision,
        string releaseId = "release-a",
        BridgeUploadActivity activity = BridgeUploadActivity.Queued,
        bool canCancel = true)
    {
        var queued = activity == BridgeUploadActivity.Queued ? 1U : 0U;
        var publishing = activity == BridgeUploadActivity.Publishing ? 1U : 0U;
        var progress = new BridgeUploadProgress(
            queued, 0, 0, 0, 0, 0, publishing, 0,
            new BridgeUploadBar(BridgeUploadPhase.Preparing, 0, 20),
            activity,
            canCancel,
            null);
        return new BridgeOutboxSnapshot(
            revision,
            [],
            [],
            new Dictionary<string, BridgeReleaseUploadProgress>
            {
                [releaseId] = new BridgeReleaseUploadProgress(progress, 0),
            },
            progress,
            0,
            [],
            BridgeOutboxPauseState.Running,
            0,
            null);
    }

    private static BridgeOutboxSnapshot EmptyOutbox(ulong revision) => new(
        revision,
        [],
        [],
        new Dictionary<string, BridgeReleaseUploadProgress>(),
        new BridgeUploadProgress(
            0, 0, 0, 0, 0, 0, 0, 0, null, null, false, null),
        0,
        [],
        BridgeOutboxPauseState.Running,
        0,
        null);
}

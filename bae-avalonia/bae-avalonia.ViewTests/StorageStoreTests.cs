using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class StorageStoreTests
{
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
                    (Revision: (ulong?)2, Error: (string?)null)));
            },
        };
        var store = new StorageStore(downloads, () => rememberedPin);
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
                Task.FromResult((true, (Revision: (ulong?)2, Error: (string?)null))),
        };
        var store = new StorageStore(downloads, () => true);
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
            (ulong? Revision, string? Error) Result)>();
        var downloads = new DownloadsService
        {
            MakeReleasesRemote = (_, _) => receipt.Task,
        };
        var store = new StorageStore(downloads, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var command = store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));

        Assert.IsType<CloudUploadHandoff.Queueing>(
            store.UploadHandoff("release-a"));
        receipt.SetResult((
            true,
            (Revision: (ulong?)2, Error: (string?)null)));
        Assert.Null(await command);
        Assert.IsType<CloudUploadHandoff.Awaiting>(
            store.UploadHandoff("release-a"));
    }

    [Fact]
    public async Task SelectionIsSubmittedOnceAndEveryHandoffMovesTogether()
    {
        var calls = new List<IReadOnlyList<string>>();
        var store = new StorageStore(new DownloadsService
        {
            MakeReleasesRemote = (releaseIds, _) =>
            {
                calls.Add(releaseIds.ToArray());
                return Task.FromResult((
                    true,
                    (Revision: (ulong?)2, Error: (string?)null)));
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
    public async Task ActiveTargetRefusesEveryForegroundHandoff()
    {
        var store = new StorageStore(new DownloadsService
        {
            MakeReleasesRemote = (_, _) => Task.FromResult((
                true,
                (Revision: (ulong?)null, Error: (string?)"already active"))),
        }, () => true);
        store.ApplyOutbox(OutboxWithRelease(revision: 1, releaseId: "release-b"));

        var error = await store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a", "release-b"],
            () => Task.FromResult<string?>(null));

        Assert.Equal("already active", error);
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
            (ulong? Revision, string? Error) Result)>();
        var secondReceipt = new TaskCompletionSource<(
            bool Current,
            (ulong? Revision, string? Error) Result)>();
        var invocation = 0;
        var store = new StorageStore(new DownloadsService
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
        firstReceipt.SetResult((
            true,
            (Revision: (ulong?)null, Error: (string?)"already active")));
        Assert.Equal("already active", await first);

        Assert.IsType<CloudUploadHandoff.Queueing>(
            store.UploadHandoff("release-a"));
        secondReceipt.SetResult((
            true,
            (Revision: (ulong?)2, Error: (string?)null)));
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
        var downloads = new DownloadsService
        {
            MakeReleasesRemote = (_, _) => Task.FromResult((
                true,
                (Revision: (ulong?)null, Error: (string?)"provider refused"))),
        };
        var store = new StorageStore(downloads, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var error = await MoveToCloud(store);

        Assert.Equal("provider refused", error);
        Assert.Null(store.UploadHandoff("release-a"));
    }

    [Fact]
    public async Task OutboxOwnershipCanArriveBeforeTheCommandFinishes()
    {
        var receipt = new TaskCompletionSource<(
            bool Current,
            (ulong? Revision, string? Error) Result)>();
        var store = new StorageStore(new DownloadsService
        {
            MakeReleasesRemote = (_, _) => receipt.Task,
        }, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));

        var command = MoveToCloud(store);
        store.ApplyOutbox(OutboxWithRelease(revision: 2));
        receipt.SetResult((false, (Revision: null, Error: null)));

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
        var store = new StorageStore(new DownloadsService
        {
            MakeReleasesRemote = (_, _) => Task.FromResult((
                true,
                (Revision: (ulong?)revision, Error: (string?)null))),
        }, () => true);
        store.ApplyOutbox(EmptyOutbox(revision: 1));
        return store;
    }

    private static Task<string?> MoveToCloud(StorageStore store) =>
        store.RunStorageActionForReleases(
            BridgeReleaseStorageAction.MakeRemote,
            ["release-a"],
            () => Task.FromResult<string?>(null));

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

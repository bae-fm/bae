using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class StorageStoreTests
{
    [Fact]
    public async Task MoveToCloudRemainsATransitionUntilTheOutboxSubscriptionArrives()
    {
        var downloads = new DownloadsService
        {
            MakeReleaseRemote = (_, _) =>
                Task.FromResult((true, (Revision: (ulong?)2, Error: (string?)null))),
        };
        var store = new StorageStore(downloads);
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
            MakeReleaseRemote = (_, _) => receipt.Task,
        };
        var store = new StorageStore(downloads);
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
            MakeReleaseRemote = (_, _) => Task.FromResult((
                true,
                (Revision: (ulong?)null, Error: (string?)"provider refused"))),
        };
        var store = new StorageStore(downloads);
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
            MakeReleaseRemote = (_, _) => receipt.Task,
        });
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
            MakeReleaseRemote = (_, _) => Task.FromResult((
                true,
                (Revision: (ulong?)revision, Error: (string?)null))),
        });
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
        BridgeUploadActivity activity = BridgeUploadActivity.Queued,
        bool canCancel = true)
    {
        var queued = activity == BridgeUploadActivity.Queued ? 1U : 0U;
        var publishing = activity == BridgeUploadActivity.Publishing ? 1U : 0U;
        var progress = new BridgeUploadProgress(
            queued, 0, 0, 0, 0, 0, publishing, 0,
            0, 10, 0, 0, false, 0, 20,
            activity,
            canCancel);
        return new BridgeOutboxSnapshot(
            revision,
            [],
            [],
            new Dictionary<string, BridgeUploadProgress>
            {
                ["release-a"] = progress,
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
        new Dictionary<string, BridgeUploadProgress>(),
        new BridgeUploadProgress(
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, true, 0, 0, null, false),
        0,
        [],
        BridgeOutboxPauseState.Running,
        0,
        null);
}

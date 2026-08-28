using System;
using System.Collections.Generic;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class LiveQueryOwnershipTests
{
    private sealed class Subscription : IDisposable
    {
        public bool Disposed { get; private set; }

        public void Dispose() => Disposed = true;
    }

    [Fact]
    public void AlbumDetailStoreRejectsAnEvictedSubscriptionsQueuedValue()
    {
        var values = new Dictionary<string, Action<AlbumDetail?>>();
        var errors = new Dictionary<string, Action<Exception>>();
        var subscriptions = new Dictionary<string, Subscription>();
        var reportedErrors = 0;
        var library = new LibraryService
        {
            SubscribeAlbumDetail = (albumId, onValue, onError) =>
            {
                values[albumId] = onValue;
                errors[albumId] = onError;
                return subscriptions[albumId] = new Subscription();
            },
        };
        using var store = new AlbumDetailStore(
            library,
            apply => apply(),
            _ => reportedErrors += 1);

        store.Select("album-a");
        store.Select("album-b");
        values["album-a"](null);
        errors["album-a"](new InvalidOperationException("evicted"));

        Assert.True(subscriptions["album-a"].Disposed);
        Assert.Equal("album-b", store.AlbumId);
        Assert.False(store.HasValue);
        Assert.Equal(0, reportedErrors);

        values["album-b"](null);
        errors["album-b"](new InvalidOperationException("current"));
        Assert.True(store.HasValue);
        Assert.Equal(1, reportedErrors);
    }

    [Fact]
    public void ImportStoreRejectsAnEvictedReleaseStatusValue()
    {
        var values = new Dictionary<string, Action<BridgeLibraryStatus>>();
        var errors = new Dictionary<string, Action<Exception>>();
        var subscriptions = new Dictionary<string, Subscription>();
        var reportedErrors = 0;
        var import = new ImportService
        {
            SubscribeReleaseLibraryStatus = (_, releaseId, _, onValue, onError) =>
            {
                values[releaseId] = onValue;
                errors[releaseId] = onError;
                return subscriptions[releaseId] = new Subscription();
            },
        };
        using var store = new ImportStore(
            import,
            new SettingsStore(new SettingsService()),
            (_, _) => reportedErrors += 1,
            new NoopMediaControl(),
            apply => apply());

        store.ObserveReleaseLibraryStatus(
            BridgeMetadataSource.MusicBrainz,
            "release-a",
            null);
        store.ObserveReleaseLibraryStatus(
            BridgeMetadataSource.MusicBrainz,
            "release-b",
            null);
        values["release-a"](
            new BridgeLibraryStatus("release-a", true, true, "Album A", "album-a"));
        errors["release-a"](new InvalidOperationException("evicted"));

        Assert.True(subscriptions["release-a"].Disposed);
        Assert.Null(store.ReleaseLibraryStatus);
        Assert.Equal(0, reportedErrors);

        var current = new BridgeLibraryStatus(
            "release-b", true, false, "Album B", "album-b");
        values["release-b"](current);
        errors["release-b"](new InvalidOperationException("current"));
        Assert.Equal(current, store.ReleaseLibraryStatus);
        Assert.Equal(1, reportedErrors);
    }
}

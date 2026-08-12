using System;
using System.Collections.Generic;
using Avalonia.Threading;
using Avalonia.Headless.XUnit;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class QueuePageStoreTests
{
    [AvaloniaFact]
    public void VisibleWindowEvictsSubscriptionsAndIgnoresLateDelivery()
    {
        var source = new QueuePageSource();
        var store = new PlaybackStore(new QueueService
        {
            SubscribeUpcomingPage = source.Subscribe,
        }, _ => { });
        store.ApplyQueueValue(Snapshot(total: 500));

        store.ReportVisibleContextRange(100, 150);
        store.ReportVisibleContextRange(200, 250);
        store.ReportVisibleContextRange(300, 350);
        store.ReportVisibleContextRange(400, 450);

        Assert.True(source.MaximumActive <= 3);
        Assert.True(source.Subscriptions[100].IsDisposed);

        source.Subscriptions[100].Deliver(new BridgeQueueUpcomingPage(
            Revision: 1,
            Entries: new[] { Entry("evicted") }));
        Dispatcher.UIThread.RunJobs();

        Assert.Null(store.ContextItemAt(100));
    }

    private static BridgeQueueSnapshot Snapshot(ulong total) => new(
        Manual: Array.Empty<BridgeQueueEntry>(),
        Context: new BridgePlaybackContext(
            Kind: BridgePlaybackSourceKind.Library,
            SourceTitle: null,
            Shuffled: false,
            Upcoming: new[] { Entry("initial") },
            UpcomingTotal: total),
        HasNext: false,
        HasPrevious: false,
        Revision: 1);

    private static BridgeQueueEntry Entry(string id) => new(
        EntryId: id,
        TrackId: $"track-{id}",
        Title: $"Title {id}",
        ArtistNames: "Artist Name",
        DurationClock: null,
        AlbumTitle: "Album Title",
        CoverImage: null);

    private sealed class QueuePageSource
    {
        public Dictionary<uint, Subscription> Subscriptions { get; } = new();
        public int MaximumActive { get; private set; }

        public IDisposable Subscribe(
            uint offset,
            uint _,
            Action<BridgeQueueUpcomingPage> onValue,
            Action<Exception> __)
        {
            var subscription = new Subscription(onValue);
            subscription.Disposed += () => RecordActive();
            Subscriptions[offset] = subscription;
            RecordActive();
            return subscription;
        }

        private void RecordActive()
        {
            var active = 0;
            foreach (var subscription in Subscriptions.Values)
            {
                if (!subscription.IsDisposed)
                {
                    active++;
                }
            }
            MaximumActive = Math.Max(MaximumActive, active);
        }
    }

    private sealed class Subscription(Action<BridgeQueueUpcomingPage> onValue) : IDisposable
    {
        public bool IsDisposed { get; private set; }
        public event Action? Disposed;

        public void Deliver(BridgeQueueUpcomingPage page) => onValue(page);

        public void Dispose()
        {
            IsDisposed = true;
            Disposed?.Invoke();
        }
    }
}

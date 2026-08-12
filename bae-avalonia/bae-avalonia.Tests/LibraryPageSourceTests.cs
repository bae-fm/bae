using System;
using System.Collections.Generic;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

public class LibraryPageSourceTests
{
    private sealed class Subscription : IDisposable
    {
        public bool Disposed { get; private set; }

        public void Dispose() => Disposed = true;
    }

    [Fact]
    public void SubscribeForwardsTheRequestedWindowAndValues()
    {
        (ulong Offset, ulong Limit) seen = default;
        var rows = new List<string> { "a", "b" };
        var source = new LibraryPageSource<string>((offset, limit, onValue, _) =>
        {
            seen = (offset, limit);
            onValue(rows, 42);
            return new Subscription();
        });
        IReadOnlyList<string>? deliveredRows = null;
        var deliveredCount = 0;

        source.Subscribe(30, 20, (value, count) =>
        {
            deliveredRows = value;
            deliveredCount = count;
        }, _ => { });

        Assert.Equal((30ul, 20ul), seen);
        Assert.Equal(rows, deliveredRows);
        Assert.Equal(42, deliveredCount);
    }

    [Fact]
    public void SubscribeForwardsErrors()
    {
        var expected = new PageLoadException("disk is full");
        var source = new LibraryPageSource<string>((_, _, _, onError) =>
        {
            onError(expected);
            return new Subscription();
        });
        Exception? delivered = null;

        source.Subscribe(0, 10, (_, _) => { }, error => delivered = error);

        Assert.Same(expected, delivered);
    }

    [Fact]
    public void MissingSubscriptionMeansTheSessionWasReplaced()
    {
        var source = new LibraryPageSource<string>((_, _, _, _) => null);

        Assert.Throws<OperationCanceledException>(
            () => source.Subscribe(0, 10, (_, _) => { }, _ => { }));
    }

    [Fact]
    public void DisposeStopsTheUnderlyingSubscription()
    {
        var expected = new Subscription();
        var source = new LibraryPageSource<string>((_, _, _, _) => expected);

        var subscription = source.Subscribe(0, 10, (_, _) => { }, _ => { });
        subscription.Dispose();

        Assert.True(expected.Disposed);
    }
}

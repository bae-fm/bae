using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// The page source maps the library reads' session-currency and error contract onto
// the contract the paging core relies on: a session-swap mid-fetch reads as a
// cancellation the list swallows, and a core error reads as a PageLoadException the
// failure sink renders — so a page load never silently drops rows or a failure.
public class LibraryPageSourceTests
{
    private static LibraryPageSource<string> Source(
        Func<(bool, long)> count,
        Func<ulong, ulong, (bool, IReadOnlyList<string>?, string?)> page) =>
        new(() => count(), (offset, limit) => page(offset, limit));

    [Fact]
    public async Task CountReturnsTheCount()
    {
        var source = Source(() => (true, 42), (_, _) => (true, new List<string>(), null));
        Assert.Equal(42, await source.CountAsync());
    }

    [Fact]
    public async Task CountOnAStaleSessionCancels()
    {
        var source = Source(() => (false, 0), (_, _) => (true, new List<string>(), null));
        await Assert.ThrowsAsync<OperationCanceledException>(() => source.CountAsync());
    }

    [Fact]
    public async Task PageReturnsTheRows()
    {
        var rows = new List<string> { "a", "b" };
        var source = Source(() => (true, 2), (_, _) => (true, rows, null));
        Assert.Equal(rows, await source.PageAsync(0, 10));
    }

    [Fact]
    public async Task PageOnAStaleSessionCancels()
    {
        var source = Source(() => (true, 2), (_, _) => (false, null, null));
        await Assert.ThrowsAsync<OperationCanceledException>(() => source.PageAsync(0, 10));
    }

    [Fact]
    public async Task PageWithAnErrorThrowsWithTheLine()
    {
        var source = Source(() => (true, 2), (_, _) => (true, null, "disk is full"));
        var exception = await Assert.ThrowsAsync<PageLoadException>(() => source.PageAsync(0, 10));
        Assert.Equal("disk is full", exception.Line);
    }

    [Fact]
    public async Task PageWithNullRowsAndNoErrorStillThrows()
    {
        var source = Source(() => (true, 2), (_, _) => (true, null, null));
        var exception = await Assert.ThrowsAsync<PageLoadException>(() => source.PageAsync(0, 10));
        Assert.Null(exception.Line);
    }

    [Fact]
    public async Task PageForwardsTheRequestedWindow()
    {
        (ulong Offset, ulong Limit) seen = default;
        var source = Source(() => (true, 100), (offset, limit) =>
        {
            seen = (offset, limit);
            return (true, new List<string>(), null);
        });
        await source.PageAsync(30, 20);
        Assert.Equal((30ul, 20ul), seen);
    }
}

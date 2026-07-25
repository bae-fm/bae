using System;
using System.Collections.Generic;
using System.Threading.Tasks;

namespace Bae.Desktop;

/// <summary>
/// A page load that failed with a core error line — the exception the page source
/// throws so <see cref="PaginatedList{TRow, TId}"/> routes it to its failure sink
/// (or, for the cold count, to <c>InitialLoadError</c>). Carries the localized line
/// core produced, or null for the generic fallback. A session-swap mid-fetch is a
/// separate case: the source throws <see cref="OperationCanceledException"/> for
/// that, which the list swallows.
/// </summary>
internal sealed class PageLoadException : Exception
{
    public PageLoadException(string? line)
        : base(line ?? "library page load failed")
    {
        Line = line;
    }

    /// <summary>The localized error line from core, or null for the generic
    /// fallback. The store's failure sink renders it into the shell banner.</summary>
    public string? Line { get; }
}

/// <summary>
/// An <see cref="IPageSource{TRow}"/> over two library-read delegates, with the
/// sort criteria baked into the instance at construction — a fresh instance per
/// sort so a sort change swaps the whole list (a new generation-0
/// <see cref="PaginatedList{TRow, TId}"/>), the way BaeKit's
/// <c>BrowseListSlot.reload</c> rebuilds its list from a fresh page source. The
/// synchronous library reads run off the UI thread; the session-currency flag maps
/// to a cancellation (drop quietly, the list is being replaced) and a core error
/// maps to <see cref="PageLoadException"/>.
/// </summary>
internal sealed class LibraryPageSource<TRow> : IPageSource<TRow>
{
    private readonly Func<(bool Current, long Count)> _count;
    private readonly Func<ulong, ulong, (bool Current, IReadOnlyList<TRow>? Rows, string? Error)> _page;

    public LibraryPageSource(
        Func<(bool Current, long Count)> count,
        Func<ulong, ulong, (bool Current, IReadOnlyList<TRow>? Rows, string? Error)> page)
    {
        _count = count;
        _page = page;
    }

    public async Task<int> CountAsync()
    {
        var (current, count) = await Task.Run(_count);
        if (!current)
        {
            throw new OperationCanceledException();
        }
        return checked((int)count);
    }

    public async Task<IReadOnlyList<TRow>> PageAsync(int offset, int limit)
    {
        var (current, rows, error) = await Task.Run(() => _page((ulong)offset, (ulong)limit));
        if (!current)
        {
            throw new OperationCanceledException();
        }
        if (error is not null || rows is null)
        {
            throw new PageLoadException(error);
        }
        return rows;
    }
}

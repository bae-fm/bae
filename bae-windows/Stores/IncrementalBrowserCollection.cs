using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI.Xaml.Data;
using Windows.Foundation;

namespace Bae.Windows;

// A collection that starts with an eagerly-fetched first page (so a
// synchronous load-result contract — status line, empty-state, enablement —
// keeps working) and pages further rows in via ISupportIncrementalLoading as
// the bound GridView/ListView scrolls toward the end. Shared by every
// library-scaled table this app pages: the album/composer/artist browser
// grids and the storage dialog's release table. Replaces a single fixed-size
// first page that either silently truncated the data past it, or fetched the
// whole (library-scaled) set at once.
internal sealed class IncrementalBrowserCollection<T> : ObservableCollection<T>, ISupportIncrementalLoading
{
    // Fetches [offset, offset + limit): Current is false when the session closed
    // mid-fetch (not a failure — just stop paging quietly), Error is set only for
    // a genuine fetch failure.
    private readonly Func<ulong, ulong, (bool Current, IReadOnlyList<T>? Items, string? Error)> _fetchPage;
    private readonly Action<string?> _onError;
    private ulong _totalCount;
    private ulong _loadedCount;
    private bool _failed;

    public IncrementalBrowserCollection(
        Func<ulong, ulong, (bool Current, IReadOnlyList<T>? Items, string? Error)> fetchPage,
        Action<string?> onError)
    {
        _fetchPage = fetchPage;
        _onError = onError;
    }

    // Called once the eager first page has been added to this collection: the
    // full row count and how many rows are already loaded, so HasMoreItems
    // reflects what's left to page in. Also the reset point for a sort/mode/tab
    // reload: the caller Clear()s and repopulates the first page before calling
    // this again, so a stale total/offset from the previous load never survives.
    public void SeedFirstPage(ulong totalCount, ulong loadedCount)
    {
        _totalCount = totalCount;
        _loadedCount = loadedCount;
        _failed = false;
    }

    public bool HasMoreItems => !_failed && _loadedCount < _totalCount;

    public IAsyncOperation<LoadMoreItemsResult> LoadMoreItemsAsync(uint count) =>
        LoadMoreItemsAsyncCore(count).AsAsyncOperation();

    private async Task<LoadMoreItemsResult> LoadMoreItemsAsyncCore(uint count)
    {
        var offset = _loadedCount;
        var (current, items, error) = await Task.Run(() => _fetchPage(offset, count));
        if (!current)
        {
            // The session closed mid-scroll: not a failure, just stop — the store
            // is about to be torn down or replaced.
            return new LoadMoreItemsResult { Count = 0 };
        }
        if (error is not null || items is null)
        {
            _failed = true;
            _onError(error);
            return new LoadMoreItemsResult { Count = 0 };
        }

        foreach (var item in items)
        {
            Add(item);
        }
        _loadedCount += (ulong)items.Count;
        return new LoadMoreItemsResult { Count = (uint)items.Count };
    }
}

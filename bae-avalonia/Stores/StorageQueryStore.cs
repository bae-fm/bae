using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal sealed class StorageQueryStore
{
    private const int PageSize = 100;
    private readonly LibraryService _library;
    private readonly Dispatcher _dispatcher;
    private readonly Dictionary<string, BridgeStorageRow> _rowsById = new();
    private int _generation;

    public StorageQueryStore(LibraryService library, Dispatcher dispatcher)
    {
        _library = library;
        _dispatcher = dispatcher;
        List = BuildList(StorageTab.All, StorageSortField.AlbumTitle, SortDirection.Ascending, 0);
    }

    public PaginatedList<BridgeStorageRow, string> List { get; private set; }
    public long? TotalSize { get; private set; }
    public string? ErrorLine { get; private set; }
    public event Action? Changed;

    public BridgeStorageRow? Row(string id) => _rowsById.GetValueOrDefault(id);

    public async Task Update(
        StorageTab tab,
        StorageSortField field,
        SortDirection direction)
    {
        _generation++;
        var generation = _generation;
        List.Cancel();
        _rowsById.Clear();
        TotalSize = null;
        ErrorLine = null;
        List = BuildList(tab, field, direction, generation);
        Changed?.Invoke();

        await List.LoadInitialAsync();
        if (generation != _generation)
        {
            return;
        }
        if (List.InitialLoadError is not null)
        {
            ErrorLine = Loc.Chrome("storage.load_failed");
            Changed?.Invoke();
            return;
        }
        await List.LoadRangeAsync(0, PageSize);
        if (generation == _generation)
        {
            Changed?.Invoke();
        }
    }

    public void Cancel()
    {
        _generation++;
        List.Cancel();
    }

    private PaginatedList<BridgeStorageRow, string> BuildList(
        StorageTab tab,
        StorageSortField field,
        SortDirection direction,
        int generation)
    {
        var source = new LibraryPageSource<BridgeStorageRow>(
            (offset, limit, onValue, onError) =>
                _library.SubscribeStorage(
                    tab,
                    field,
                    direction,
                    offset,
                    limit,
                    (rows, count, size) => _dispatcher.Post(() =>
                    {
                        if (generation != _generation)
                        {
                            return;
                        }
                        TotalSize = size;
                        onValue(rows, count);
                        Changed?.Invoke();
                    }),
                    error => _dispatcher.Post(() =>
                    {
                        if (generation == _generation)
                        {
                            onError(error);
                        }
                    })));
        return new PaginatedList<BridgeStorageRow, string>(
            source,
            row => row.Release.Id,
            rows =>
            {
                foreach (var row in rows)
                {
                    _rowsById[row.Release.Id] = row;
                }
            },
            OnError);
    }

    private void OnError(Exception exception)
    {
        if (exception is OperationCanceledException)
        {
            return;
        }
        ErrorLine = (exception as PageLoadException)?.Line ?? Loc.Chrome("storage.load_failed");
        Changed?.Invoke();
    }
}

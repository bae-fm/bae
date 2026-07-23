using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI.Xaml.Data;
using Windows.Foundation;

namespace Bae.Windows;

// One row of the album grid: up to ColumnCount album cards laid out left to
// right. A row is the unit the grid ListView virtualizes, so a full-width detail
// panel can be injected under the row that holds the expanded album (mirroring
// macOS's LazyVStack of rows, each followed by an expansion slot). Pure data —
// which row hosts the expansion, and the panel itself, live on the window, keyed
// off the selected album id.
internal sealed class AlbumGridRow
{
    public AlbumGridRow(IReadOnlyList<Album> cards)
    {
        Cards = cards;
    }

    public IReadOnlyList<Album> Cards { get; }
}

// The album grid's shared card metric: the content width every card is sized to
// for the current column fit (the cell width minus its trailing gutter, which
// each card carries as a right margin so a row fills exactly — the same split
// ItemsWrapGrid did with ItemWidth). One instance, referenced by every card's
// width binding, so a resize that keeps the column count updates one value
// instead of rebuilding rows.
internal sealed class AlbumGridLayout : INotifyPropertyChanged
{
    private double _cardWidth = AlbumGridColumns.TargetCellWidth;

    public event PropertyChangedEventHandler? PropertyChanged;

    public double CardWidth
    {
        get => _cardWidth;
        set
        {
            if (Math.Abs(_cardWidth - value) < 0.5)
            {
                return;
            }
            _cardWidth = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(CardWidth)));
        }
    }
}

// A row projection over the flat, incrementally-paged album collection: it groups
// the albums into rows of ColumnCount and stays in step as pages stream in
// (append) or the column count changes on resize / full-width toggle (regroup).
// The flat collection stays the source of truth and the pager; this delegates
// ISupportIncrementalLoading to it so scrolling toward the last row pages more
// albums in, which arrive as CollectionChanged.Add and extend the rows.
internal sealed class AlbumGridRows : ObservableCollection<AlbumGridRow>, ISupportIncrementalLoading
{
    private readonly IncrementalBrowserCollection<Album> _source;
    private int _columnCount = 1;
    // How many source albums have been placed into rows so far — the running
    // grouping cursor. Kept in step with _source.Count by the change handler.
    private int _placed;

    public AlbumGridRows(IncrementalBrowserCollection<Album> source)
    {
        _source = source;
        _source.CollectionChanged += OnSourceChanged;
    }

    // The number of cards per row. Setting it regroups the already-loaded albums
    // in place (a resize crossed a column-fit threshold or the full-width
    // preference flipped). A cell-width change that keeps the count never touches
    // this — that flows through AlbumGridLayout.CellWidth instead.
    public int ColumnCount
    {
        get => _columnCount;
        set
        {
            var next = Math.Max(1, value);
            if (next == _columnCount)
            {
                return;
            }
            _columnCount = next;
            Regroup();
        }
    }

    private void OnSourceChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        switch (e.Action)
        {
            // A reload (sort / mode / library change) clears the flat collection
            // before repopulating its first page; drop the rows and reset the
            // grouping cursor so a stale row never survives.
            case NotifyCollectionChangedAction.Reset:
                Clear();
                _placed = 0;
                return;
            case NotifyCollectionChangedAction.Add when e.NewItems is not null:
                foreach (Album album in e.NewItems)
                {
                    AppendAlbum(album);
                }
                return;
        }
    }

    // Place one album at the grouping cursor: start a new row on a column
    // boundary, else grow the last (partial) row by replacing it — a Replace
    // event the grid turns into a single container rebuild.
    private void AppendAlbum(Album album)
    {
        if (_placed % _columnCount == 0)
        {
            Add(new AlbumGridRow(new List<Album> { album }));
        }
        else
        {
            var cards = new List<Album>(this[Count - 1].Cards) { album };
            this[Count - 1] = new AlbumGridRow(cards);
        }
        _placed++;
    }

    private void Regroup()
    {
        Clear();
        _placed = 0;
        foreach (var album in _source)
        {
            AppendAlbum(album);
        }
    }

    public bool HasMoreItems => _source.HasMoreItems;

    public IAsyncOperation<LoadMoreItemsResult> LoadMoreItemsAsync(uint count) =>
        LoadMoreCore(count).AsAsyncOperation();

    // Delegate paging to the flat collection; its Add events (routed through
    // OnSourceChanged) extend the rows. Report how many rows grew — not how many
    // albums loaded — so the ListView's "did the viewport fill?" check reflects
    // the items it actually virtualizes.
    private async Task<LoadMoreItemsResult> LoadMoreCore(uint count)
    {
        var before = Count;
        await _source.LoadMoreItemsAsync(count);
        return new LoadMoreItemsResult { Count = (uint)Math.Max(0, Count - before) };
    }
}

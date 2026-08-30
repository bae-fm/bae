using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Templates;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The storage sheet's per-release table, on the incremental-loading core: it
// presents the paginated list's TotalCount as a fixed count of row slots so the
// VirtualizingStackPanel realizes only visible rows, resolves each realized row
// through list.IdAt then the side store, and triggers LoadRange for a batch window
// around itself — the same realize-triggers-load shape the album grid uses. Beyond
// that it carries the storage-table behaviors: tap-to-select with a highlight tint,
// a right-tap that targets the selection (or just the tapped row), and a live
// Storage cell each realized row re-renders as its transfer / outbox state changes.
// The presenter owns the selection set and the list (swapped on a tab / sort
// change); the table reads them through the callbacks.
internal sealed class StorageTableView : UserControl
{
    private readonly Func<PaginatedList<BridgeStorageRow, string>> _list;
    private readonly Func<string, BridgeStorageRow?> _lookup;
    private readonly HashSet<string> _selected;
    private readonly Func<BridgeStorageRow, string> _cellText;
    private readonly RowSlotFacade _facade;
    private readonly ItemsControl _rows;
    private readonly HashSet<StorageRowControl> _realized = new();

    private PaginatedList<BridgeStorageRow, string> _bound;

    // The right-tap menu builder, set by the presenter after construction — it
    // targets the selection, which the presenter owns, and reloads the rows, so it
    // closes over the presenter's state rather than the table's.
    public Func<Control, Task>? MenuCallback { get; set; }

    public StorageTableView(
        Func<PaginatedList<BridgeStorageRow, string>> list,
        Func<string, BridgeStorageRow?> lookup,
        HashSet<string> selected,
        Func<BridgeStorageRow, string> cellText)
    {
        _list = list;
        _lookup = lookup;
        _selected = selected;
        _cellText = cellText;
        _bound = list();

        _facade = new RowSlotFacade(() => _list().TotalCount);
        _rows = new ItemsControl
        {
            ItemsPanel = new FuncTemplate<Panel?>(() => new VirtualizingStackPanel()),
            ItemsSource = _facade,
            ItemTemplate = new FuncDataTemplate<RowSlot>((_, _) => new StorageRowControl(this)),
        };
        Content = new ScrollViewer
        {
            Content = _rows,
            MaxHeight = 320,
            MinWidth = 560,
            HorizontalScrollBarVisibility = Avalonia.Controls.Primitives.ScrollBarVisibility.Disabled,
        };
        Subscribe(_bound);
    }

    // Re-read the list after a tab / sort change swapped it for a fresh generation-0
    // instance, and re-realize the rows against the new one.
    public void Rebind()
    {
        _bound.PropertyChanged -= OnListChanged;
        _bound = _list();
        Subscribe(_bound);
        _facade.NotifyReset();
    }

    // Re-render every realized row's Storage cell from its latest transfer / outbox
    // state — bounded to realized rows, never the whole library.
    public void RefreshCells()
    {
        foreach (var row in _realized)
        {
            row.RefreshCell();
        }
    }

    // Recolor every realized row from the current selection.
    public void RefreshHighlights()
    {
        foreach (var row in _realized)
        {
            row.RefreshTint();
        }
    }

    // ── Row-facing surface ────────────────────────────────────────────────────
    internal PaginatedList<BridgeStorageRow, string> List => _list();

    internal BridgeStorageRow? Lookup(string id) => _lookup(id);

    internal string CellText(BridgeStorageRow row) => _cellText(row);

    internal bool IsSelected(string id) => _selected.Contains(id);

    internal void ToggleSelection(string id)
    {
        if (!_selected.Add(id))
        {
            _selected.Remove(id);
        }
    }

    // Right-tapping a row outside the selection targets just that row (and selects
    // it), matching the macOS menu.
    internal void SelectOnly(string id)
    {
        _selected.Clear();
        _selected.Add(id);
        RefreshHighlights();
    }

    internal Task ShowMenu(Control anchor) => MenuCallback?.Invoke(anchor) ?? Task.CompletedTask;

    internal void Track(StorageRowControl row) => _realized.Add(row);

    internal void Untrack(StorageRowControl row) => _realized.Remove(row);

    private void Subscribe(PaginatedList<BridgeStorageRow, string> list) => list.PropertyChanged += OnListChanged;

    private void OnListChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(PaginatedList<BridgeStorageRow, string>.TotalCount))
        {
            _facade.NotifyReset();
        }
    }
}

// One realized storage row: the six-column grid (album / artist / media / storage
// / files / size), tap-to-select, and a right-tap menu. Resolves its position
// through IdAt then the side store, triggers LoadRange for a batch window around
// itself, and re-renders when it lands or the subscribed page changes.
internal sealed class StorageRowControl : ContentControl
{
    private readonly StorageTableView _table;
    private readonly Border _rowBorder;

    private PaginatedList<BridgeStorageRow, string>? _subscribed;
    private CancellationTokenSource? _loadCts;
    private TextBlock? _storageCell;
    private string? _releaseId;
    private int _index = -1;

    public StorageRowControl(StorageTableView table)
    {
        _table = table;
        _rowBorder = new Border
        {
            Padding = new Thickness(6, 4),
            CornerRadius = new CornerRadius(4),
            Background = Brushes.Transparent,
        };
        Content = _rowBorder;
        DataContextChanged += (_, _) => OnRowChanged();
        _rowBorder.PointerReleased += OnPointerReleased;
    }

    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        _table.Track(this);
        OnRowChanged();
    }

    protected override void OnDetachedFromVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnDetachedFromVisualTree(e);
        _table.Untrack(this);
        if (_subscribed is not null)
        {
            _subscribed.PropertyChanged -= OnListChanged;
            _subscribed = null;
        }
        _loadCts?.Cancel();
    }

    private void OnRowChanged()
    {
        if (DataContext is not RowSlot slot)
        {
            return;
        }
        _index = slot.RowIndex;
        EnsureSubscription();
        Render();
        KickLoad();
    }

    private void EnsureSubscription()
    {
        var list = _table.List;
        if (ReferenceEquals(list, _subscribed))
        {
            return;
        }
        if (_subscribed is not null)
        {
            _subscribed.PropertyChanged -= OnListChanged;
        }
        _subscribed = list;
        list.PropertyChanged += OnListChanged;
    }

    private void OnListChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(PaginatedList<BridgeStorageRow, string>.Epoch))
        {
            KickLoad();
        }
    }

    private void KickLoad()
    {
        _loadCts?.Cancel();
        var cts = new CancellationTokenSource();
        _loadCts = cts;
        _ = LoadAndRender(cts.Token);
    }

    private async Task LoadAndRender(CancellationToken token)
    {
        var offset = Math.Max(0, _index - LibraryBrowserStore.Page / 2);
        await _table.List.LoadRangeAsync(offset, LibraryBrowserStore.Page);
        if (!token.IsCancellationRequested)
        {
            Render();
        }
    }

    private void Render()
    {
        var id = _table.List.IdAt(_index);
        var row = id is null ? null : _table.Lookup(id);
        if (row is null)
        {
            _releaseId = null;
            _storageCell = null;
            _rowBorder.Child = null;
            _rowBorder.Background = Brushes.Transparent;
            return;
        }

        _releaseId = row.Release.Id;
        var grid = StorageGrid.MakeRow();
        StorageGrid.AddCell(grid, 0, row.Album.Title);
        StorageGrid.AddCell(grid, 1, row.Album.ArtistNames);
        StorageGrid.AddCell(grid, 2, row.Release.Format ?? string.Empty);
        _storageCell = StorageGrid.AddCell(grid, 3, _table.CellText(row));
        StorageGrid.AddCell(grid, 4, Loc.Number(row.Release.FileCount), rightAligned: true);
        StorageGrid.AddCell(grid, 5, Loc.Bytes(row.Release.TotalSize), rightAligned: true);
        _rowBorder.Child = grid;
        RefreshTint();
    }

    // Re-render just the Storage cell from the row's latest transfer / outbox state.
    public void RefreshCell()
    {
        if (_releaseId is not { } id || _storageCell is null || _table.Lookup(id) is not { } row)
        {
            return;
        }
        _storageCell.Text = _table.CellText(row);
    }

    public void RefreshTint()
    {
        if (_releaseId is { } id && _table.IsSelected(id))
        {
            _rowBorder[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
        }
        else
        {
            _rowBorder.Background = Brushes.Transparent;
        }
    }

    private async void OnPointerReleased(object? sender, PointerReleasedEventArgs e)
    {
        if (_releaseId is not { } id)
        {
            return;
        }
        if (e.InitialPressMouseButton == MouseButton.Left)
        {
            _table.ToggleSelection(id);
            RefreshTint();
            return;
        }
        if (e.InitialPressMouseButton == MouseButton.Right)
        {
            if (!_table.IsSelected(id))
            {
                _table.SelectOnly(id);
            }
            await _table.ShowMenu(_rowBorder);
        }
    }
}

// The six-column storage grid shared by the sortable header and each row: album and
// artist stretch (2:2), media / storage auto-size, files and size auto-size and
// right-align in their cell.
internal static class StorageGrid
{
    internal static Grid MakeRow() => new()
    {
        ColumnSpacing = 8,
        ColumnDefinitions = new ColumnDefinitions("2*,2*,Auto,Auto,Auto,Auto"),
    };

    internal static TextBlock AddCell(Grid grid, int column, string text, bool rightAligned = false)
    {
        var block = new TextBlock
        {
            Text = text,
            VerticalAlignment = VerticalAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
            HorizontalAlignment = rightAligned ? HorizontalAlignment.Right : HorizontalAlignment.Left,
        };
        block[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        Grid.SetColumn(block, column);
        grid.Children.Add(block);
        return block;
    }
}

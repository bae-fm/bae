using System;
using System.Collections;
using System.Collections.Generic;
using System.Collections.Specialized;
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
using Avalonia.Threading;

namespace Bae.Desktop;

// The album grid: the library's album pane, rebuilt on the incremental-loading
// core. It presents the list's TotalCount as a fixed count of row slots so the
// VirtualizingStackPanel realizes only visible rows; a realized row resolves its
// column positions through list.IdAt (a placeholder for a not-yet-loaded slot) and
// triggers LoadRange for a batch window around itself. A realized row re-triggers
// when the list's load epoch changes (a subscription value bumps the generation) or when
// the list instance is swapped (a sort change), matching the BaeKit AlbumGridView.
// The one expanded album's inline detail panel is injected under its host row.
internal sealed class AlbumGridView : UserControl
{
    private readonly AppService _app;
    private readonly ReleaseActionDialogs _dialogs;
    private readonly AlbumGridContext _context;
    private readonly ItemsControl _rows;
    private readonly RowSlotFacade _facade;
    private readonly Control _emptyState;
    private readonly InitialLoadErrorView _errorState;
    private readonly Panel _root;

    private PaginatedList<Album, string> _list;
    private int _columns = 1;

    public AlbumGridView(AppService app, ReleaseActionDialogs dialogs)
    {
        _app = app;
        _dialogs = dialogs;
        _list = app.LibraryBrowserStore.Albums;

        _context = new AlbumGridContext(
            app.LibraryBrowserStore,
            () => _list,
            () => _columns,
            new AlbumGridLayout(),
            OnCardPressed,
            BuildExpansion);
        _context.StartCardDrag = StartCardDrag;

        _facade = new RowSlotFacade(() => _list.RowCount(_columns));
        _rows = new ItemsControl
        {
            ItemsPanel = new FuncTemplate<Panel?>(() => new VirtualizingStackPanel()),
            ItemsSource = _facade,
            ItemTemplate = new FuncDataTemplate<RowSlot>((_, _) => new AlbumRowControl(_context)),
            Margin = new Thickness(AlbumGridColumns.HorizontalInset, 0),
        };
        var scroller = new ScrollViewer
        {
            Content = _rows,
            HorizontalScrollBarVisibility = Avalonia.Controls.Primitives.ScrollBarVisibility.Disabled,
        };

        _emptyState = BuildEmptyState();
        _errorState = new InitialLoadErrorView(() => _list.RetryInitialLoadAsync());

        _root = new Panel();
        _root.Children.Add(scroller);
        _root.Children.Add(_emptyState);
        _root.Children.Add(_errorState);
        Content = _root;

        Focusable = true;
        KeyDown += OnKeyDown;
        BindList(_list);
        app.LibraryBrowserStore.ListSwapped += OnListSwapped;
        this.GetObservable(BoundsProperty).Subscribe(new AnonymousObserver<Rect>(_ => ApplyMetrics()));
        RenderState();
    }

    // ── List binding / swap ───────────────────────────────────────────────────
    private void BindList(PaginatedList<Album, string> list)
    {
        _list = list;
        list.PropertyChanged += OnListChanged;
    }

    private void OnListSwapped(BrowserMode mode)
    {
        if (mode != BrowserMode.Albums)
        {
            return;
        }
        _list.PropertyChanged -= OnListChanged;
        _app.LibraryBrowserStore.Albums.PropertyChanged += OnListChanged;
        _list = _app.LibraryBrowserStore.Albums;
        _context.SelectedAlbumId = null;
        _context.Selection.Clear();
        ApplyMetrics();
        _facade.NotifyReset();
        RenderState();
    }

    private void OnListChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(PaginatedList<Album, string>.TotalCount):
                _facade.NotifyReset();
                RenderState();
                break;
            case nameof(PaginatedList<Album, string>.InitialLoadError):
                RenderState();
                break;
        }
    }

    // ── Layout metrics ────────────────────────────────────────────────────────
    private void ApplyMetrics()
    {
        var width = Bounds.Width;
        if (width <= 0)
        {
            return;
        }
        var metrics = AlbumGridColumns.Compute(width);
        _context.Layout.CardWidth = metrics.CellWidth - AlbumGridColumns.Gutter;
        if (metrics.Columns != _columns)
        {
            _columns = metrics.Columns;
            _facade.NotifyReset();
        }
    }

    // ── Empty / error / rows state ────────────────────────────────────────────
    private void RenderState()
    {
        var failed = _list.InitialLoadError is not null;
        var empty = !failed && _list.TotalCount == 0;
        _errorState.IsVisible = failed;
        _emptyState.IsVisible = empty;
        _rows.IsVisible = !failed && !empty;
    }

    private Control BuildEmptyState()
    {
        var stack = new StackPanel
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 6,
        };
        var title = new TextBlock
        {
            Text = Loc.Chrome("library.empty"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontSize = 24,
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var guidance = new TextBlock
        {
            Text = Loc.Chrome("library.empty_guidance"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontSize = 14,
        };
        guidance[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        stack.Children.Add(title);
        stack.Children.Add(guidance);
        return stack;
    }

    // ── Card interaction ──────────────────────────────────────────────────────
    // Plain click toggles the album's inline expansion (clearing multi-selection);
    // Ctrl toggles the clicked album in the selection; Shift extends the range from
    // the anchor. Modifier clicks never open the expansion.
    private void OnCardPressed(Album album, KeyModifiers modifiers)
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }
        if (modifiers.HasFlag(KeyModifiers.Control) || modifiers.HasFlag(KeyModifiers.Meta))
        {
            _context.Selection.Toggle(album.Id);
            SyncSelectionTint();
            return;
        }
        if (modifiers.HasFlag(KeyModifiers.Shift))
        {
            _context.Selection.ExtendRange(album.Id, id => _list.PositionOf(id), _list.IdAt);
            SyncSelectionTint();
            return;
        }
        ToggleExpansion(album);
    }

    private void ToggleExpansion(Album album)
    {
        if (_context.SelectedAlbumId == album.Id)
        {
            Collapse();
            return;
        }
        SetExpanded(album.Id);
    }

    private void SetExpanded(string? albumId)
    {
        // Clear the previous ring and the multi-selection; light the new ring.
        if (_context.SelectedAlbumId is { } prior && _app.LibraryBrowserStore.AlbumById(prior) is { } priorAlbum)
        {
            priorAlbum.IsExpanded = false;
        }
        _context.SelectedAlbumId = albumId;
        _context.ExpansionEpoch++;
        if (albumId is not null && _app.LibraryBrowserStore.AlbumById(albumId) is { } album)
        {
            album.IsExpanded = true;
            _context.Selection.Clear();
            SyncSelectionTint();
        }
        _context.RaiseExpansionChanged();
    }

    private void Collapse() => SetExpanded(null);

    private void SyncSelectionTint()
    {
        foreach (var id in _list.AllLoadedIds)
        {
            if (_app.LibraryBrowserStore.AlbumById(id) is { } album)
            {
                album.IsSelected = _context.Selection.Contains(id);
            }
        }
    }

    private void OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape && !_context.Selection.IsEmpty)
        {
            _context.Selection.Clear();
            SyncSelectionTint();
            e.Handled = true;
            return;
        }
        if (e.Key == Key.A && (e.KeyModifiers.HasFlag(KeyModifiers.Control) || e.KeyModifiers.HasFlag(KeyModifiers.Meta)))
        {
            _context.Selection.SelectAll(_list.AllLoadedIds);
            SyncSelectionTint();
            e.Handled = true;
        }
    }

    private Task<Control?> BuildExpansion(Album album) =>
        AlbumExpansionView.BuildAsync(_app, _dialogs, album, Collapse);

    // Start a card drag carrying the ordered targets (the whole multi-selection in
    // visible order when the card is part of it, else just this card) as the
    // newline-joined album-id payload the queue drop decodes. Cancelled when no
    // library is open. Never mutates the selection.
    private async Task StartCardDrag(Album album, PointerEventArgs e)
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }
        var ids = _context.Selection.OrderedTargets(album.Id, id => _list.PositionOf(id));
        var data = new DataObject();
        data.Set(DataFormats.Text, QueueDragPayload.Encode(ids));
        await DragDrop.DoDragDrop(e, data, DragDropEffects.Copy);
    }

    // ── Reveal / reset ────────────────────────────────────────────────────────

    // Bring an album into view and expand it: switch nothing here (the browser view
    // owns the mode), page the album's row in, then expand. A bridge gap for the
    // album index (not present under the current sort) is surfaced quietly.
    public async Task RevealAlbum(string albumId)
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }
        var (current, result) = await _app.LibraryBrowserStore.AlbumIndex(albumId);
        if (!current || result.Error is not null || result.Index is not long index)
        {
            return;
        }
        // Page forward until the target position is loaded.
        while (_list.IdAt((int)index) is null)
        {
            var before = _list.AllLoadedIds.Count;
            await _list.LoadRangeAsync((int)index, LibraryBrowserStore.Page);
            if (_list.AllLoadedIds.Count == before)
            {
                break;
            }
        }
        if (_app.LibraryBrowserStore.AlbumById(_list.IdAt((int)index) ?? string.Empty) is { } album)
        {
            SetExpanded(album.Id);
        }
    }

    public void Reset()
    {
        _context.SelectedAlbumId = null;
        _context.Selection.Clear();
    }
}

// The shared per-grid state a realized row reads: the store (id → row), the current
// list (re-read so a swap is seen), the column count, the shared card width, the
// one expanded album and its epoch, and the card-press / expansion-build callbacks.
internal sealed class AlbumGridContext
{
    public AlbumGridContext(
        LibraryBrowserStore store,
        Func<PaginatedList<Album, string>> list,
        Func<int> columns,
        AlbumGridLayout layout,
        Action<Album, KeyModifiers> onCardPressed,
        Func<Album, Task<Control?>> buildExpansion)
    {
        Store = store;
        ListFn = list;
        ColumnsFn = columns;
        Layout = layout;
        OnCardPressed = onCardPressed;
        BuildExpansion = buildExpansion;
    }

    public LibraryBrowserStore Store { get; }
    private Func<PaginatedList<Album, string>> ListFn { get; }
    private Func<int> ColumnsFn { get; }
    public AlbumGridLayout Layout { get; }
    public AlbumGridSelectionModel Selection { get; } = new();
    public Action<Album, KeyModifiers> OnCardPressed { get; }
    public Func<Album, Task<Control?>> BuildExpansion { get; }

    public PaginatedList<Album, string> List => ListFn();
    public int Columns => ColumnsFn();

    public string? SelectedAlbumId { get; set; }
    public int ExpansionEpoch { get; set; }

    // Starts an album-card drag carrying the card's ordered targets (set by the
    // grid, which owns the DragDrop session and the session guard).
    public Func<Album, PointerEventArgs, Task>? StartCardDrag { get; set; }

    public event Action? ExpansionChanged;

    public void RaiseExpansionChanged() => ExpansionChanged?.Invoke();
}

// The shared card metric the whole grid binds its card width to, so a resize that
// keeps the column count re-sizes cards without rebuilding rows.
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

// One row slot addressed by its grid-row index. The facade hands out these as the
// fixed-count item list; the realized row reads its index and resolves its cards.
internal sealed record RowSlot(int RowIndex);

// A fixed-count, read-only IList of row slots whose Count is the list's row count.
// On a count / column change the grid raises a collection Reset, so the
// VirtualizingStackPanel re-reads Count and re-realizes visible rows without losing
// the whole binding. Avalonia's ItemsControl requires an IList when the source
// raises collection changes, so this presents the read side of one; every mutator
// throws (the slots are computed from Count, never stored).
internal sealed class RowSlotFacade : IReadOnlyList<RowSlot>, IList, INotifyCollectionChanged
{
    private readonly Func<int> _count;

    public RowSlotFacade(Func<int> count) => _count = count;

    public event NotifyCollectionChangedEventHandler? CollectionChanged;

    public int Count => _count();

    public RowSlot this[int index] => new(index);

    object? IList.this[int index]
    {
        get => new RowSlot(index);
        set => throw new NotSupportedException();
    }

    public bool IsFixedSize => false;

    public bool IsReadOnly => true;

    public bool IsSynchronized => false;

    public object SyncRoot => this;

    public void NotifyReset() =>
        CollectionChanged?.Invoke(this, new NotifyCollectionChangedEventArgs(NotifyCollectionChangedAction.Reset));

    public IEnumerator<RowSlot> GetEnumerator()
    {
        var count = _count();
        for (var i = 0; i < count; i++)
        {
            yield return new RowSlot(i);
        }
    }

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

    bool IList.Contains(object? value) => value is RowSlot slot && slot.RowIndex >= 0 && slot.RowIndex < _count();

    int IList.IndexOf(object? value) =>
        value is RowSlot slot && slot.RowIndex >= 0 && slot.RowIndex < _count() ? slot.RowIndex : -1;

    void ICollection.CopyTo(Array array, int index)
    {
        var count = _count();
        for (var i = 0; i < count; i++)
        {
            array.SetValue(new RowSlot(i), index + i);
        }
    }

    int IList.Add(object? value) => throw new NotSupportedException();

    void IList.Clear() => throw new NotSupportedException();

    void IList.Insert(int index, object? value) => throw new NotSupportedException();

    void IList.Remove(object? value) => throw new NotSupportedException();

    void IList.RemoveAt(int index) => throw new NotSupportedException();
}

// One realized grid row: up to Columns album cards, plus — when this row holds the
// expanded album — the inline detail panel beneath them. Reads its row index from
// its DataContext (a RowSlot) so a recycled container re-renders for the new row.
// Triggers LoadRange for a batch window around itself and re-renders when it lands;
// re-triggers when the list's generation changes.
internal sealed class AlbumRowControl : ContentControl
{
    private readonly AlbumGridContext _context;
    private readonly StackPanel _cards;
    private readonly Panel _expansionHost;

    private PaginatedList<Album, string>? _subscribedList;
    private CancellationTokenSource? _loadCts;
    private int _rowIndex = -1;
    private int _builtExpansionEpoch = -1;

    public AlbumRowControl(AlbumGridContext context)
    {
        _context = context;
        _cards = new StackPanel { Orientation = Orientation.Horizontal };
        _expansionHost = new Panel();
        Content = new StackPanel { Children = { _cards, _expansionHost } };
        DataContextChanged += (_, _) => OnRowChanged();
    }

    protected override void OnAttachedToVisualTree(Avalonia.VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        _context.ExpansionChanged += UpdateExpansion;
        OnRowChanged();
    }

    protected override void OnDetachedFromVisualTree(Avalonia.VisualTreeAttachmentEventArgs e)
    {
        base.OnDetachedFromVisualTree(e);
        _context.ExpansionChanged -= UpdateExpansion;
        if (_subscribedList is not null)
        {
            _subscribedList.PropertyChanged -= OnListChanged;
            _subscribedList = null;
        }
        _loadCts?.Cancel();
    }

    private void OnRowChanged()
    {
        if (DataContext is not RowSlot slot)
        {
            return;
        }
        _rowIndex = slot.RowIndex;
        EnsureListSubscription();
        RenderCards();
        KickLoad();
    }

    private void EnsureListSubscription()
    {
        var list = _context.List;
        if (ReferenceEquals(list, _subscribedList))
        {
            return;
        }
        if (_subscribedList is not null)
        {
            _subscribedList.PropertyChanged -= OnListChanged;
        }
        _subscribedList = list;
        list.PropertyChanged += OnListChanged;
    }

    private void OnListChanged(object? sender, PropertyChangedEventArgs e)
    {
        // A new page value restarts this row's load because its position may
        // now resolve to a different album.
        if (e.PropertyName == nameof(PaginatedList<Album, string>.Epoch))
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
        var list = _context.List;
        var columns = _context.Columns;
        var center = _rowIndex * columns;
        var offset = Math.Max(0, center - LibraryBrowserStore.Page / 2);
        await list.LoadRangeAsync(offset, LibraryBrowserStore.Page);
        if (!token.IsCancellationRequested)
        {
            RenderCards();
        }
    }

    private void RenderCards()
    {
        _cards.Children.Clear();
        var list = _context.List;
        var columns = _context.Columns;
        var total = list.TotalCount;
        for (var col = 0; col < columns; col++)
        {
            var position = _rowIndex * columns + col;
            if (position >= total)
            {
                break;
            }
            var id = list.IdAt(position);
            var album = id is null ? null : _context.Store.AlbumById(id);
            _cards.Children.Add(album is null ? BuildPlaceholder() : BuildCard(album));
        }
        UpdateExpansion();
    }

    private Control BuildCard(Album album)
    {
        var card = AlbumCardVisual.Build(album, _context.Layout);
        // Click vs. drag: a press that releases in place is a click (expand /
        // select); a press that moves past a threshold starts a card drag. The
        // click fires on release so a drag doesn't also expand the album.
        Point? pressAt = null;
        var dragged = false;
        card.PointerPressed += (_, e) =>
        {
            if (e.GetCurrentPoint(card).Properties.IsLeftButtonPressed)
            {
                pressAt = e.GetPosition(card);
                dragged = false;
            }
        };
        card.PointerMoved += async (_, e) =>
        {
            if (pressAt is not { } start || dragged || !e.GetCurrentPoint(card).Properties.IsLeftButtonPressed)
            {
                return;
            }
            var now = e.GetPosition(card);
            if (Math.Abs(now.X - start.X) < 6 && Math.Abs(now.Y - start.Y) < 6)
            {
                return;
            }
            dragged = true;
            if (_context.StartCardDrag is { } start2)
            {
                await start2(album, e);
            }
        };
        card.PointerReleased += (_, e) =>
        {
            if (pressAt is not null && !dragged && e.InitialPressMouseButton == MouseButton.Left)
            {
                _context.OnCardPressed(album, e.KeyModifiers);
            }
            pressAt = null;
        };
        return card;
    }

    private Control BuildPlaceholder()
    {
        var art = new Border { CornerRadius = new CornerRadius(12) };
        art[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        return new Panel
        {
            Margin = new Thickness(0, 0, AlbumGridColumns.Gutter, AlbumGridColumns.RowGap),
            Children = { new StackPanel { Margin = new Thickness(6), Children = { new SquareBox { Child = art } } } },
        }.WithWidthBinding(_context.Layout);
    }

    private void UpdateExpansion()
    {
        var expandedId = _context.SelectedAlbumId;
        var hosts = expandedId is not null && RowContainsId(expandedId);
        if (!hosts)
        {
            _expansionHost.Children.Clear();
            _builtExpansionEpoch = -1;
            return;
        }
        if (_builtExpansionEpoch == _context.ExpansionEpoch && _expansionHost.Children.Count > 0)
        {
            return;
        }
        _builtExpansionEpoch = _context.ExpansionEpoch;
        _expansionHost.Children.Clear();
        _expansionHost.Children.Add(BuildLoading());
        var epoch = _context.ExpansionEpoch;
        var album = _context.Store.AlbumById(expandedId!)!;
        _ = BuildExpansionAsync(album, epoch);
    }

    private async Task BuildExpansionAsync(Album album, int epoch)
    {
        var content = await _context.BuildExpansion(album);
        if (epoch != _context.ExpansionEpoch || !RowContainsId(album.Id))
        {
            return;
        }
        _expansionHost.Children.Clear();
        if (content is not null)
        {
            _expansionHost.Children.Add(content);
        }
    }

    private bool RowContainsId(string id)
    {
        var list = _context.List;
        var columns = _context.Columns;
        for (var col = 0; col < columns; col++)
        {
            if (list.IdAt(_rowIndex * columns + col) == id)
            {
                return true;
            }
        }
        return false;
    }

    private static Control BuildLoading() => new ProgressBar
    {
        IsIndeterminate = true,
        Width = 120,
        Margin = new Thickness(0, 24),
        HorizontalAlignment = HorizontalAlignment.Center,
    };
}

// A tiny IObserver over an Action, for GetObservable subscriptions.
internal sealed class AnonymousObserver<T> : IObserver<T>
{
    private readonly Action<T> _onNext;

    public AnonymousObserver(Action<T> onNext) => _onNext = onNext;

    public void OnCompleted() { }

    public void OnError(Exception error) { }

    public void OnNext(T value) => _onNext(value);
}

internal static class PlaceholderExtensions
{
    // Bind a placeholder panel's width to the shared card width so it flexes with
    // the cards it stands in for.
    public static Panel WithWidthBinding(this Panel panel, AlbumGridLayout layout)
    {
        panel[!Layoutable.WidthProperty] = new Avalonia.Data.Binding(nameof(AlbumGridLayout.CardWidth))
        {
            Source = layout,
            Mode = Avalonia.Data.BindingMode.OneWay,
        };
        return panel;
    }
}

using System;
using System.ComponentModel;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Templates;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;

namespace Bae.Desktop;

// A single-column incremental list over the paging core, for the composer and
// artist browse modes: it presents the list's TotalCount as a fixed count of row
// slots so the VirtualizingStackPanel realizes only visible rows, resolves each
// realized row through list.IdAt then the side store, and triggers LoadRange for a
// batch window around itself — the same realize-triggers-load shape the album grid
// uses, minus the multi-column layout, selection, and expansion.
internal sealed class IncrementalListView<TRow> : UserControl
    where TRow : class
{
    private readonly Func<PaginatedList<TRow, string>> _list;
    private readonly Func<string, TRow?> _lookup;
    private readonly Func<TRow, Control> _cell;
    private readonly Func<string> _emptyText;
    private readonly RowSlotFacade _facade;
    private readonly ItemsControl _rows;
    private readonly TextBlock _emptyState;
    private readonly InitialLoadErrorView _errorState;

    private PaginatedList<TRow, string> _bound;

    public IncrementalListView(
        Func<PaginatedList<TRow, string>> list,
        Func<string, TRow?> lookup,
        Func<TRow, Control> cell,
        Func<string> emptyText)
    {
        _list = list;
        _lookup = lookup;
        _cell = cell;
        _emptyText = emptyText;
        _bound = list();

        _facade = new RowSlotFacade(() => _list().TotalCount);
        _rows = new ItemsControl
        {
            ItemsPanel = new FuncTemplate<Panel?>(() => new VirtualizingStackPanel()),
            ItemsSource = _facade,
            ItemTemplate = new FuncDataTemplate<RowSlot>((_, _) => new IncrementalRow<TRow>(_list, _lookup, _cell)),
        };
        var scroller = new ScrollViewer
        {
            Content = _rows,
            HorizontalScrollBarVisibility = Avalonia.Controls.Primitives.ScrollBarVisibility.Disabled,
        };

        _emptyState = new TextBlock
        {
            Text = emptyText(),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            FontSize = 24,
        };
        _emptyState[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        _errorState = new InitialLoadErrorView(() => _list().RetryInitialLoadAsync());

        Content = new Panel { Children = { scroller, _emptyState, _errorState } };
        Subscribe(_bound);
        RenderState();
    }

    // Re-read the empty line and which of the three states is showing, without
    // resetting the realized rows.
    public void Refresh() => RenderState();

    // Re-bind after a swap replaced the list instance.
    public void Rebind()
    {
        _bound.PropertyChanged -= OnListChanged;
        _bound = _list();
        Subscribe(_bound);
        _facade.NotifyReset();
        RenderState();
    }

    private void Subscribe(PaginatedList<TRow, string> list) => list.PropertyChanged += OnListChanged;

    private void OnListChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(PaginatedList<TRow, string>.TotalCount))
        {
            _facade.NotifyReset();
            RenderState();
        }
        else if (e.PropertyName == nameof(PaginatedList<TRow, string>.InitialLoadError))
        {
            RenderState();
        }
    }

    private void RenderState()
    {
        _emptyState.Text = _emptyText();
        var failed = _list().InitialLoadError is not null;
        var empty = !failed && _list().TotalCount == 0;
        _errorState.IsVisible = failed;
        _emptyState.IsVisible = empty;
        _rows.IsVisible = !failed && !empty;
    }
}

// One realized list row: resolves its position through IdAt then the side store,
// triggers LoadRange for a batch window around itself, and re-renders when it
// lands or when the list's generation changes.
internal sealed class IncrementalRow<TRow> : ContentControl
    where TRow : class
{
    private readonly Func<PaginatedList<TRow, string>> _list;
    private readonly Func<string, TRow?> _lookup;
    private readonly Func<TRow, Control> _cell;

    private PaginatedList<TRow, string>? _subscribed;
    private CancellationTokenSource? _loadCts;
    private int _index = -1;

    public IncrementalRow(
        Func<PaginatedList<TRow, string>> list,
        Func<string, TRow?> lookup,
        Func<TRow, Control> cell)
    {
        _list = list;
        _lookup = lookup;
        _cell = cell;
        DataContextChanged += (_, _) => OnRowChanged();
    }

    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        OnRowChanged();
    }

    protected override void OnDetachedFromVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnDetachedFromVisualTree(e);
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
        var list = _list();
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
        if (e.PropertyName == nameof(PaginatedList<TRow, string>.Epoch))
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
        await _list().LoadRangeAsync(offset, LibraryBrowserStore.Page);
        if (!token.IsCancellationRequested)
        {
            Render();
        }
    }

    private void Render()
    {
        var id = _list().IdAt(_index);
        var row = id is null ? null : _lookup(id);
        Content = row is null ? null : _cell(row);
    }
}

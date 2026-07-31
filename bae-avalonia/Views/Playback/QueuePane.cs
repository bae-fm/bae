using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
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

// The queue side pane: a docked right-hand sidebar hosting the manual "Up Next"
// lane and the playback context as one virtualized list, with a fixed header and
// now-playing card above it. Reads the lanes from the playback store and rebuilds
// the list on its QueueChanged while open; a row skips on double-click, removes on
// its remove button or context menu, and each lane clears from its section header.
// The context lane is library-scaled and pages further entries in as the list
// scrolls, revision-checked so a reply for a superseded queue state is dropped.
// Reorder and external album/folder drops land with the drag-drop area; this is the
// display and the direct actions.
internal sealed class QueuePane
{
    private const double Width = 420;

    private readonly AppService _app;
    private readonly Border _host;

    private ContentControl? _listSlot;
    private Border? _card;
    private Image? _cardCover;
    private TextBlock? _cardTitle;
    private TextBlock? _cardArtist;
    private TextBlock? _cardElapsed;
    private ColumnDefinition? _cardFill;
    private ColumnDefinition? _cardRest;

    private NowPlayingBarTrack? _nowPlaying;
    private PlaybackPositionRender? _position;
    private long _renderedSecond = -1;

    // Incremental context paging state for the current open list.
    private ObservableCollection<object>? _rows;
    private ulong _revision;
    private ulong _contextTotal;
    private ulong _contextLoaded;
    private bool _contextLoading;

    public QueuePane(AppService app, Border host)
    {
        _app = app;
        _host = host;
        _host.IsVisible = false;
        _host.Width = Width;
        _host.BorderThickness = new Thickness(1, 0, 0, 0);
        _host[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSurfaceBrush");
        _host[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");

        _app.PlaybackStore.NowPlayingChanged += OnNowPlayingChanged;
        _app.PlaybackStore.PlaybackStopped += OnPlaybackStopped;
        _app.PlaybackStore.PositionChanged += OnPositionChanged;
    }

    public bool IsOpen => _host.IsVisible;

    public void AttachToggle(Button button)
    {
        button.Click += (_, _) => Toggle();
        // A card dropped on the queue button appends the album's tracks to Up Next,
        // whether or not the pane is open.
        EnableQueueDrop(button);
    }

    // Accept album-card drops on a target: on drop, resolve the dropped album ids to
    // tracks and append them to the manual "Up Next" lane. Drops are album-id text
    // (the drag payload); a Files drag falls through to the window's folder import.
    private void EnableQueueDrop(Control target)
    {
        DragDrop.SetAllowDrop(target, true);
        target.AddHandler(DragDrop.DragOverEvent, (_, e) =>
        {
            e.DragEffects = e.Data.Contains(DataFormats.Text) ? DragDropEffects.Copy : DragDropEffects.None;
            e.Handled = true;
        });
        target.AddHandler(DragDrop.DropEvent, (_, e) =>
        {
            if (!e.Data.Contains(DataFormats.Text))
            {
                return;
            }
            e.Handled = true;
            HandleQueueDrop(e.Data.GetText());
        });
    }

    private void HandleQueueDrop(string? payload)
    {
        if (_app.Session.CurrentHandleOrNull() is null || string.IsNullOrEmpty(payload))
        {
            return;
        }
        var albumIds = QueueDragPayload.Decode(payload);
        var (current, resolved) = _app.Library.ResolveToTrackIds(albumIds);
        if (!current)
        {
            return;
        }
        if (resolved.Value is not { } trackIds)
        {
            if (resolved.Error is not null)
            {
                _app.ShowError(Loc.Chrome("error.title"), resolved.Error);
            }
            return;
        }
        var (queueCurrent, error) = _app.Queue.AddToQueue(trackIds);
        if (queueCurrent && error is not null)
        {
            _app.ShowError(Loc.Chrome("error.title"), error);
        }
    }

    public void Toggle()
    {
        if (IsOpen)
        {
            Hide();
        }
        else
        {
            Show();
        }
    }

    public void Hide()
    {
        if (!IsOpen)
        {
            return;
        }
        _app.PlaybackStore.QueueChanged -= Rebuild;
        _host.Child = null;
        _listSlot = null;
        _card = null;
        _host.IsVisible = false;
    }

    public void Show()
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }
        _host.Child = BuildShell();
        Rebuild();
        RenderCard();
        _app.PlaybackStore.QueueChanged += Rebuild;
        _host.IsVisible = true;
    }

    // ── Shell ─────────────────────────────────────────────────────────────────
    private Control BuildShell()
    {
        var shell = new Grid { RowDefinitions = new RowDefinitions("Auto,Auto,*") };
        var header = BuildHeader();
        Grid.SetRow(header, 0);
        shell.Children.Add(header);
        var card = BuildNowPlayingCard();
        Grid.SetRow(card, 1);
        shell.Children.Add(card);
        _listSlot = new ContentControl
        {
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            VerticalContentAlignment = VerticalAlignment.Stretch,
        };
        Grid.SetRow(_listSlot, 2);
        shell.Children.Add(_listSlot);
        return shell;
    }

    private Control BuildHeader()
    {
        var title = new TextBlock
        {
            Text = Loc.Chrome("queue.title"),
            FontSize = 22,
            FontWeight = FontWeight.ExtraBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var close = new Button
        {
            Content = "✕",
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        close[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        Avalonia.Automation.AutomationProperties.SetName(close, Loc.Chrome("action.close"));
        close.Click += (_, _) => Hide();
        var grid = new Grid { Margin = new Thickness(20, 18, 12, 12), ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        Grid.SetColumn(title, 0);
        Grid.SetColumn(close, 1);
        grid.Children.Add(title);
        grid.Children.Add(close);
        return grid;
    }

    // ── Now-playing card ──────────────────────────────────────────────────────
    private Control BuildNowPlayingCard()
    {
        _cardCover = new Image { Stretch = Stretch.UniformToFill };
        var art = new Border { Width = 56, Height = 56, CornerRadius = new CornerRadius(9), ClipToBounds = true, Child = _cardCover, Margin = new Thickness(0, 0, 12, 0) };
        art[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");

        var eyebrow = new TextBlock { Text = Loc.Chrome("queue.now_playing").ToUpper(CultureInfo.CurrentUICulture), FontSize = 9, FontWeight = FontWeight.Bold };
        eyebrow[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        _cardTitle = new TextBlock { FontSize = 14, FontWeight = FontWeight.Bold, MaxLines = 1, TextTrimming = TextTrimming.CharacterEllipsis };
        _cardTitle[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        _cardArtist = new TextBlock { FontSize = 11, FontWeight = FontWeight.Medium, MaxLines = 1, TextTrimming = TextTrimming.CharacterEllipsis };
        _cardArtist[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var text = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(eyebrow);
        text.Children.Add(_cardTitle);
        text.Children.Add(_cardArtist);
        text.Children.Add(BuildCardProgress());

        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*") };
        Grid.SetColumn(art, 0);
        Grid.SetColumn(text, 1);
        row.Children.Add(art);
        row.Children.Add(text);

        _card = new Border
        {
            Margin = new Thickness(14, 0, 14, 6),
            Padding = new Thickness(12),
            CornerRadius = new CornerRadius(12),
            BorderThickness = new Thickness(1),
            IsVisible = false,
            Child = row,
        };
        _card[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        _card[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        return _card;
    }

    private Control BuildCardProgress()
    {
        _cardFill = new ColumnDefinition { Width = new GridLength(0, GridUnitType.Star) };
        _cardRest = new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) };
        var fill = new Border { CornerRadius = new CornerRadius(2) };
        fill[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        var trackGrid = new Grid { ColumnDefinitions = new ColumnDefinitions { _cardFill, _cardRest } };
        Grid.SetColumn(fill, 0);
        trackGrid.Children.Add(fill);
        var track = new Border { Height = 4, CornerRadius = new CornerRadius(2), VerticalAlignment = VerticalAlignment.Center, Child = trackGrid };
        track[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        _cardElapsed = new TextBlock { Text = "0:00", Width = 34, TextAlignment = TextAlignment.Right, FontSize = 10, FontWeight = FontWeight.SemiBold, VerticalAlignment = VerticalAlignment.Center };
        _cardElapsed[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var strip = new Grid { Margin = new Thickness(0, 6, 0, 0), ColumnSpacing = 8, ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        Grid.SetColumn(track, 0);
        Grid.SetColumn(_cardElapsed, 1);
        strip.Children.Add(track);
        strip.Children.Add(_cardElapsed);
        return strip;
    }

    private void OnNowPlayingChanged(NowPlayingBarTrack track)
    {
        _nowPlaying = track;
        RenderCard();
    }

    private void OnPlaybackStopped()
    {
        _nowPlaying = null;
        _position = null;
        RenderCard();
    }

    private void OnPositionChanged(PlaybackPositionRender render)
    {
        _position = render;
        RenderCardPosition(force: false);
    }

    private void RenderCard()
    {
        if (_card is null)
        {
            return;
        }
        if (_nowPlaying is { } track)
        {
            _card.IsVisible = true;
            _cardTitle!.Text = track.Title;
            _cardArtist!.Text = track.Artist;
            _app.Images.Bind(
                _cardCover!,
                ImageContent.ForLibraryImage(track.CoverImage),
                ImageWidths.Row);
            _renderedSecond = -1;
            RenderCardPosition(force: true);
        }
        else
        {
            _card.IsVisible = false;
        }
    }

    private void RenderCardPosition(bool force)
    {
        if (_card is null || _nowPlaying is null || _position is not { } position)
        {
            return;
        }
        var second = (long)(position.PositionMs / 1000);
        if (!force && second == _renderedSecond)
        {
            return;
        }
        _renderedSecond = second;
        var progress = Math.Clamp(position.Progress, 0, 1);
        _cardFill!.Width = new GridLength(progress, GridUnitType.Star);
        _cardRest!.Width = new GridLength(1 - progress, GridUnitType.Star);
        _cardElapsed!.Text = BridgeDisplay.Clock(position.PositionMs);
    }

    // ── List ──────────────────────────────────────────────────────────────────
    private void Rebuild()
    {
        var manual = _app.PlaybackStore.ManualQueue;
        var context = _app.PlaybackStore.Context;
        _revision = _app.PlaybackStore.Revision;

        var rows = new ObservableCollection<object>();
        if (manual.Count == 0)
        {
            rows.Add(new EmptyManualRow());
        }
        else
        {
            rows.Add(new SectionHeaderRow(QueueLane.Manual, Loc.Chrome("queue.section.up_next"), null));
            foreach (var entry in manual)
            {
                rows.Add(new EntryRow(QueueLane.Manual, entry));
            }
        }

        _contextTotal = 0;
        _contextLoaded = 0;
        if (context is { UpcomingTotal: > 0 } ctx)
        {
            rows.Add(new SectionHeaderRow(QueueLane.Context, ContextSectionTitle(ctx), ctx.Shuffled));
            foreach (var entry in ctx.Upcoming)
            {
                rows.Add(new EntryRow(QueueLane.Context, entry));
            }
            _contextTotal = ctx.UpcomingTotal;
            _contextLoaded = (ulong)ctx.Upcoming.Length;
        }

        _rows = rows;
        _listSlot!.Content = BuildList(rows);
    }

    private static string ContextSectionTitle(BridgePlaybackContext context)
    {
        if (context.Kind == BridgePlaybackSourceKind.Library)
        {
            return Loc.Chrome("queue.section.your_library");
        }
        var playingFrom = Loc.Chrome("queue.section.playing_from");
        return string.IsNullOrEmpty(context.SourceTitle) ? playingFrom : $"{playingFrom} · {context.SourceTitle}";
    }

    private Control BuildList(ObservableCollection<object> rows)
    {
        var items = new ItemsControl
        {
            ItemsSource = rows,
            ItemsPanel = new FuncTemplate<Panel?>(() => new VirtualizingStackPanel()),
            ItemTemplate = new FuncDataTemplate<object>((item, _) => BuildRowVisual(item), supportsRecycling: false),
            Margin = new Thickness(12, 4, 12, 16),
        };
        var scroller = new ScrollViewer
        {
            Content = items,
            HorizontalScrollBarVisibility = Avalonia.Controls.Primitives.ScrollBarVisibility.Disabled,
        };
        scroller.ScrollChanged += (_, _) =>
        {
            if (scroller.Offset.Y >= scroller.Extent.Height - scroller.Viewport.Height - 400)
            {
                _ = LoadMoreContext(rows);
            }
        };
        // A card dropped anywhere on the list appends to Up Next (a precise
        // drop-at-index and in-list reorder land with the reorder change).
        EnableQueueDrop(scroller);
        return scroller;
    }

    private async Task LoadMoreContext(ObservableCollection<object> rows)
    {
        if (_contextLoading || _contextLoaded >= _contextTotal || !ReferenceEquals(rows, _rows))
        {
            return;
        }
        _contextLoading = true;
        try
        {
            var (current, result) = await _app.Queue.GetUpcomingPage(checked((uint)_contextLoaded), 100);
            if (!current || !ReferenceEquals(rows, _rows))
            {
                return;
            }
            if (result.Page is not { } page || page.Revision != _revision)
            {
                return;
            }
            foreach (var entry in page.Entries)
            {
                rows.Add(new EntryRow(QueueLane.Context, entry));
            }
            _contextLoaded += (ulong)page.Entries.Length;
        }
        finally
        {
            _contextLoading = false;
        }
    }

    private Control BuildRowVisual(object? row) => row switch
    {
        SectionHeaderRow header => BuildSectionHeader(header),
        EntryRow entry => BuildEntryRow(entry),
        EmptyManualRow => BuildEmptyDropArea(),
        _ => new Control(),
    };

    private Control BuildSectionHeader(SectionHeaderRow header)
    {
        var label = new TextBlock
        {
            Text = header.Text.ToUpper(CultureInfo.CurrentUICulture),
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var controls = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center, Spacing = 4 };
        controls.Children.Add(ClearLaneButton(header.Lane));
        if (header.Shuffled is bool shuffled)
        {
            controls.Children.Add(ShuffleToggle(shuffled));
        }
        var grid = new Grid { Margin = new Thickness(8, 12, 8, 4), ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        Grid.SetColumn(label, 0);
        Grid.SetColumn(controls, 1);
        grid.Children.Add(label);
        grid.Children.Add(controls);
        return grid;
    }

    private Button ClearLaneButton(QueueLane lane)
    {
        var (nameKey, clear) = lane switch
        {
            QueueLane.Manual => ("queue.clear_up_next", (Action)(() => _app.Queue.ClearUpNext())),
            _ => ("queue.clear_playing_from", (Action)(() => _app.Queue.ClearPlayingFrom())),
        };
        var button = new Button
        {
            Content = Loc.Chrome("queue.clear"),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Padding = new Thickness(6, 4),
            VerticalAlignment = VerticalAlignment.Center,
        };
        button[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        Avalonia.Automation.AutomationProperties.SetName(button, Loc.Chrome(nameKey));
        button.Click += (_, _) => clear();
        return button;
    }

    private Button ShuffleToggle(bool shuffled)
    {
        var glyph = Icons.Glyph(Icons.Shuffle, 12, shuffled ? "BaeAccentBrush" : "BaeTextSecondaryBrush");
        var toggle = new Button
        {
            Content = glyph,
            Width = 28,
            Height = 28,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(7),
            BorderThickness = new Thickness(0),
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
        };
        toggle[!Button.BackgroundProperty] = new DynamicResourceExtension(shuffled ? "BaeSelectionTintBrush" : "BaeElevatedBrush");
        Avalonia.Automation.AutomationProperties.SetName(toggle, Loc.Chrome(shuffled ? "queue.shuffle.off" : "queue.shuffle.on"));
        toggle.Click += (_, _) => _app.Queue.SetShuffle(!shuffled);
        return toggle;
    }

    private Control BuildEntryRow(EntryRow row)
    {
        var cover = new Image { Stretch = Stretch.UniformToFill };
        _app.Images.Bind(
            cover,
            ImageContent.ForLibraryImage(row.Entry.CoverImage),
            ImageWidths.Row);
        var art = new Border { Width = 44, Height = 44, CornerRadius = new CornerRadius(8), ClipToBounds = true, Child = cover };
        art[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");

        var title = new TextBlock { Text = row.Entry.Title, FontSize = 13, FontWeight = FontWeight.SemiBold, MaxLines = 1, TextTrimming = TextTrimming.CharacterEllipsis };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var album = new TextBlock { Text = row.Entry.AlbumTitle, FontSize = 11, FontWeight = FontWeight.Medium, MaxLines = 1, TextTrimming = TextTrimming.CharacterEllipsis };
        album[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var textColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        textColumn.Children.Add(title);
        textColumn.Children.Add(album);

        var duration = new TextBlock { Text = BridgeDisplay.Clock(row.Entry.DurationClock), FontSize = 11, FontWeight = FontWeight.SemiBold, VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Right };
        duration[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var remove = new Button
        {
            Content = "✕",
            Width = 28,
            Height = 28,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(7),
            BorderThickness = new Thickness(0),
            Background = Brushes.Transparent,
            Opacity = 0,
            IsHitTestVisible = false,
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        remove[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        Avalonia.Automation.AutomationProperties.SetName(remove, Loc.Chrome("queue.remove_item"));
        remove.Click += (_, _) => RemoveEntry(row);
        var trailing = new Grid { VerticalAlignment = VerticalAlignment.Center };
        trailing.Children.Add(duration);
        trailing.Children.Add(remove);

        var grid = new Grid { ColumnSpacing = 12, VerticalAlignment = VerticalAlignment.Center, ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto") };
        Grid.SetColumn(art, 0);
        Grid.SetColumn(textColumn, 1);
        Grid.SetColumn(trailing, 2);
        grid.Children.Add(art);
        grid.Children.Add(textColumn);
        grid.Children.Add(trailing);

        var hover = new Border { Padding = new Thickness(8, 6), Margin = new Thickness(0, 1), CornerRadius = new CornerRadius(10), Background = Brushes.Transparent, Child = grid };
        hover.PointerEntered += (_, _) =>
        {
            hover[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
            duration.Opacity = 0;
            remove.Opacity = 1;
            remove.IsHitTestVisible = true;
        };
        hover.PointerExited += (_, _) =>
        {
            hover.Background = Brushes.Transparent;
            duration.Opacity = 1;
            remove.Opacity = 0;
            remove.IsHitTestVisible = false;
        };
        // Double-click skips playback to this entry.
        hover.DoubleTapped += (_, _) => _app.Queue.SkipToEntry(row.Entry.EntryId);
        hover.ContextMenu = new ContextMenu { ItemsSource = new[] { RemoveMenuItem(row) } };
        EnableRowReorder(hover, row);
        return hover;
    }

    // The private data format a queue-row reorder drag carries (the moved entry id),
    // distinct from an album-card drop's plain text so a row drop can tell them
    // apart. Reorder stays within one lane and forwards a move-before to core.
    private const string ReorderFormat = "application/x-bae-queue-reorder";

    // Make a queue row both a reorder drag source and a same-lane drop target: a
    // press that moves past a threshold drags the row (carrying its entry id and
    // lane); dropping it on another row of the same lane moves it before that row.
    private void EnableRowReorder(Control hover, EntryRow row)
    {
        Point? pressAt = null;
        var dragging = false;
        hover.PointerPressed += (_, e) =>
        {
            if (e.GetCurrentPoint(hover).Properties.IsLeftButtonPressed)
            {
                pressAt = e.GetPosition(hover);
                dragging = false;
            }
        };
        hover.PointerMoved += async (_, e) =>
        {
            if (pressAt is not { } start || dragging || !e.GetCurrentPoint(hover).Properties.IsLeftButtonPressed)
            {
                return;
            }
            var now = e.GetPosition(hover);
            if (Math.Abs(now.X - start.X) < 6 && Math.Abs(now.Y - start.Y) < 6)
            {
                return;
            }
            dragging = true;
            var data = new DataObject();
            data.Set(ReorderFormat, row.Entry.EntryId + "\n" + (int)row.Lane);
            await DragDrop.DoDragDrop(e, data, DragDropEffects.Move);
        };
        hover.PointerReleased += (_, _) => pressAt = null;

        DragDrop.SetAllowDrop(hover, true);
        hover.AddHandler(DragDrop.DragOverEvent, (_, e) =>
        {
            e.DragEffects = IsSameLaneReorder(e, row.Lane) ? DragDropEffects.Move : e.DragEffects;
            if (IsSameLaneReorder(e, row.Lane))
            {
                e.Handled = true;
            }
        });
        hover.AddHandler(DragDrop.DropEvent, (_, e) =>
        {
            if (!IsSameLaneReorder(e, row.Lane) || e.Data.Get(ReorderFormat) is not string payload)
            {
                return;
            }
            e.Handled = true;
            var movedId = payload.Split('\n')[0];
            if (movedId != row.Entry.EntryId)
            {
                // Move the dragged entry to just before this row.
                _app.Queue.ReorderEntry(movedId, row.Entry.EntryId);
            }
        });
    }

    private static bool IsSameLaneReorder(DragEventArgs e, QueueLane lane) =>
        e.Data.Get(ReorderFormat) is string payload
        && payload.Split('\n') is { Length: 2 } parts
        && int.TryParse(parts[1], out var laneOrdinal)
        && laneOrdinal == (int)lane;

    private MenuItem RemoveMenuItem(EntryRow row)
    {
        var item = new MenuItem { Header = Loc.Chrome("queue.remove_item") };
        item.Click += (_, _) => RemoveEntry(row);
        return item;
    }

    private void RemoveEntry(EntryRow row)
    {
        _app.Queue.RemoveEntry(row.Entry.EntryId);
        _rows?.Remove(row);
    }

    private Control BuildEmptyDropArea()
    {
        var stack = new StackPanel { Spacing = 6, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
        var title = new TextBlock { Text = Loc.Chrome("queue.empty.title"), HorizontalAlignment = HorizontalAlignment.Center, FontWeight = FontWeight.SemiBold };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var hint = new TextBlock { Text = Loc.Chrome("queue.empty.hint"), HorizontalAlignment = HorizontalAlignment.Center, TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center };
        hint[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        stack.Children.Add(title);
        stack.Children.Add(hint);
        return new Border { MinHeight = 120, Padding = new Thickness(16), Child = stack };
    }

    // ── Row model ─────────────────────────────────────────────────────────────
    private enum QueueLane { Manual, Context }

    private sealed record SectionHeaderRow(QueueLane Lane, string Text, bool? Shuffled);

    private sealed record EntryRow(QueueLane Lane, BridgeQueueEntry Entry);

    private sealed record EmptyManualRow;
}

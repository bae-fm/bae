using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.UI;

namespace Bae.Windows;

// The virtualized queue list: the ListView, its per-row visuals, the section
// headers, and the reorder/remove wiring. Split out of QueuePane.cs unchanged.
internal sealed partial class QueuePane
{
    // The single virtualized list hosting every row: both section headers and both
    // lanes' entries. Fills the pane's remaining height (its slot's Grid row is
    // Star-sized) so it is its own bounded scroll region — there is no outer
    // ScrollViewer.
    private ListView BuildQueueList(QueuePaneRowCollection rows, IReadOnlyList<BridgeQueueEntry> manual)
    {
        var list = new ListView
        {
            ItemsSource = rows,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            CanReorderItems = true,
            CanDragItems = true,
            AllowDrop = true,
            Padding = new Thickness(12, 4, 12, 16),
            IncrementalLoadingTrigger = IncrementalLoadingTrigger.Edge,
        };

        // The custom rows carry their own hover fill; neutralize the container's
        // default pointer/selection backgrounds so the two don't stack, and strip
        // its padding/min-height so a row is exactly its content.
        foreach (var key in new[]
        {
            "ListViewItemBackgroundPointerOver",
            "ListViewItemBackgroundPressed",
            "ListViewItemBackgroundSelected",
            "ListViewItemBackgroundSelectedPointerOver",
            "ListViewItemBackgroundSelectedPressed",
        })
        {
            list.Resources[key] = new SolidColorBrush(Colors.Transparent);
        }
        var itemStyle = new Style(typeof(ListViewItem));
        itemStyle.Setters.Add(new Setter(Control.PaddingProperty, new Thickness(0)));
        itemStyle.Setters.Add(new Setter(FrameworkElement.MinHeightProperty, 0.0));
        itemStyle.Setters.Add(new Setter(Control.HorizontalContentAlignmentProperty, HorizontalAlignment.Stretch));
        list.ItemContainerStyle = itemStyle;

        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue)
            {
                return;
            }
            args.ItemContainer.Content = BuildRowVisual(args.Item as QueueRow, rows);
            args.Handled = true;
        };

        // Only an EntryRow is draggable — headers, the empty state, and the
        // trailing drop strip are chrome, not queue content.
        list.DragItemsStarting += (_, args) =>
        {
            if (args.Items.Any(item => item is not EntryRow))
            {
                args.Cancel = true;
            }
        };

        list.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is EntryRow clicked)
            {
                _queueService.SkipToEntry(clicked.EntryId);
            }
        };

        // Right-tap a row to drop it from the queue. Removing locally too keeps
        // the pane in sync; the Move-only reorder handler below ignores this
        // Remove (it only reacts to NotifyCollectionChangedAction.Move).
        list.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element
                || element.DataContext is not EntryRow item)
            {
                return;
            }

            if (rows.IndexOf(item) < 0)
            {
                return;
            }

            var menu = new MenuFlyout();
            var remove = new MenuFlyoutItem { Text = Loc.Chrome("queue.remove_item") };
            remove.Click += (_, _) => RemoveEntry(rows, item);
            menu.Items.Add(remove);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        // A drag-reorder mutates `rows` in place before this fires (the framework's
        // own behavior); validate it stayed within one lane before forwarding to
        // core, and revert (a full Rebuild, since core's state never changed) if
        // it crossed a section boundary — dragging past a header is visually
        // possible but not a valid move.
        rows.CollectionChanged += (_, args) =>
        {
            if (args.Action != NotifyCollectionChangedAction.Move)
            {
                return;
            }
            if (rows[args.NewStartingIndex] is not EntryRow moved)
            {
                return;
            }
            if (!IsLaneOrderValid(rows))
            {
                BaeDiagnostics.Logger.Warning(
                    $"Queue reorder for {moved.EntryId} crossed a section boundary; reverting.");
                Rebuild();
                return;
            }

            var laneEntryIds = rows.OfType<EntryRow>().Where(e => e.Lane == moved.Lane)
                .Select(e => e.EntryId).ToList();
            var newIndex = laneEntryIds.IndexOf(moved.EntryId);
            var move = QueueReorderModel.ResolveMove(laneEntryIds, newIndex);
            _queueService.ReorderEntry(move.MovedEntryId, move.BeforeEntryId);
        };

        // External album-card drops land only in the manual lane; the pointer's Y
        // position resolves to an insert index among the manual lane's realized
        // rows (or the lane's end when it's empty or the pointer is past them).
        AttachExternalDrop(list, e => ComputeInsertIndex(list, rows, manual, e));

        return list;
    }

    // Remove one entry from core and from the live collection. The bridge removes
    // by entry id, so a reorder between the gesture and this can't target the wrong
    // row; the local index only keeps the pane's collection in sync.
    private void RemoveEntry(QueuePaneRowCollection rows, EntryRow item)
    {
        var index = rows.IndexOf(item);
        if (index < 0)
        {
            return;
        }
        _queueService.RemoveEntry(item.EntryId);
        rows.RemoveAt(index);
    }

    // Whether `rows` still partitions into contiguous same-lane runs in
    // Manual/Context ordinal order — the invariant a valid single-lane reorder
    // always preserves. A cross-lane drag breaks it (some row's ordinal would
    // regress relative to an earlier row's).
    private static bool IsLaneOrderValid(IReadOnlyList<QueueRow> rows)
    {
        var lastOrdinal = -1;
        foreach (var row in rows)
        {
            var ordinal = (int)row.Lane;
            if (ordinal < lastOrdinal)
            {
                return false;
            }
            lastOrdinal = ordinal;
        }
        return true;
    }

    // Build one row's visual by kind. ContainerContentChanging re-fires per
    // recycled container, so this runs again whenever a row's content changes
    // (including a fresh page landing).
    private FrameworkElement BuildRowVisual(QueueRow? row, QueuePaneRowCollection rows) => row switch
    {
        SectionHeaderRow header => QueueSectionLabel(header),
        EntryRow entry => BuildEntryRow(entry, rows),
        EmptyManualRow => BuildEmptyDropArea(),
        TrailingDropRow => BuildTrailingDropArea(),
        _ => throw new ArgumentOutOfRangeException(nameof(row), row, "Unknown queue row kind"),
    };

    // The empty manual-lane state: a heading and a hint centered inside a
    // drop-accepting card; a drop lands at the front of the (empty) lane.
    private Border BuildEmptyDropArea()
    {
        var stack = new StackPanel
        {
            Spacing = 6,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        stack.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("queue.empty.title"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontWeight = FontWeights.SemiBold,
        });
        stack.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("queue.empty.hint"),
            HorizontalAlignment = HorizontalAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
            TextAlignment = TextAlignment.Center,
            Foreground = Secondary,
        });
        return new Border
        {
            MinHeight = 120,
            Padding = new Thickness(16),
            Child = stack,
        };
    }

    // A drop strip below the manual lane's rows that appends at the lane's end,
    // mirroring the reference platforms' trailing zone.
    private static Border BuildTrailingDropArea() => new()
    {
        Height = 18,
        Background = new SolidColorBrush(Colors.Transparent),
    };

    // A lane's section header: a small, wide-tracked, uppercase label, the Clear
    // that empties THIS lane, and — on the context lane only — a shuffle toggle
    // that flips its order while the current track keeps playing. A header is
    // emitted only for a lane that has rows, so its Clear always has something
    // to clear.
    private Grid QueueSectionLabel(SectionHeaderRow header)
    {
        var label = new TextBlock
        {
            Text = header.Text.ToUpper(CultureInfo.CurrentUICulture),
            FontSize = 10,
            FontWeight = FontWeights.Bold,
            CharacterSpacing = 120,
            Foreground = Secondary,
            TextTrimming = TextTrimming.CharacterEllipsis,
            MaxLines = 1,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var controls = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            VerticalAlignment = VerticalAlignment.Center,
        };
        controls.Children.Add(ClearLaneButton(header.Lane));
        if (header.Shuffled is bool shuffled)
        {
            controls.Children.Add(ShuffleToggle(shuffled));
        }

        var row = new Grid { Margin = new Thickness(8, 12, 8, 4) };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(label, 0);
        Grid.SetColumn(controls, 1);
        row.Children.Add(label);
        row.Children.Add(controls);
        return row;
    }

    // A lane's Clear. The visible word stays "clear" — the label beside it already
    // names the lane — while the tooltip and the automation name spell out which
    // lane it empties, for anyone reading the button on its own.
    private Button ClearLaneButton(QueueLane lane)
    {
        var (nameKey, clear) = lane switch
        {
            QueueLane.Manual => ("queue.clear_up_next", (Action)(() => _queueService.ClearUpNext())),
            QueueLane.Context => ("queue.clear_playing_from", (Action)(() => _queueService.ClearPlayingFrom())),
            _ => throw new ArgumentOutOfRangeException(nameof(lane), lane, "Unknown queue lane"),
        };
        var button = new Button
        {
            Content = Loc.Chrome("queue.clear"),
            Background = new SolidColorBrush(Colors.Transparent),
            BorderThickness = new Thickness(0),
            Foreground = Secondary,
            FontSize = 10,
            FontWeight = FontWeights.Bold,
            Padding = new Thickness(6, 4, 6, 4),
            VerticalAlignment = VerticalAlignment.Center,
        };
        AutomationName(button, nameKey);
        button.Click += (_, _) => clear();
        return button;
    }

    // Segoe MDL2 Assets "Shuffle" glyph (U+E8B1): accent on a soft-accent fill
    // when on, secondary when off.
    private Button ShuffleToggle(bool shuffled)
    {
        var toggle = new Button
        {
            Content = new FontIcon
            {
                Glyph = "",
                FontSize = 12,
                Foreground = shuffled ? Accent : Secondary,
            },
            Width = 28,
            Height = 28,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(7),
            BorderThickness = new Thickness(0),
            Background = shuffled ? AccentSoftFill() : new SolidColorBrush(Colors.Transparent),
            VerticalAlignment = VerticalAlignment.Center,
        };
        AutomationName(toggle, shuffled ? "queue.shuffle.off" : "queue.shuffle.on");
        toggle.Click += (_, _) => _queueService.SetShuffle(!shuffled);
        return toggle;
    }

    // A queue entry row: cover, title/album, and a trailing slot that swaps the
    // duration for a remove button on hover. The swap is opacity-only in a
    // fixed-size slot, so the row never resizes on hover.
    private Border BuildEntryRow(EntryRow row, QueuePaneRowCollection rows)
    {
        var cover = new Image { Stretch = Stretch.UniformToFill };
        CoverImage.BindById(cover, _mediaPaths, row.Entry.CoverImageId);
        var art = new Border
        {
            Width = 44,
            Height = 44,
            CornerRadius = new CornerRadius(8),
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = cover,
        };

        var title = new TextBlock
        {
            Text = row.Entry.Title,
            FontSize = 13,
            FontWeight = FontWeights.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        var album = new TextBlock
        {
            Text = row.Entry.AlbumTitle,
            FontSize = 11,
            FontWeight = FontWeights.Medium,
            Foreground = Secondary,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        var textColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        textColumn.Children.Add(title);
        textColumn.Children.Add(album);

        var duration = new TextBlock
        {
            Text = BridgeDisplay.Clock(row.Entry.DurationClock),
            FontSize = 11,
            FontWeight = FontWeights.SemiBold,
            Foreground = Secondary,
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        var removeGlyph = new FontIcon { Glyph = "\uE711", FontSize = 11, Foreground = Secondary };
        var remove = new Button
        {
            Content = removeGlyph,
            Width = 28,
            Height = 28,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(7),
            BorderThickness = new Thickness(0),
            Background = new SolidColorBrush(Colors.Transparent),
            Opacity = 0,
            IsHitTestVisible = false,
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        AutomationName(remove, "queue.remove_item");
        remove.PointerEntered += (_, _) =>
        {
            removeGlyph.Foreground = Accent;
            remove.Background = AccentSoftFill();
        };
        remove.PointerExited += (_, _) =>
        {
            removeGlyph.Foreground = Secondary;
            remove.Background = new SolidColorBrush(Colors.Transparent);
        };
        remove.Click += (_, _) => RemoveEntry(rows, row);

        var trailing = new Grid { VerticalAlignment = VerticalAlignment.Center };
        trailing.Children.Add(duration);
        trailing.Children.Add(remove);

        var grid = new Grid { ColumnSpacing = 12, VerticalAlignment = VerticalAlignment.Center };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(art, 0);
        Grid.SetColumn(textColumn, 1);
        Grid.SetColumn(trailing, 2);
        grid.Children.Add(art);
        grid.Children.Add(textColumn);
        grid.Children.Add(trailing);

        var hover = new Border
        {
            Padding = new Thickness(8, 6, 8, 6),
            Margin = new Thickness(0, 1, 0, 1),
            CornerRadius = new CornerRadius(10),
            Background = new SolidColorBrush(Colors.Transparent),
            Child = grid,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(hover, $"{row.Entry.Title}, {row.Entry.AlbumTitle}");
        hover.PointerEntered += (_, _) =>
        {
            hover.Background = ForegroundWash(0.06);
            duration.Opacity = 0;
            remove.Opacity = 1;
            remove.IsHitTestVisible = true;
        };
        hover.PointerExited += (_, _) =>
        {
            hover.Background = new SolidColorBrush(Colors.Transparent);
            duration.Opacity = 1;
            remove.Opacity = 0;
            remove.IsHitTestVisible = false;
        };
        return hover;
    }
}
